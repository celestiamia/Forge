//! IR to native machine code translator.
//!
//! For the hosted x86-64 target the translator targets the System V AMD64 ABI:
//!   - integer/pointer args in RDI, RSI, RDX, RCX, R8, R9,
//!   - return value in RAX,
//!   - stack frame is `push rbp; mov rbp, rsp; sub rsp, N`.
//!
//! For the bare-metal x86-16 boot target the translator emits a flat 512-byte
//! boot sector by assembling 16-bit real-mode inline assembly blocks.

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::backend::ir::{BinOp, Expr, ExprKind, Func, LValue, Literal, Program, Stmt, StructDef, Type};
use crate::backend::codegen16;
use crate::backend::x64::{Assembler, Cond, Mem, Reg};
use crate::obj::elf::Elf64Writer;
use crate::obj::flat::FlatWriter;
use crate::obj::ObjectWriter;

const BASE_VADDR: u64 = 0x400000;
const EHDR_SIZE: u64 = 64;
const PHDR_SIZE: u64 = 56;

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
    global_labels: HashMap<String, u32>,

    locals: HashMap<String, Slot>,
    frame_size: usize,
    struct_layouts: HashMap<String, StructLayout>,
    addr_tmp: i32,
    ret_label: u32,
    rand_seed_patch: Option<usize>,
    /// Code offset of the `movabs rax, <bump_ptr_vaddr>` immediate in `_dev_alloc`,
    /// patched once the `.data` segment layout is known.  `None` when the program
    /// does not use the bump allocator.
    alloc_ptr_patch: Option<usize>,
    loop_end_stack: Vec<u32>,
    loop_head_stack: Vec<u32>,
}

pub fn compile_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    let is_flat = prog.obj_format.as_deref() == Some("flat");
    if is_flat {
        compile_flat_program(prog)
    } else {
        compile_elf_program(prog)
    }
}

fn compile_flat_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    if prog.arch.as_deref() != Some("x86_16") {
        bail!(
            "flat binary target {} is not supported",
            prog.arch.as_deref().unwrap_or("(unknown)")
        );
    }

    let code = codegen16::compile_program(prog)?;

    // Validate that the boot sector fits in 512 bytes.  The flat writer will pad
    // to 510 bytes and append the boot signature.
    if code.len() > 510 {
        bail!(
            "boot sector code is {} bytes, exceeding the 510-byte limit",
            code.len()
        );
    }

    Ok(Box::new(FlatWriter::new(code, true)))
}

