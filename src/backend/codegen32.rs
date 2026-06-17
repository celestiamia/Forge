//! IR to 32-bit native machine code translator.
//!
//! For the hosted x86_32 target the translator targets the IA-32 cdecl ABI:
//!   - arguments passed on the stack right-to-left,
//!   - return value in EAX,
//!   - stack frame is `push ebp; mov ebp, esp; sub esp, N`.
//!
//! Linux system calls use `int 0x80` with the call number in EAX.

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::backend::ir::{BinOp, Expr, ExprKind, Func, LValue, Literal, Program, Stmt, StructDef, Type};
use crate::backend::x86::{Assembler, Cond, Mem, Reg};
use crate::obj::elf32::Elf32Writer;
use crate::obj::ObjectWriter;

const BASE_VADDR: u32 = 0x08048000;
const EHDR_SIZE: u32 = 52;
const PHDR_SIZE: u32 = 32;

#[derive(Clone, Debug)]
struct Slot {
    offset: i32,
    #[allow(dead_code)]
    size: usize,
}

#[derive(Clone, Debug)]
struct StructLayout {
    size: usize,
    align: usize,
    offsets: Vec<usize>,
}

pub struct CodeGen<'p> {
    prog: &'p Program,
    asm: Assembler,
    label_offsets: HashMap<u32, usize>,
    func_labels: HashMap<String, u32>,
    string_labels: HashMap<String, u32>,

    locals: HashMap<String, Slot>,
    frame_size: usize,
    struct_layouts: HashMap<String, StructLayout>,
    addr_tmp: i32,
    ret_label: u32,
    string_patches: Vec<(usize, u32)>,
    /// Code offset of the `mov eax, <bump_ptr_vaddr>` immediate in `_dev_alloc`,
    /// patched once the `.data` segment layout is known.  `None` when the program
    /// does not use the bump allocator.
    alloc_ptr_patch: Option<usize>,
    loop_end_stack: Vec<u32>,
    loop_head_stack: Vec<u32>,
}

