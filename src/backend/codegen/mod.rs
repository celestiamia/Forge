//! IR to native machine code translator.
//!
//! For the hosted x86-64 target the translator targets the System V AMD64 ABI:
//!   - integer/pointer args in RDI, RSI, RDX, RCX, R8, R9,
//!   - return value in RAX,
//!   - stack frame is `push rbp; mov rbp, rsp; sub rsp, N`.
//!
//! For the bare-metal x86-16 boot target the translator emits a flat 512-byte
//! boot sector by assembling 16-bit real-mode inline assembly blocks.

pub(super) use std::collections::HashMap;

pub(super) use anyhow::{bail, Result};

pub(super) use crate::backend::ir::{BinOp, Expr, ExprKind, Func, LValue, Literal, Program, Stmt, StructDef, Type};
pub(super) use crate::backend::codegen16;
pub(super) use crate::backend::x64::{Assembler, Cond, Mem, Reg};
pub(super) use crate::obj::elf::Elf64Writer;
pub(super) use crate::obj::flat::FlatWriter;
pub(super) use crate::obj::ObjectWriter;

pub(super) const BASE_VADDR: u64 = 0x400000;
pub(super) const EHDR_SIZE: u64 = 64;
pub(super) const PHDR_SIZE: u64 = 56;

#[derive(Clone, Debug)]
pub(super) struct Slot {
    offset: i32,
    #[allow(dead_code)]
    size: usize,
}

#[derive(Clone, Debug)]
pub(super) struct StructLayout {
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

pub(super) fn compile_flat_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    if prog.arch.as_deref() != Some("x86_16") {
        bail!(
            "flat binary target {} is not supported",
            prog.arch.as_deref().unwrap_or("(unknown)")
        );
    }

    let code = codegen16::compile_program(prog)?;

    if code.len() > 510 {
        bail!(
            "boot sector code is {} bytes, exceeding the 510-byte limit",
            code.len()
        );
    }

    Ok(Box::new(FlatWriter::new(code, true)))
}

pub(super) fn compile_elf_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    let mut cg = CodeGen::new(prog);

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
        let op = cg.asm.new_label();
        cg.func_labels.insert("_dev_open".to_string(), op);
        let ls = cg.asm.new_label();
        cg.func_labels.insert("_dev_lseek".to_string(), ls);
        let un = cg.asm.new_label();
        cg.func_labels.insert("_dev_unlink".to_string(), un);
        let fk = cg.asm.new_label();
        cg.func_labels.insert("_dev_fork".to_string(), fk);
        let fc = cg.asm.new_label();
        cg.func_labels.insert("_dev_fcntl".to_string(), fc);
        let ss = cg.asm.new_label();
        cg.func_labels.insert("_dev_setsockopt".to_string(), ss);
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

    for f in &prog.funcs {
        cg.emit_func(f)?;
    }

    if prog.hosted {
        cg.emit_runtime(start_label)?;
    }

    let rodata_start = cg.asm.new_label();
    cg.bind_label(rodata_start);

    let mut globals: Vec<(String, u32, Literal, Type)> = cg
        .prog
        .globals
        .iter()
        .map(|g| (g.name.clone(), *cg.global_labels.get(&g.name).unwrap(), g.init.clone(), g.ty.clone()))
        .collect();
    globals.sort_by_key(|(_, lab, _, _)| *lab);
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

    let rodata_start_off = *cg.label_offsets.get(&rodata_start).unwrap_or(&0);
    let rodata_vaddr = BASE_VADDR + text_offset + code.len() as u64;
    for (g_lab, s_lab) in string_patches {
        let g_off = *cg.label_offsets.get(&g_lab).unwrap_or(&0);
        let s_off = *cg.label_offsets.get(&s_lab).unwrap_or(&0);
        let slot = g_off - rodata_start_off;
        let addr = rodata_vaddr + (s_off - rodata_start_off) as u64;
        rodata[slot..slot + 8].copy_from_slice(&addr.to_le_bytes());
    }

    let first_seg_end = text_offset + code.len() as u64 + rodata.len() as u64;
    let data_offset = align_up_u64(first_seg_end, PAGE_SIZE);
    let data_vaddr = BASE_VADDR + data_offset;

    let mut data: Vec<u8> = Vec::new();
    if let Some(patch_off) = cg.rand_seed_patch {
        code[patch_off..patch_off + 8].copy_from_slice(&data_vaddr.to_le_bytes());
        data.extend_from_slice(&[0; 8]);
    }

    let mut bss_size: u64 = 0;
    if let Some(alloc_patch) = cg.alloc_ptr_patch {
        const ARENA_SIZE: u64 = 64 * 1024;
        let bump_data_off = data.len() as u64;
        data.extend_from_slice(&[0u8; 8]); // placeholder for the bump pointer
        let bump_ptr_vaddr = data_vaddr + bump_data_off;
        code[alloc_patch..alloc_patch + 8].copy_from_slice(&bump_ptr_vaddr.to_le_bytes());
        let arena_base = data_vaddr + data.len() as u64;
        data[bump_data_off as usize..bump_data_off as usize + 8]
            .copy_from_slice(&arena_base.to_le_bytes());
        bss_size = ARENA_SIZE;
    }

    Ok(Box::new(Elf64Writer::new(code, rodata, data, bss_size, entry_vaddr)))
}