fn compile_elf_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    let mut cg = CodeGen::new(prog);

    // Reserve labels for every user function and the runtime helpers.
    for f in &prog.funcs {
        let lab = cg.asm.new_label();
        cg.func_labels.insert(f.name.clone(), lab);
    }
    let start_label = if prog.hosted {
        let l = cg.asm.new_label();
        cg.func_labels.insert("_start".to_string(), l);
        let w = cg.asm.new_label();
        cg.func_labels.insert("_dev_write".to_string(), w);
        let p = cg.asm.new_label();
        cg.func_labels.insert("_dev_puts".to_string(), p);
        let c = cg.asm.new_label();
        cg.func_labels.insert("_dev_getchar".to_string(), c);
        let pc = cg.asm.new_label();
        cg.func_labels.insert("_dev_putchar".to_string(), pc);
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
        let s = cg.asm.new_label();
        cg.func_labels.insert("_dev_socket".to_string(), s);
        let b = cg.asm.new_label();
        cg.func_labels.insert("_dev_bind".to_string(), b);
        let li = cg.asm.new_label();
        cg.func_labels.insert("_dev_listen".to_string(), li);
        let a = cg.asm.new_label();
        cg.func_labels.insert("_dev_accept".to_string(), a);
        let re = cg.asm.new_label();
        cg.func_labels.insert("_dev_read".to_string(), re);
        let cl = cg.asm.new_label();
        cg.func_labels.insert("_dev_close".to_string(), cl);
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

    // Append global constants and string literals as rodata at the end of the assembler buffer.
    let rodata_start = cg.asm.new_label();
    cg.bind_label(rodata_start);

    // Emit globals (constants) first
    let mut globals: Vec<(String, u32, Literal, Type)> = cg
        .prog
        .globals
        .iter()
        .map(|g| (g.name.clone(), *cg.global_labels.get(&g.name).unwrap(), g.init.clone(), g.ty.clone()))
        .collect();
    globals.sort_by_key(|(_, lab, _, _)| *lab);
    // Global slots initialized with a string literal hold the absolute address
    // of the literal in rodata; patched once the layout is known below.
    let mut string_patches: Vec<(u32, u32)> = Vec::new();
    for (_, lab, init, ty) in globals {
        cg.bind_label(lab);
        let value = match init {
            Literal::Int(v) => {
                let size = ty_width(&ty);
                match size {
                    8 => (v as i8).to_le_bytes().to_vec(),
                    16 => (v as i16).to_le_bytes().to_vec(),
                    32 => (v as i32).to_le_bytes().to_vec(),
                    64 => v.to_le_bytes().to_vec(),
                    _ => v.to_le_bytes().to_vec(),
                }
            }
            Literal::Bool(v) => {
                let val = if v { 1i8 } else { 0i8 };
                val.to_le_bytes().to_vec()
            }
            Literal::Char(v) => (v as u8).to_le_bytes().to_vec(),
            Literal::String(s) => {
                let s_lab = cg.string_label(&s);
                string_patches.push((lab, s_lab));
                vec![0; 8]
            }
            _ => vec![0; 8],
        };
        let mut padded = value;
        padded.resize(8, 0);
        cg.asm.append_bytes(&padded);
    }

    // Emit string literals
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
    let mut rodata = bytes[split..].to_vec();

    let text_offset = EHDR_SIZE + PHDR_SIZE * 2;
    let entry_vaddr = BASE_VADDR + text_offset + *cg.label_offsets.get(&start_label).unwrap_or(&0) as u64;

    // Patch string-valued global slots with the absolute address of their
    // literal in rodata.
    let rodata_start_off = *cg.label_offsets.get(&rodata_start).unwrap_or(&0);
    let rodata_vaddr = BASE_VADDR + text_offset + code.len() as u64;
    for (g_lab, s_lab) in string_patches {
        let g_off = *cg.label_offsets.get(&g_lab).unwrap_or(&0);
        let s_off = *cg.label_offsets.get(&s_lab).unwrap_or(&0);
        let slot = g_off - rodata_start_off;
        let addr = rodata_vaddr + (s_off - rodata_start_off) as u64;
        rodata[slot..slot + 8].copy_from_slice(&addr.to_le_bytes());
    }

    // Build the writable `.data` segment and patch absolute virtual addresses
    // into the code.  Two things can live here in hosted mode: the random seed
    // (8 bytes) and, when the program uses the bump allocator, the bump pointer
    // (8 bytes, initialized to the base of a 64 KiB `.bss` arena).
    let first_seg_end = text_offset + code.len() as u64 + rodata.len() as u64;
    let data_offset = align_up_u64(first_seg_end, PAGE_SIZE);
    let data_vaddr = BASE_VADDR + data_offset;

    let mut data: Vec<u8> = Vec::new();
    if let Some(patch_off) = cg.rand_seed_patch {
        // Random seed lives at the start of `.data`.
        code[patch_off..patch_off + 8].copy_from_slice(&data_vaddr.to_le_bytes());
        data.extend_from_slice(&[0; 8]);
    }

    // Bump allocator pointer, appended after the seed.  Its initial value is the
    // virtual address of the `.bss` arena, which immediately follows `.data`.
    let mut bss_size: u64 = 0;
    if let Some(alloc_patch) = cg.alloc_ptr_patch {
        const ARENA_SIZE: u64 = 64 * 1024;
        let bump_data_off = data.len() as u64;
        data.extend_from_slice(&[0u8; 8]); // placeholder for the bump pointer
        let bump_ptr_vaddr = data_vaddr + bump_data_off;
        code[alloc_patch..alloc_patch + 8].copy_from_slice(&bump_ptr_vaddr.to_le_bytes());
        // `.bss` starts right after `.data` in the virtual address space.
        let arena_base = data_vaddr + data.len() as u64;
        data[bump_data_off as usize..bump_data_off as usize + 8]
            .copy_from_slice(&arena_base.to_le_bytes());
        bss_size = ARENA_SIZE;
    }

    Ok(Box::new(Elf64Writer::new(code, rodata, data, bss_size, entry_vaddr)))
}