pub fn compile_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    let mut cg = CodeGen::new(prog);

    // Reserve labels for every user function and the runtime helpers.
    for f in &prog.funcs {
        let lab = cg.asm.new_label();
        cg.func_labels.insert(f.name.clone(), lab);
    }
    let start_label = if prog.hosted {
        let l = cg.asm.new_label();
        cg.func_labels.insert("_start".to_string(), l);
        let p = cg.asm.new_label();
        cg.func_labels.insert("_dev_puts".to_string(), p);
        let pc = cg.asm.new_label();
        cg.func_labels.insert("_dev_putchar".to_string(), pc);
        let c = cg.asm.new_label();
        cg.func_labels.insert("_dev_getchar".to_string(), c);
        let r = cg.asm.new_label();
        cg.func_labels.insert("_dev_rand".to_string(), r);
        let e = cg.asm.new_label();
        cg.func_labels.insert("_dev_exit".to_string(), e);
        let lb = cg.asm.new_label();
        cg.func_labels.insert("_dev_lfence".to_string(), lb);
        let sb = cg.asm.new_label();
        cg.func_labels.insert("_dev_sfence".to_string(), sb);
        let mb = cg.asm.new_label();
        cg.func_labels.insert("_dev_mfence".to_string(), mb);
        // The bump allocator helpers are only emitted when the program
        // actually imports `std.alloc` (i.e. declares the extern helpers).
        let need_alloc = prog
            .externs
            .iter()
            .any(|e| e.name == "_dev_alloc" || e.name == "_dev_free");
        if need_alloc {
            let a = cg.asm.new_label();
            cg.func_labels.insert("_dev_alloc".to_string(), a);
            let f = cg.asm.new_label();
            cg.func_labels.insert("_dev_free".to_string(), f);
        }
        l
    } else {
        *cg.func_labels
            .get("_start")
            .ok_or_else(|| anyhow::anyhow!("freestanding mode requires a _start function"))?
    };

    // Emit user functions.
    for f in &prog.funcs {
        cg.emit_func(f)?;
    }

    // Emit tiny hosted runtime.
    if prog.hosted {
        cg.emit_runtime(start_label)?;
    }

    // Append string literals as rodata at the end of the assembler buffer.
    let rodata_start = cg.asm.new_label();
    cg.bind_label(rodata_start);
    let mut strings: Vec<(String, u32)> = cg.string_labels.drain().collect();
    strings.sort_by_key(|(_, lab)| *lab);
    for (s, lab) in strings {
        cg.bind_label(lab);
        cg.asm.append_bytes(s.as_bytes());
        cg.asm.push_byte(0);
    }

    let bytes = cg.asm.into_bytes();
    let split = *cg.label_offsets.get(&rodata_start).unwrap_or(&bytes.len());
    let mut code = bytes[..split].to_vec();
    let rodata = bytes[split..].to_vec();

    let text_offset = EHDR_SIZE + PHDR_SIZE * 2;
    let entry_vaddr = BASE_VADDR
        + text_offset
        + *cg.label_offsets.get(&start_label).unwrap_or(&0) as u32;

    // Patch absolute virtual addresses for string literals.
    for (patch_off, label) in &cg.string_patches {
        let label_off = *cg.label_offsets.get(label).unwrap_or(&0) as u32;
        let abs = BASE_VADDR + text_offset + label_off;
        code[*patch_off..*patch_off + 4].copy_from_slice(&abs.to_le_bytes());
    }

    // Build the writable `.data` segment.  When the program uses the bump
    // allocator, a 4-byte pointer (initialized to the base of a 64 KiB `.bss`
    // arena) lives here and its absolute address is patched into `_dev_alloc`.
    const PAGE_SIZE_32: u32 = 0x1000;
    let first_seg_end = text_offset + code.len() as u32 + rodata.len() as u32;
    let data_offset = align_up_u32(first_seg_end, PAGE_SIZE_32);
    let data_vaddr = BASE_VADDR + data_offset;

    let mut data: Vec<u8> = Vec::new();
    let mut bss_size: u32 = 0;
    if let Some(alloc_patch) = cg.alloc_ptr_patch {
        const ARENA_SIZE: u32 = 64 * 1024;
        let bump_data_off = data.len() as u32;
        data.extend_from_slice(&[0u8; 4]); // placeholder for the bump pointer
        let bump_ptr_vaddr = data_vaddr + bump_data_off;
        code[alloc_patch..alloc_patch + 4].copy_from_slice(&bump_ptr_vaddr.to_le_bytes());
        // `.bss` starts right after `.data` in the virtual address space.
        let arena_base = data_vaddr + data.len() as u32;
        data[bump_data_off as usize..bump_data_off as usize + 4]
            .copy_from_slice(&arena_base.to_le_bytes());
        bss_size = ARENA_SIZE;
    }

    Ok(Box::new(Elf32Writer::new(code, rodata, data, bss_size, entry_vaddr)))
}

impl<'p> CodeGen<'p> {
    fn new(prog: &'p Program) -> Self {
        let mut layouts = HashMap::new();
        for s in &prog.structs {
            layouts.insert(s.name.clone(), layout_struct(s));
        }

        Self {
            prog,
            asm: Assembler::new(),
            label_offsets: HashMap::new(),
            func_labels: HashMap::new(),
            string_labels: HashMap::new(),
            locals: HashMap::new(),
            frame_size: 0,
            struct_layouts: layouts,
            addr_tmp: 0,
            ret_label: 0,
            string_patches: Vec::new(),
            alloc_ptr_patch: None,
            loop_end_stack: Vec::new(),
            loop_head_stack: Vec::new(),
        }
    }

    fn bind_label(&mut self, lab: u32) {
        let off = self.asm.len();
        self.asm.bind(lab);
        self.label_offsets.insert(lab, off);
    }