pub(super) const PAGE_SIZE: u64 = 0x1000;

pub(super) fn align_up_u64(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    ((value + align - 1) / align) * align
}

mod expr;
mod layout;
mod runtime;

impl<'p> CodeGen<'p> {
    pub(super) fn new(prog: &'p Program) -> Self {
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

    pub(super) fn bind_label(&mut self, lab: u32) {
        let off = self.asm.len();
        self.asm.bind(lab);
        self.label_offsets.insert(lab, off);
    }

    pub(super) fn emit_func(&mut self, f: &Func) -> Result<()> {
        self.locals.clear();
        self.frame_size = 0;
        let entry = *self.func_labels.get(&f.name).unwrap();
        self.bind_label(entry);

        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        let sub_imm_offset = self.asm.len() + 3; // REX + opcode + modrm
        self.asm.sub(Reg::Rsp, 0i32);

        let slot = self.alloc_slot(8, 8);
        self.addr_tmp = slot.offset;

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

        for s in &f.body {
            self.emit_stmt(s)?;
        }

        self.bind_label(self.ret_label);
        self.asm.leave();
        self.asm.ret();

        let frame = align_up(self.frame_size, 16);
        self.asm.patch_i32(sub_imm_offset, frame as i32);

        Ok(())
    }

pub(super) fn emit_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, ty, init } => self.emit_let(name, ty, init),
            Stmt::StackAlloc { name, elem_ty, count } => self.emit_stack_alloc(name, elem_ty, count),
            Stmt::Assign { lhs, rhs } => self.emit_assign(lhs, rhs),
            Stmt::Return(None) => self.emit_return_none(),
            Stmt::Return(Some(e)) => self.emit_return(e),
            Stmt::Expr(e) => self.emit_expr(e),
            Stmt::If { cond, then, else_ } => self.emit_if(cond, then, else_),
            Stmt::While { cond, body } => self.emit_while(cond, body),
            Stmt::For { init, cond, step, body } => self.emit_for(init, cond, step, body),
            Stmt::Break => self.emit_break(),
            Stmt::Continue => self.emit_continue(),
            Stmt::Unsafe(b) => self.emit_unsafe(b),
        }
    }

    fn emit_let(&mut self, name: &str, ty: &Type, init: &Option<Expr>) -> Result<()> {
        let slot = if let Some(s) = self.locals.get(name) {
            s.clone()
        } else {
            let (size, align) = self.var_size_align(ty);
            self.alloc_named_slot(name, size, align)
        };
        if let Some(e) = init {
            self.eval_expr(e)?;
            self.store_scalar(slot.offset);
        }
        let _ = ty;
        Ok(())
    }

    fn emit_stack_alloc(&mut self, name: &str, elem_ty: &Type, count: &usize) -> Result<()> {
        let elem_size = elem_ty.byte_size();
        let raw_size = elem_size * *count;
        let align = elem_size.max(1);
        let raw_slot = self.alloc_slot(raw_size, align);
        let ptr_slot = self.alloc_named_slot(name, 8, 8);
        self.asm
            .lea(Reg::Rax, Mem::base_disp(Reg::Rbp, raw_slot.offset));
        self.store_scalar(ptr_slot.offset);
        Ok(())
    }

    fn emit_assign(&mut self, lhs: &LValue, rhs: &Expr) -> Result<()> {
        self.lvalue_addr(lhs)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rbp, self.addr_tmp), Reg::Rax);
        self.eval_expr(rhs)?;
        self.asm
            .mov(Reg::Rdx, Mem::base_disp(Reg::Rbp, self.addr_tmp));
        let width = self.lvalue_store_width(lhs);
        self.store_width(width, Reg::Rdx, Reg::Rax);
        Ok(())
    }

    fn emit_return_none(&mut self) -> Result<()> {
        self.asm.jmp(self.ret_label);
        Ok(())
    }

    fn emit_return(&mut self, e: &Expr) -> Result<()> {
        self.eval_expr(e)?;
        self.asm.jmp(self.ret_label);
        Ok(())
    }

    fn emit_expr(&mut self, e: &Expr) -> Result<()> {
        self.eval_expr(e)?;
        Ok(())
    }

    fn emit_if(&mut self, cond: &Expr, then: &[Stmt], else_: &Option<Vec<Stmt>>) -> Result<()> {
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
        }
        self.bind_label(end_lab);
        Ok(())
    }

    fn emit_while(&mut self, cond: &Expr, body: &[Stmt]) -> Result<()> {
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
        Ok(())
    }

    fn emit_for(&mut self, init: &Option<Box<Stmt>>, cond: &Expr, step: &Option<Expr>, body: &[Stmt]) -> Result<()> {
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
        Ok(())
    }

    fn emit_break(&mut self) -> Result<()> {
        let end = *self.loop_end_stack.last()
            .ok_or_else(|| anyhow::anyhow!("break outside of loop"))?;
        self.asm.jmp(end);
        Ok(())
    }

    fn emit_continue(&mut self) -> Result<()> {
        let head = *self.loop_head_stack.last()
            .ok_or_else(|| anyhow::anyhow!("continue outside of loop"))?;
        self.asm.jmp(head);
        Ok(())
    }

    fn emit_unsafe(&mut self, b: &[Stmt]) -> Result<()> {
        for st in b {
            self.emit_stmt(st)?;
        }
        Ok(())
    }

    pub(super) fn string_label(&mut self, s: &str) -> u32 {
        if let Some(&lab) = self.string_labels.get(s) {
            return lab;
        }
        let lab = self.asm.new_label();
        self.string_labels.insert(s.to_string(), lab);
        lab
    }

}
pub(super) fn layout_struct(s: &StructDef) -> StructLayout {
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

pub(super) fn type_size(ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 8,
        Type::Struct(name) => panic!("struct size for {} must come from layout table", name),
        _ => 8,
    }
}