const PAGE_SIZE: u64 = 0x1000;

fn align_up_u64(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    ((value + align - 1) / align) * align
}

impl<'p> CodeGen<'p> {
    fn new(prog: &'p Program) -> Self {
        let mut layouts = HashMap::new();
        for s in &prog.structs {
            layouts.insert(s.name.clone(), layout_struct(s));
        }

        let mut global_labels = HashMap::new();
        for g in &prog.globals {
            let lab = global_labels.len() as u32 + 1000; // offset to avoid conflict
            global_labels.insert(g.name.clone(), lab);
        }

        Self {
            prog,
            asm: Assembler::new(),
            label_offsets: HashMap::new(),
            func_labels: HashMap::new(),
            string_labels: HashMap::new(),
            global_labels,
            locals: HashMap::new(),
            frame_size: 0,
            struct_layouts: layouts,
            addr_tmp: 0,
            ret_label: 0,
            rand_seed_patch: None,
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
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        let sub_imm_offset = self.asm.len() + 3; // REX + opcode + modrm
        self.asm.sub(Reg::Rsp, 0i32);

        // Reserve an address-scratch slot used by assignments.
        let slot = self.alloc_slot(8, 8);
        self.addr_tmp = slot.offset;

        // Allocate slots for parameters and spill them from ABI registers.
        for (name, _ty) in &f.params {
            self.alloc_named_slot(name, 8, 8);
        }
        for (i, (name, _ty)) in f.params.iter().enumerate() {
            let slot = self.locals.get(name).unwrap();
            let reg = abi_reg(i)?;
            self.asm
                .mov(Mem::base_disp(Reg::Rbp, slot.offset), reg);
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

        // Patch the sub rsp, imm32 with the aligned frame size.
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
                let slot = if let Some(s) = self.locals.get(name) {
                    s.clone()
                } else {
                    self.alloc_named_slot(name, 8, 8)
                };
                if let Some(e) = init {
                    self.eval_expr(e)?;
                    self.store_scalar(slot.offset);
                }
                // If the type is a struct or array, the front-end should have
                // emitted a StackAlloc for it; pure Let only creates scalar slots.
                let _ = ty;
            }
            Stmt::StackAlloc { name, elem_ty, count } => {
                let elem_size = elem_ty.byte_size();
                let raw_size = elem_size * *count;
                let align = elem_size.max(1);
                let raw_slot = self.alloc_slot(raw_size, align);
                // Pointer slot for the local.
                let ptr_slot = self.alloc_named_slot(name, 8, 8);
                self.asm
                    .lea(Reg::Rax, Mem::base_disp(Reg::Rbp, raw_slot.offset));
                self.store_scalar(ptr_slot.offset);
            }
            Stmt::Assign { lhs, rhs } => {
                self.lvalue_addr(lhs)?; // address in RAX
                self.asm
                    .mov(Mem::base_disp(Reg::Rbp, self.addr_tmp), Reg::Rax);
                self.eval_expr(rhs)?; // value in RAX
                self.asm
                    .mov(Reg::Rdx, Mem::base_disp(Reg::Rbp, self.addr_tmp));
                let width = self.lvalue_store_width(lhs);
                self.store_width(width, Reg::Rdx, Reg::Rax);
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
                self.asm.test(Reg::Rax, Reg::Rax);
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
                    // fall through to end
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
                self.asm.test(Reg::Rax, Reg::Rax);
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
                self.asm.test(Reg::Rax, Reg::Rax);
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
                Literal::Int(v) => self.asm.mov(Reg::Rax, *v),
                Literal::Bool(v) => self.asm.mov(Reg::Rax, if *v { 1i32 } else { 0i32 }),
                Literal::Char(v) => self.asm.mov(Reg::Rax, *v as i32),
                Literal::String(s) => {
                    let lab = self.string_label(s);
                    self.asm.lea_rip(Reg::Rax, lab);
                }
                Literal::Float(_) => bail!("floating point is not implemented in the x64 backend"),
                Literal::Null => self.asm.mov(Reg::Rax, 0i32),
            },
            ExprKind::Var(name) => {
                // First check if it's a global variable (constant)
                if let Some(&lab) = self.global_labels.get(name) {
                    self.asm.lea_rip(Reg::Rax, lab);
                    // Load the value based on type
                    match &e.ty {
                        Type::I8 | Type::U8 | Type::Bool | Type::Char => {
                            self.asm.movsx8(Reg::Rax, Mem::base(Reg::Rax));
                        }
                        Type::I16 | Type::U16 => {
                            self.asm.movsx16(Reg::Rax, Mem::base(Reg::Rax));
                        }
                        Type::I32 | Type::U32 => {
                            self.asm.mov32(Reg::Rax, Mem::base(Reg::Rax));
                        }
                        _ => {
                            self.asm.mov(Reg::Rax, Mem::base(Reg::Rax));
                        }
                    }
                    return Ok(());
                }
                // Otherwise it's a local variable
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("unknown variable: {}", name))?;
                match &e.ty {
                    Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32 | Type::Bool | Type::Char => {
                        self.asm
                            .lea(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset));
                        self.load_from_addr(&e.ty)?;
                    }
                    _ => {
                        self.asm
                            .mov(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset));
                    }
                }
            }
            ExprKind::Bin { op, left, right } => self.eval_bin(*op, left, right, &e.ty)?,
            ExprKind::Call { func, args } => self.eval_call(func, args)?,
            ExprKind::Cast { expr, ty } => self.eval_cast(expr, ty)?,
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?; // pointer to struct in RAX
                let (struct_name, _layout) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Rax, off as i32);
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
            ExprKind::Asm { .. } => bail!("inline assembly is not implemented in the x64 backend"),
        }
        Ok(())
    }

    fn eval_bin(&mut self, op: BinOp, left: &Expr, right: &Expr, _ty: &Type) -> Result<()> {
        if op.is_logical() {
            return self.eval_logical(op, left, right);
        }

        self.eval_expr(left)?;
        self.asm.mov(Reg::R10, Reg::Rax); // left -> R10
        self.asm.push(Reg::R10);          // preserve left across right evaluation
        self.eval_expr(right)?;           // right -> RAX
        self.asm.pop(Reg::R10);           // restore left

        if op.is_arithmetic() {
            match op {
                BinOp::Add => self.asm.add(Reg::Rax, Reg::R10),
                BinOp::Sub => {
                    self.asm.mov(Reg::Rdx, Reg::Rax);
                    self.asm.mov(Reg::Rax, Reg::R10);
                    self.asm.sub(Reg::Rax, Reg::Rdx);
                }
                BinOp::Mul => {
                    self.asm.mov(Reg::Rdx, Reg::Rax);
                    self.asm.mov(Reg::Rax, Reg::R10);
                    self.asm.imul(Reg::Rax, Reg::Rdx);
                }
                BinOp::Div | BinOp::Mod => {
                    if !left.ty.is_signed() {
                        bail!("unsigned division is not implemented in the x64 backend");
                    }
                    // Divisor must not be in RDX/RAX. Move right to R11.
                    self.asm.mov(Reg::R11, Reg::Rax);
                    self.asm.mov(Reg::Rax, Reg::R10); // dividend
                    self.asm.cqo();
                    self.asm.idiv(Reg::R11);
                    if op == BinOp::Mod {
                        self.asm.mov(Reg::Rax, Reg::Rdx); // remainder
                    }
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        // Bitwise operations
        if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr) {
            match op {
                BinOp::BitAnd => self.asm.and(Reg::Rax, Reg::R10),
                BinOp::BitOr => self.asm.or(Reg::Rax, Reg::R10),
                BinOp::BitXor => self.asm.xor(Reg::Rax, Reg::R10),
                BinOp::Shl => {
                    self.asm.mov(Reg::Rcx, Reg::Rax);
                    self.asm.mov(Reg::Rax, Reg::R10);
                    self.asm.shl_cl(Reg::Rax);
                }
                BinOp::Shr => {
                    self.asm.mov(Reg::Rcx, Reg::Rax);
                    self.asm.mov(Reg::Rax, Reg::R10);
                    self.asm.sar_cl(Reg::Rax);
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        // Comparisons: left in R10, right in RAX.
        self.asm.cmp(Reg::R10, Reg::Rax);
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg::Rax.r8());
        self.asm.movzx8(Reg::Rax, Reg::Rax);
        Ok(())
    }

    fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        let short = self.asm.new_label();
        let end = self.asm.new_label();
        self.eval_expr(left)?;
        self.asm.test(Reg::Rax, Reg::Rax);
        match op {
            BinOp::And => self.asm.je(short),
            BinOp::Or => self.asm.jne(short),
            _ => unreachable!(),
        }
        self.eval_expr(right)?;
        self.asm.jmp(end);
        self.bind_label(short);
        match op {
            BinOp::And => self.asm.mov(Reg::Rax, 0i32),
            BinOp::Or => self.asm.mov(Reg::Rax, 1i32),
            _ => unreachable!(),
        }
        self.bind_label(end);
        Ok(())
    }

    fn eval_call(&mut self, func: &str, args: &[Expr]) -> Result<()> {
        if args.len() > 6 {
            bail!("functions with more than 6 arguments are not supported");
        }
        let target = *self
            .func_labels
            .get(func)
            .ok_or_else(|| anyhow::anyhow!("unknown function: {}", func))?;

        // Evaluate each argument into a temporary stack slot so that later
        // argument evaluations (which may themselves be function calls) do
        // not clobber earlier arguments in caller-saved ABI registers.
        let mut arg_slots = Vec::new();
        for a in args {
            self.eval_expr(a)?;
            let slot = self.alloc_slot(8, 8);
            self.store_scalar(slot.offset);
            arg_slots.push(slot);
        }

        for (i, slot) in arg_slots.iter().enumerate() {
            let reg = abi_reg(i)?;
            self.asm.mov(reg, Mem::base_disp(Reg::Rbp, slot.offset));
        }

        self.asm.call(target);
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
            // Sign-extend small signed integers to 64 bits.
            (Type::I8, Type::I64) | (Type::I8, Type::I32) => {
                self.asm.movsx8(Reg::Rax, Reg::Rax);
            }
            (Type::I16, Type::I64) | (Type::I16, Type::I32) => {
                self.asm.movsx16(Reg::Rax, Reg::Rax);
            }
            (Type::I32, Type::I64) => {
                self.asm.movsxd(Reg::Rax, Reg::Rax);
            }
            // Zero-extend unsigned small integers.
            (Type::U8 | Type::Char | Type::Bool, _) if to.is_integer() => {
                self.asm.movzx8(Reg::Rax, Reg::Rax);
            }
            (Type::U16, _) if to.is_integer() => {
                self.asm.movzx8(Reg::Rax, Reg::Rax); // movzx16 missing; use 8 as placeholder
            }
            // 64-bit truncation is a no-op because we keep values in 64-bit slots.
            (_, Type::I64 | Type::U64) => {}
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
                self.asm.lea(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset));
            }
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?;
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Rax, off as i32);
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
                self.asm.lea(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset));
            }
            LValue::Deref(ptr) => {
                self.eval_expr(ptr)?; // pointer value is already the address
            }
            LValue::Field { base, field } => {
                self.eval_expr(base)?; // pointer to struct
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Rax, off as i32);
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
            .mov(Mem::base_disp(Reg::Rbp, offset), Reg::Rax);
    }

    fn store_rdx_64(&mut self) {
        self.asm.mov(Mem::base(Reg::Rdx), Reg::Rax);
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
                _ => 64,
            },
            LValue::Field { base, field } => {
                let name = match &base.ty {
                    Type::Ptr(inner) => match inner.as_ref() {
                        Type::Struct(n) => n.clone(),
                        _ => return 64,
                    },
                    _ => return 64,
                };
                self.prog
                    .structs
                    .iter()
                    .find(|s| s.name == name)
                    .and_then(|s| s.fields.get(*field))
                    .map(|(_, t)| scalar_width(t))
                    .unwrap_or(64)
            }
            LValue::Var(_) => 64,
        }
    }

    fn load_from_addr(&mut self, ty: &Type) -> Result<()> {
        match ty {
            Type::I8 => self.asm.movsx8(Reg::Rax, Mem::base(Reg::Rax)),
            Type::U8 | Type::Char | Type::Bool => self.asm.movzx8(Reg::Rax, Mem::base(Reg::Rax)),
            Type::I16 => self.asm.movsx16(Reg::Rax, Mem::base(Reg::Rax)),
            Type::U16 => self.asm.movzx16(Reg::Rax, Mem::base(Reg::Rax)),
            Type::I32 => self.asm.movsxd(Reg::Rax, Mem::base(Reg::Rax)),
            Type::U32 => self.asm.mov32(Reg::Rax, Mem::base(Reg::Rax)),
            _ => self.asm.mov(Reg::Rax, Mem::base(Reg::Rax)),
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
        // _dev_write(fd, buf, len) -> sys_write(rax=1)
        let w = *self.func_labels.get("_dev_write").unwrap();
        self.bind_label(w);
        self.asm.mov(Reg::Rax, 1i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_puts(s) -> print null-terminated string to stdout
        let p = *self.func_labels.get("_dev_puts").unwrap();
        self.bind_label(p);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.push(Reg::Rdi);          // preserve original buf on stack
        self.asm.mov(Reg::Rsi, Reg::Rdi); // scan pointer
        self.asm.xor(Reg::Rcx, Reg::Rcx); // length
        let loop_lab = self.asm.new_label();
        let done_lab = self.asm.new_label();
        self.bind_label(loop_lab);
        self.asm.movzx8(Reg::Rdx, Mem::base(Reg::Rsi));
        self.asm.test(Reg::Rdx, Reg::Rdx);
        self.asm.je(done_lab);
        self.asm.inc(Reg::Rsi);
        self.asm.inc(Reg::Rcx);
        self.asm.jmp(loop_lab);
        self.bind_label(done_lab);
        self.asm.mov(Reg::Rdi, 1i32);     // stdout
        self.asm.mov(Reg::Rsi, Mem::base_disp(Reg::Rbp, -8)); // original buf
        self.asm.mov(Reg::Rdx, Reg::Rcx); // len
        let write_lab = *self.func_labels.get("_dev_write").unwrap();
        self.asm.call(write_lab);
        self.asm.leave();
        self.asm.ret();

        // _dev_putchar(c) -> write one byte to stdout
        let pc = *self.func_labels.get("_dev_putchar").unwrap();
        self.bind_label(pc);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.push(Reg::Rdi);                 // store character on stack
        self.asm.mov(Reg::Rdi, 1i32);            // stdout
        self.asm.lea(Reg::Rsi, Mem::base_disp(Reg::Rbp, -8)); // buffer address
        self.asm.mov(Reg::Rdx, 1i32);            // count
        self.asm.mov(Reg::Rax, 1i32);            // sys_write
        self.asm.syscall();
        self.asm.leave();
        self.asm.ret();

        // _dev_getchar() -> read one byte from stdin, or -1 on EOF
        let c = *self.func_labels.get("_dev_getchar").unwrap();
        self.bind_label(c);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.sub(Reg::Rsp, 16);            // allocate a 16-byte aligned buffer
        self.asm.mov(Reg::Rdi, 0i32);          // stdin
        self.asm.lea(Reg::Rsi, Mem::base_disp(Reg::Rbp, -8)); // buffer address
        self.asm.mov(Reg::Rdx, 1i32);          // count
        self.asm.mov(Reg::Rax, 0i32);          // sys_read
        self.asm.syscall();
        self.asm.cmp(Reg::Rax, 1i32);          // did we read exactly 1 byte?
        let ok_lab = self.asm.new_label();
        let eof_lab = self.asm.new_label();
        self.asm.je(ok_lab);
        self.bind_label(eof_lab);
        self.asm.mov(Reg::Rax, -1i32);         // return -1 on EOF/error
        self.asm.leave();
        self.asm.ret();
        self.bind_label(ok_lab);
        self.asm.movzx8(Reg::Rax, Mem::base_disp(Reg::Rbp, -8)); // return byte zero-extended
        self.asm.leave();
        self.asm.ret();

        // _dev_rand() -> simple LCG pseudo-random number.
        // The mutable seed lives in the writable .data segment; the absolute
        // address is patched into the code once the segment layout is known.
        let r = *self.func_labels.get("_dev_rand").unwrap();
        self.bind_label(r);
        let seed_addr_offset = self.asm.len() + 2; // REX + opcode before imm64
        self.rand_seed_patch = Some(seed_addr_offset);
        self.asm.movabs(Reg::Rax, 0); // placeholder: movabs rax, <seed_vaddr>
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.mov(Reg::R11, Mem::base(Reg::Rax)); // load seed
        // LCG: seed = seed * 1103515245 + 12345
        self.asm.mov(Reg::R10, 1103515245i32);
        self.asm.imul(Reg::R11, Reg::R10);
        self.asm.add(Reg::R11, 12345i32);
        self.asm.mov(Mem::base(Reg::Rax), Reg::R11); // store seed
        // return (seed >> 16) & 0x7fffffff
        self.asm.shr(Reg::R11, 16i8);
        self.asm.mov(Reg::Rax, 0x7fffffffi32);
        self.asm.and(Reg::Rax, Reg::R11);
        self.asm.leave();
        self.asm.ret();

        // _dev_exit(code) -> sys_exit(rax=60)
        let e = *self.func_labels.get("_dev_exit").unwrap();
        self.bind_label(e);
        self.asm.mov(Reg::Rax, 60i32);
        self.asm.syscall();

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

        // _dev_socket(domain, type, protocol) -> sys_socket(rax=41)
        let s = *self.func_labels.get("_dev_socket").unwrap();
        self.bind_label(s);
        self.asm.mov(Reg::Rax, 41i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_bind(fd, addr, addrlen) -> sys_bind(rax=49)
        let b = *self.func_labels.get("_dev_bind").unwrap();
        self.bind_label(b);
        self.asm.mov(Reg::Rax, 49i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_listen(fd, backlog) -> sys_listen(rax=50)
        let li = *self.func_labels.get("_dev_listen").unwrap();
        self.bind_label(li);
        self.asm.mov(Reg::Rax, 50i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_accept(fd, addr, addrlen) -> sys_accept(rax=43)
        let a = *self.func_labels.get("_dev_accept").unwrap();
        self.bind_label(a);
        self.asm.mov(Reg::Rax, 43i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_read(fd, buf, count) -> sys_read(rax=0)
        let re = *self.func_labels.get("_dev_read").unwrap();
        self.bind_label(re);
        self.asm.mov(Reg::Rax, 0i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_close(fd) -> sys_close(rax=3)
        let cl = *self.func_labels.get("_dev_close").unwrap();
        self.bind_label(cl);
        self.asm.mov(Reg::Rax, 3i32);
        self.asm.syscall();
        self.asm.ret();

        // _dev_alloc(size) -> ptr[char]
        // A tiny bump allocator: a pointer in the writable `.data` segment is
        // initialized to the base of a 64 KiB `.bss` arena.  Each call advances
        // the pointer by `size` bytes and returns the previous value.  There is
        // no free-list; `_dev_free` is a no-op.  The absolute address of the bump
        // pointer is patched into the `movabs` below once `.data` is laid out.
        if let Some(&a) = self.func_labels.get("_dev_alloc") {
            self.bind_label(a);
            let patch_off = self.asm.len() + 2; // REX + opcode before imm64
            self.alloc_ptr_patch = Some(patch_off);
            self.asm.movabs(Reg::Rax, 0); // rax = &bump_ptr (patched)
            self.asm.mov(Reg::Rcx, Mem::base(Reg::Rax)); // current ptr
            self.asm.mov(Reg::Rdx, Reg::Rcx);
            self.asm.add(Reg::Rdx, Reg::Rdi); // new ptr = current + size
            self.asm.mov(Mem::base(Reg::Rax), Reg::Rdx); // store new ptr
            self.asm.mov(Reg::Rax, Reg::Rcx); // return previous ptr
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
        self.asm.mov(Reg::Rdi, Reg::Rax);
        let exit_lab = *self.func_labels.get("_dev_exit").unwrap();
        self.asm.call(exit_lab);

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
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 8,
        Type::Struct(name) => panic!("struct size for {} must come from layout table", name),
        _ => 8,
    }
}

fn type_align(ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 8,
        Type::Struct(_) => 8,
        _ => 8,
    }
}

fn scalar_width(ty: &Type) -> u32 {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 | Type::F32 => 32,
        _ => 64,
    }
}

fn ty_width(ty: &Type) -> u32 {
    scalar_width(ty)
}

fn abi_reg(idx: usize) -> Result<Reg> {
    match idx {
        0 => Ok(Reg::Rdi),
        1 => Ok(Reg::Rsi),
        2 => Ok(Reg::Rdx),
        3 => Ok(Reg::Rcx),
        4 => Ok(Reg::R8),
        5 => Ok(Reg::R9),
        _ => bail!("argument index {} out of ABI register range", idx),
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