    fn emit_func(&mut self, f: &Func) -> Result<()> {
        self.locals.clear();
        self.frame_size = 0;
        let entry = *self.func_labels.get(&f.name).unwrap();
        self.bind_label(entry);

        // Prologue.
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        let sub_imm_offset = self.asm.len() + 2; // opcode + modrm
        self.asm.sub(Reg::Esp, 0i32);

        // Reserve an address-scratch slot used by assignments.
        let slot = self.alloc_slot(4, 4);
        self.addr_tmp = slot.offset;

        // Allocate slots for parameters and spill them from the cdecl stack.
        for (name, _ty) in &f.params {
            self.alloc_named_slot(name, 4, 4);
        }
        for (i, (name, _ty)) in f.params.iter().enumerate() {
            let slot = self.locals.get(name).unwrap();
            self.asm
                .mov(Reg::Eax, Mem::base_disp(Reg::Ebp, (8 + i * 4) as i32));
            self.asm
                .mov(Mem::base_disp(Reg::Ebp, slot.offset), Reg::Eax);
        }

        self.ret_label = self.asm.new_label();

        // Body.
        for s in &f.body {
            self.emit_stmt(s)?;
        }

        // Implicit epilogue for fall-through.
        self.bind_label(self.ret_label);
        self.asm.leave();
        self.asm.ret();

        // Patch the sub esp, imm32 with the aligned frame size.
        let frame = align_up(self.frame_size, 16);
        self.asm.patch_i32(sub_imm_offset, frame as i32);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    fn emit_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, ty, init } => {
                let slot = self.alloc_named_slot(name, 4, 4);
                if let Some(e) = init {
                    self.eval_expr(e)?;
                    self.store_scalar(slot.offset);
                }
                let _ = ty;
            }
            Stmt::StackAlloc { name, elem_ty, count } => {
                let elem_size = elem_ty.byte_size();
                let raw_size = elem_size * *count;
                let align = elem_size.max(1);
                let raw_slot = self.alloc_slot(raw_size, align);
                let ptr_slot = self.alloc_named_slot(name, 4, 4);
                self.asm
                    .lea(Reg::Eax, Mem::base_disp(Reg::Ebp, raw_slot.offset));
                self.store_scalar(ptr_slot.offset);
            }
            Stmt::Assign { lhs, rhs } => {
                self.lvalue_addr(lhs)?; // address in EAX
                self.asm
                    .mov(Mem::base_disp(Reg::Ebp, self.addr_tmp), Reg::Eax);
                self.eval_expr(rhs)?; // value in EAX
                self.asm
                    .mov(Reg::Edx, Mem::base_disp(Reg::Ebp, self.addr_tmp));
                let width = self.lvalue_store_width(lhs);
                self.store_width(width, Reg::Edx, Reg::Eax);
            }
            Stmt::Return(None) => {
                self.asm.jmp(self.ret_label);
            }
            Stmt::Return(Some(e)) => {
                self.eval_expr(e)?;
                self.asm.jmp(self.ret_label);
            }
            Stmt::Expr(e) => {
                self.eval_expr(e)?;
            }
            Stmt::If { cond, then, else_ } => {
                let then_lab = self.asm.new_label();
                let end_lab = self.asm.new_label();
                let else_lab = if else_.is_some() {
                    Some(self.asm.new_label())
                } else {
                    None
                };
                self.eval_expr(cond)?;
                self.asm.test(Reg::Eax, Reg::Eax);
                if let Some(l) = else_lab {
                    self.asm.je(l);
                } else {
                    self.asm.je(end_lab);
                }
                self.bind_label(then_lab);
                for st in then {
                    self.emit_stmt(st)?;
                }
                self.asm.jmp(end_lab);
                if let Some(l) = else_lab {
                    self.bind_label(l);
                    for st in else_.as_ref().unwrap() {
                        self.emit_stmt(st)?;
                    }
                }
                self.bind_label(end_lab);
            }
            Stmt::While { cond, body } => {
                let head = self.asm.new_label();
                let end = self.asm.new_label();
                self.loop_head_stack.push(head);
                self.loop_end_stack.push(end);
                self.bind_label(head);
                self.eval_expr(cond)?;
                self.asm.test(Reg::Eax, Reg::Eax);
                self.asm.je(end);
                for st in body {
                    self.emit_stmt(st)?;
                }
                self.asm.jmp(head);
                self.bind_label(end);
                self.loop_head_stack.pop();
                self.loop_end_stack.pop();
            }
            Stmt::For { init, cond, step, body } => {
                if let Some(i) = init {
                    self.emit_stmt(i)?;
                }
                let head = self.asm.new_label();
                let end = self.asm.new_label();
                self.loop_head_stack.push(head);
                self.loop_end_stack.push(end);
                self.bind_label(head);
                self.eval_expr(cond)?;
                self.asm.test(Reg::Eax, Reg::Eax);
                self.asm.je(end);
                for st in body {
                    self.emit_stmt(st)?;
                }
                if let Some(st) = step {
                    self.eval_expr(st)?;
                }
                self.asm.jmp(head);
                self.bind_label(end);
                self.loop_head_stack.pop();
                self.loop_end_stack.pop();
            }
            Stmt::Break => {
                let end = *self.loop_end_stack.last()
                    .ok_or_else(|| anyhow::anyhow!("break outside of loop"))?;
                self.asm.jmp(end);
            }
            Stmt::Continue => {
                let head = *self.loop_head_stack.last()
                    .ok_or_else(|| anyhow::anyhow!("continue outside of loop"))?;
                self.asm.jmp(head);
            }
            Stmt::Unsafe(b) => {
                for st in b {
                    self.emit_stmt(st)?;
                }
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    fn eval_expr(&mut self, e: &Expr) -> Result<()> {
        match &e.kind {
            ExprKind::Lit(lit) => match lit {
                Literal::Int(v) => self.asm.mov(Reg::Eax, *v as i32),
                Literal::Bool(v) => self.asm.mov(Reg::Eax, if *v { 1i32 } else { 0i32 }),
                Literal::Char(v) => self.asm.mov(Reg::Eax, *v as i32),
                Literal::String(s) => {
                    let lab = self.string_label(s);
                    let patch_off = self.asm.len() + 2; // C7 /0 imm32, offset of imm32
                    self.asm.mov(Reg::Eax, 0i32);
                    self.string_patches.push((patch_off, lab));
                }
                Literal::Float(_) => bail!("floating point is not implemented in the x86_32 backend"),
                Literal::Null => self.asm.mov(Reg::Eax, 0i32),
            },
            ExprKind::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("unknown variable: {}", name))?;
                match &e.ty {
                    Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32 | Type::Bool | Type::Char => {
                        self.asm
                            .lea(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset));
                        self.load_from_addr(&e.ty)?;
                    }
                    _ => {
                        self.asm
                            .mov(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset));
                    }
                }
            }
            ExprKind::Bin { op, left, right } => self.eval_bin(*op, left, right, &e.ty)?,
            ExprKind::Call { func, args } => self.eval_call(func, args)?,
            ExprKind::Cast { expr, ty } => self.eval_cast(expr, ty)?,
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?; // pointer to struct in EAX
                let (struct_name, _layout) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Eax, off as i32);
                }
            }
            ExprKind::Load(ptr) => {
                self.eval_expr(ptr)?;
                self.load_from_addr(&e.ty)?;
            }
            ExprKind::AddrOf(inner) => self.expr_addr(inner)?,
            ExprKind::Block(stmts, trailing) => {
                for st in stmts {
                    self.emit_stmt(st)?;
                }
                self.eval_expr(trailing)?;
            }
            ExprKind::Asm { .. } => bail!("inline assembly is not implemented in the x86_32 backend"),
        }
        Ok(())
    }

    fn eval_bin(&mut self, op: BinOp, left: &Expr, right: &Expr, _ty: &Type) -> Result<()> {
        if op.is_logical() {
            return self.eval_logical(op, left, right);
        }

        self.eval_expr(left)?;
        self.asm.mov(Reg::Ecx, Reg::Eax); // left -> Ecx
        self.asm.push(Reg::Ecx);          // preserve left across right evaluation
        self.eval_expr(right)?;           // right -> Eax
        self.asm.pop(Reg::Ecx);           // restore left

        if op.is_arithmetic() {
            match op {
                BinOp::Add => self.asm.add(Reg::Eax, Reg::Ecx),
                BinOp::Sub => {
                    self.asm.mov(Reg::Edx, Reg::Eax); // right -> Edx
                    self.asm.mov(Reg::Eax, Reg::Ecx); // left -> Eax
                    self.asm.sub(Reg::Eax, Reg::Edx);
                }
                BinOp::Mul => {
                    self.asm.imul(Reg::Eax, Reg::Ecx); // Eax = right * left
                }
                BinOp::Div | BinOp::Mod => {
                    // left in Ecx, right in Eax
                    if left.ty.is_signed() {
                        self.asm.push(Reg::Eax);          // save divisor (right)
                        self.asm.mov(Reg::Eax, Reg::Ecx); // dividend (left)
                        self.asm.cdq();
                        self.asm.pop(Reg::Ecx); // divisor
                        self.asm.idiv(Reg::Ecx);
                    } else {
                        self.asm.push(Reg::Eax);
                        self.asm.mov(Reg::Eax, Reg::Ecx);
                        self.asm.xor(Reg::Edx, Reg::Edx);
                        self.asm.pop(Reg::Ecx);
                        self.asm.div(Reg::Ecx);
                    }
                    if op == BinOp::Mod {
                        self.asm.mov(Reg::Eax, Reg::Edx); // remainder
                    }
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        // Comparisons: left in Ecx, right in Eax.
        self.asm.cmp(Reg::Ecx, Reg::Eax);
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg::Eax.r8());
        self.asm.movzx8(Reg::Eax, Reg::Eax);
        Ok(())
    }

    fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        let short = self.asm.new_label();
        let end = self.asm.new_label();
        self.eval_expr(left)?;
        self.asm.test(Reg::Eax, Reg::Eax);
        match op {
            BinOp::And => self.asm.je(short),
            BinOp::Or => self.asm.jne(short),
            _ => unreachable!(),
        }
        self.eval_expr(right)?;
        self.asm.jmp(end);
        self.bind_label(short);
        match op {
            BinOp::And => self.asm.mov(Reg::Eax, 0i32),
            BinOp::Or => self.asm.mov(Reg::Eax, 1i32),
            _ => unreachable!(),
        }
        self.bind_label(end);
        Ok(())
    }

    fn eval_call(&mut self, func: &str, args: &[Expr]) -> Result<()> {
        let target = *self
            .func_labels
            .get(func)
            .ok_or_else(|| anyhow::anyhow!("unknown function: {}", func))?;

        // Evaluate each argument into a temporary stack slot so that later
        // argument evaluations (which may themselves be function calls) do
        // not clobber earlier arguments.
        let mut arg_slots = Vec::new();
        for a in args {
            self.eval_expr(a)?;
            let slot = self.alloc_slot(4, 4);
            self.store_scalar(slot.offset);
            arg_slots.push(slot);
        }

        // Push arguments right-to-left for cdecl.
        for slot in arg_slots.iter().rev() {
            self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset));
            self.asm.push(Reg::Eax);
        }

        self.asm.call(target);

        if !arg_slots.is_empty() {
            self.asm.add(Reg::Esp, (arg_slots.len() * 4) as i32);
        }
        Ok(())
    }

    fn eval_cast(&mut self, expr: &Expr, to: &Type) -> Result<()> {
        self.eval_expr(expr)?;
        if &expr.ty == to {
            return Ok(());
        }
        match (expr.ty.clone(), to.clone()) {
            // Pointer / integer conversions are no-ops at the machine level.
            (Type::Ptr(_), _) if to.is_integer() => {}
            (_, Type::Ptr(_)) if expr.ty.is_integer() => {}
            // Sign-extend small signed integers to 32 bits.
            (Type::I8, Type::I32) => {
                self.asm.movsx8(Reg::Eax, Reg::Eax);
            }
            (Type::I16, Type::I32) => {
                self.asm.movsx16(Reg::Eax, Reg::Eax);
            }
            // Zero-extend unsigned small integers.
            (Type::U8 | Type::Char | Type::Bool, Type::U32) => {
                self.asm.movzx8(Reg::Eax, Reg::Eax);
            }
            (Type::U16, Type::U32) => {
                self.asm.movzx16(Reg::Eax, Reg::Eax);
            }
            // 32-bit truncation is a no-op because values already live in 32-bit slots.
            (_, Type::I32 | Type::U32) => {}
            // Any narrowing is acceptable for the milestone (value already in low bits).
            (_, _) => {}
        }
        Ok(())
    }

    fn expr_addr(&mut self, e: &Expr) -> Result<()> {
        match &e.kind {
            ExprKind::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("unknown variable: {}", name))?;
                self.asm.lea(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset));
            }
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?;
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Eax, off as i32);
                }
            }
            ExprKind::Load(ptr) => {
                self.eval_expr(ptr)?;
            }
            _ => bail!("cannot take address of expression"),
        }
        Ok(())
    }

    fn lvalue_addr(&mut self, lv: &LValue) -> Result<()> {
        match lv {
            LValue::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("unknown variable: {}", name))?;
                self.asm.lea(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset));
            }
            LValue::Deref(ptr) => {
                self.eval_expr(ptr)?; // pointer value is already the address
            }
            LValue::Field { base, field } => {
                self.eval_expr(base)?; // pointer to struct
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Eax, off as i32);
                }
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn alloc_slot(&mut self, size: usize, align: usize) -> Slot {
        let aligned = align_up(self.frame_size, align);
        self.frame_size = aligned + size;
        Slot {
            offset: -(self.frame_size as i32),
            size,
        }
    }

    fn alloc_named_slot(&mut self, name: &str, size: usize, align: usize) -> Slot {
        let slot = self.alloc_slot(size, align);
        self.locals.insert(name.to_string(), slot.clone());
        slot
    }

    fn string_label(&mut self, s: &str) -> u32 {
        if let Some(&lab) = self.string_labels.get(s) {
            return lab;
        }
        let lab = self.asm.new_label();
        self.string_labels.insert(s.to_string(), lab);
        lab
    }

    fn store_scalar(&mut self, offset: i32) {
        self.asm
            .mov(Mem::base_disp(Reg::Ebp, offset), Reg::Eax);
    }

    fn store_width(&mut self, width: u32, addr: Reg, value: Reg) {
        let mem = Mem::base(addr);
        match width {
            8 => self.asm.store8(mem, value),
            16 => self.asm.store16(mem, value),
            32 => self.asm.store32(mem, value),
            _ => self.asm.mov(mem, value),
        }
    }

    fn lvalue_store_width(&self, lv: &LValue) -> u32 {
        match lv {
            LValue::Deref(ptr) => match &ptr.ty {
                Type::Ptr(inner) => scalar_width(inner),
                _ => 32,
            },
            LValue::Field { base, field } => {
                let name = match &base.ty {
                    Type::Ptr(inner) => match inner.as_ref() {
                        Type::Struct(n) => n.clone(),
                        _ => return 32,
                    },
                    _ => return 32,
                };
                self.prog
                    .structs
                    .iter()
                    .find(|s| s.name == name)
                    .and_then(|s| s.fields.get(*field))
                    .map(|(_, t)| scalar_width(t))
                    .unwrap_or(32)
            }
            LValue::Var(_) => 32,
        }
    }

    fn load_from_addr(&mut self, ty: &Type) -> Result<()> {
        match ty {
            Type::I8 => self.asm.movsx8(Reg::Eax, Mem::base(Reg::Eax)),
            Type::U8 | Type::Char | Type::Bool => self.asm.movzx8(Reg::Eax, Mem::base(Reg::Eax)),
            Type::I16 => self.asm.movsx16(Reg::Eax, Mem::base(Reg::Eax)),
            Type::U16 => self.asm.movzx16(Reg::Eax, Mem::base(Reg::Eax)),
            Type::I32 | Type::U32 | Type::F32 => self.asm.mov(Reg::Eax, Mem::base(Reg::Eax)),
            _ => self.asm.mov(Reg::Eax, Mem::base(Reg::Eax)),
        }
        Ok(())
    }

    fn struct_ptr_layout(&self, ty: &Type) -> Result<(String, StructLayout)> {
        let name = match ty {
            Type::Ptr(inner) => match inner.as_ref() {
                Type::Struct(n) => n.clone(),
                _ => bail!("gep/load on non-struct pointer: {:?}", ty),
            },
            Type::Struct(n) => n.clone(),
            _ => bail!("field access on non-struct type: {:?}", ty),
        };
        let lay = self
            .struct_layouts
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?
            .clone();
        Ok((name, lay))
    }

    fn field_offset(&self, struct_name: &str, idx: usize) -> Result<usize> {
        let lay = self
            .struct_layouts
            .get(struct_name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", struct_name))?;
        lay.offsets
            .get(idx)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("field index {} out of range", idx))
    }

    fn emit_runtime(&mut self, start_label: u32) -> Result<()> {
        // _dev_exit(code) -> sys_exit(ebx=code)
        let e = *self.func_labels.get("_dev_exit").unwrap();
        self.bind_label(e);
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8));
        self.asm.mov(Reg::Eax, 1i32); // sys_exit
        self.asm.int(0x80);

        // _dev_puts(s) -> print null-terminated string to stdout
        let p = *self.func_labels.get("_dev_puts").unwrap();
        self.bind_label(p);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.push(Reg::Esi); // callee-saved
        self.asm.push(Reg::Edi); // callee-saved
        self.asm.mov(Reg::Esi, Mem::base_disp(Reg::Ebp, 8)); // s
        self.asm.mov(Reg::Edi, Reg::Esi); // original pointer
        self.asm.xor(Reg::Ecx, Reg::Ecx); // length
        let loop_lab = self.asm.new_label();
        let done_lab = self.asm.new_label();
        self.bind_label(loop_lab);
        self.asm.movzx8(Reg::Eax, Mem::base(Reg::Esi));
        self.asm.test(Reg::Eax, Reg::Eax);
        self.asm.je(done_lab);
        self.asm.inc(Reg::Esi);
        self.asm.inc(Reg::Ecx);
        self.asm.jmp(loop_lab);
        self.bind_label(done_lab);
        self.asm.mov(Reg::Eax, 4i32); // sys_write
        self.asm.mov(Reg::Ebx, 1i32); // stdout
        self.asm.mov(Reg::Edx, Reg::Ecx); // len
        self.asm.mov(Reg::Ecx, Reg::Edi); // buf
        self.asm.int(0x80);
        self.asm.pop(Reg::Edi);
        self.asm.pop(Reg::Esi);
        self.asm.leave();
        self.asm.ret();

        // _dev_putchar(c) -> write one byte to stdout
        let pc = *self.func_labels.get("_dev_putchar").unwrap();
        self.bind_label(pc);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.push(Reg::Eax); // allocate 4-byte slot for the character
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8)); // c
        self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax); // store low byte (and zeros)
        self.asm.mov(Reg::Eax, 4i32); // sys_write
        self.asm.mov(Reg::Ebx, 1i32); // stdout
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -4)); // buffer address
        self.asm.mov(Reg::Edx, 1i32); // count
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        // _dev_getchar() -> read one byte from stdin, or -1 on EOF
        let c = *self.func_labels.get("_dev_getchar").unwrap();
        self.bind_label(c);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.sub(Reg::Esp, 4i32); // buffer
        self.asm.mov(Reg::Eax, 3i32); // sys_read
        self.asm.mov(Reg::Ebx, 0i32); // stdin
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -4));
        self.asm.mov(Reg::Edx, 1i32);
        self.asm.int(0x80);
        self.asm.cmp(Reg::Eax, 1i32);
        let ok_lab = self.asm.new_label();
        self.asm.je(ok_lab);
        self.asm.mov(Reg::Eax, -1i32);
        self.asm.leave();
        self.asm.ret();
        self.bind_label(ok_lab);
        self.asm.movzx8(Reg::Eax, Mem::base_disp(Reg::Ebp, -4));
        self.asm.leave();
        self.asm.ret();

        // _dev_rand() -> read timestamp counter
        let r = *self.func_labels.get("_dev_rand").unwrap();
        self.bind_label(r);
        self.asm.rdtsc(); // edx:eax
        self.asm.ret();

        // Memory fences for std.volatile barriers.
        let lb = *self.func_labels.get("_dev_lfence").unwrap();
        self.bind_label(lb);
        self.asm.lfence();
        self.asm.ret();

        let sb = *self.func_labels.get("_dev_sfence").unwrap();
        self.bind_label(sb);
        self.asm.sfence();
        self.asm.ret();

        let mb = *self.func_labels.get("_dev_mfence").unwrap();
        self.bind_label(mb);
        self.asm.mfence();
        self.asm.ret();

        // _dev_alloc(size) -> ptr[char]
        // A tiny bump allocator over a 64 KiB `.bss` arena.  A pointer in `.data`
        // (initialized to the arena base) is advanced by `size` on each call and
        // the previous value is returned.  The absolute address of the pointer
        // is patched into the `mov eax, imm32` below once `.data` is laid out.
        if let Some(&a) = self.func_labels.get("_dev_alloc") {
            self.bind_label(a);
            self.asm.push(Reg::Ebp);
            self.asm.mov(Reg::Ebp, Reg::Esp);
            let patch_off = self.asm.len() + 2; // C7 /0 imm32, offset of imm32
            self.alloc_ptr_patch = Some(patch_off);
            self.asm.mov(Reg::Eax, 0i32); // eax = &bump_ptr (patched)
            self.asm.mov(Reg::Ecx, Mem::base(Reg::Eax)); // current ptr
            self.asm.mov(Reg::Edx, Reg::Ecx);
            self.asm.add(Reg::Edx, Mem::base_disp(Reg::Ebp, 8)); // new ptr = current + size
            self.asm.mov(Mem::base(Reg::Eax), Reg::Edx); // store new ptr
            self.asm.mov(Reg::Eax, Reg::Ecx); // return previous ptr
            self.asm.leave();
            self.asm.ret();
        }

        // _dev_free(p) -> void (bump arena: deallocation is a no-op).
        if let Some(&f) = self.func_labels.get("_dev_free") {
            self.bind_label(f);
            self.asm.ret();
        }

        // _start: call main, then exit.
        self.bind_label(start_label);
        let main_lab = *self
            .func_labels
            .get("_forge_main")
            .ok_or_else(|| anyhow::anyhow!("hosted mode requires a main function"))?;
        self.asm.call(main_lab);
        self.asm.mov(Reg::Ebx, Reg::Eax);
        self.asm.mov(Reg::Eax, 1i32);
        self.asm.int(0x80);

        Ok(())
    }
}