impl<'p> CodeGen<'p> {
    pub(super) fn type_size_bytes(&self, ty: &Type) -> usize {
        match ty {
            Type::Struct(name) => {
                self.struct_layouts.get(name)
                    .map(|l| l.size)
                    .unwrap_or_else(|| type_size(ty))
            }
            _ => type_size(ty),
        }
    }

    /// Return the (size, align) for a local variable of the given type.
    pub(super) fn var_size_align(&self, ty: &Type) -> (usize, usize) {
        match ty {
            Type::Struct(name) => {
                if let Some(layout) = self.struct_layouts.get(name) {
                    return (layout.size.max(8), layout.align.max(8));
                }
            }
            _ => {}
        }
        let size = type_size(ty);
        let align = type_align(ty);
        (size.max(8), align.max(8))
    }
}

pub(super) fn ptr_elem_size(ty: &Type) -> Option<usize> {
    match ty {
        Type::Ptr(inner) => Some(type_size(inner)),
        _ => None,
    }
}

pub(super) fn type_align(ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 8,
        Type::Struct(_) => 8,
        _ => 8,
    }
}

pub(super) fn scalar_width(ty: &Type) -> u32 {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 | Type::F32 => 32,
        _ => 64,
    }
}

pub(super) fn ty_width(ty: &Type) -> u32 {
    scalar_width(ty)
}

pub(super) fn abi_reg(idx: usize) -> Result<Reg> {
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

pub(super) fn cond_for_cmp(op: BinOp, ty: &Type) -> Result<Cond> {
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

pub(super) fn align_up(v: usize, align: usize) -> usize {
    if align == 0 {
        return v;
    }
    ((v + align - 1) / align) * align
}