// ----------------------------------------------------------------------------
// Layout / ABI helpers
// ----------------------------------------------------------------------------

fn layout_struct(s: &StructDef) -> StructLayout {
    let mut offset = 0usize;
    let mut offsets = Vec::with_capacity(s.fields.len());
    for (_name, ty) in &s.fields {
        let size = type_size(ty);
        let align = type_align(ty);
        offset = align_up(offset, align);
        offsets.push(offset);
        offset += size;
    }
    let align = s.fields.iter().map(|(_, ty)| type_align(ty)).max().unwrap_or(1);
    StructLayout {
        size: align_up(offset, align),
        align,
        offsets,
    }
}

fn type_size(ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 4,
        Type::Struct(name) => panic!("struct size for {} must come from layout table", name),
        _ => 4,
    }
}

fn type_align(ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 4,
        Type::Struct(_) => 4,
        _ => 4,
    }
}

fn scalar_width(ty: &Type) -> u32 {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 | Type::F32 => 32,
        _ => 32,
    }
}

fn cond_for_cmp(op: BinOp, ty: &Type) -> Result<Cond> {
    let signed = ty.is_signed();
    let unsigned = !signed && (ty.is_integer() || matches!(ty, Type::Ptr(_)));
    match op {
        BinOp::Eq => Ok(Cond::E),
        BinOp::Ne => Ok(Cond::Ne),
        BinOp::Lt => {
            if signed {
                Ok(Cond::L)
            } else if unsigned {
                Ok(Cond::B)
            } else {
                bail!("comparison {:?} on type {:?}", op, ty)
            }
        }
        BinOp::Le => {
            if signed {
                Ok(Cond::Le)
            } else if unsigned {
                Ok(Cond::Be)
            } else {
                bail!("comparison {:?} on type {:?}", op, ty)
            }
        }
        BinOp::Gt => {
            if signed {
                Ok(Cond::G)
            } else if unsigned {
                Ok(Cond::A)
            } else {
                bail!("comparison {:?} on type {:?}", op, ty)
            }
        }
        BinOp::Ge => {
            if signed {
                Ok(Cond::Ge)
            } else if unsigned {
                Ok(Cond::Ae)
            } else {
                bail!("comparison {:?} on type {:?}", op, ty)
            }
        }
        _ => bail!("not a comparison: {:?}", op),
    }
}

fn align_up(v: usize, align: usize) -> usize {
    if align == 0 {
        return v;
    }
    ((v + align - 1) / align) * align
}

fn align_up_u32(v: u32, align: u32) -> u32 {
    if align == 0 {
        return v;
    }
    ((v + align - 1) / align) * align
}
