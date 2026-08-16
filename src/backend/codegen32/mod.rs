//! IR to 32-bit native machine code translator.
//!
//! For the hosted x86_32 target the translator targets the IA-32 cdecl ABI:
//!   - arguments passed on the stack right-to-left,
//!   - return value in EAX,
//!   - stack frame is `push ebp; mov ebp, esp; sub esp, N`.
//!
//! Linux system calls use `int 0x80` with the call number in EAX.

pub(super) use std::collections::HashMap;

pub(super) use anyhow::{Result, bail};

pub(super) use crate::backend::ir::{
    BinOp, Expr, ExprKind, Func, LValue, Literal, Program, Stmt, StructDef, Type,
};
pub(super) use crate::backend::x86::{Assembler, Cond, Mem, Reg};
pub(super) use crate::obj::ObjectWriter;
pub(super) use crate::obj::elf32::Elf32Writer;

pub(super) const BASE_VADDR: u32 = 0x08048000;
pub(super) const EHDR_SIZE: u32 = 52;
pub(super) const PHDR_SIZE: u32 = 32;

#[derive(Clone, Debug)]
pub(super) struct Slot {
    offset: i32,
    #[allow(dead_code)]
    size: usize,
}

#[derive(Clone, Debug)]
pub(super) struct StructLayout {
    size: usize,
    #[allow(dead_code)]
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
    /// Whether the current function returns a struct larger than 4 bytes
    /// through the i386 sret convention (hidden first arg = caller-allocated
    /// struct pointer; callee writes the struct there and returns the same
    /// pointer in EAX).
    sret: bool,
    string_patches: Vec<(usize, u32)>,
    /// (global label, string label) pairs for string-initialized constants,
    /// patched once the rodata layout is known.
    global_string_patches: Vec<(u32, u32)>,
    /// Code offset of the `mov eax, <bump_ptr_vaddr>` immediate in `_dev_alloc`,
    /// patched once the `.data` segment layout is known.  `None` when the program
    /// does not use the bump allocator.
    alloc_ptr_patch: Option<usize>,
    loop_end_stack: Vec<u32>,
    loop_head_stack: Vec<u32>,
}

pub fn compile_program(prog: &Program) -> Result<Box<dyn ObjectWriter>> {
    let struct_layouts = compute_struct_layouts(&prog.structs)?;
    let mut cg = CodeGen::new(prog, struct_layouts);

    for f in &prog.funcs {
        let lab = cg.asm.new_label();
        cg.func_labels.insert(f.name.clone(), lab);
    }
    for g in &prog.globals {
        let lab = cg.asm.new_label();
        cg.global_labels.insert(g.name.clone(), lab);
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
        let gt = cg.asm.new_label();
        cg.func_labels.insert("_dev_gettimeofday".to_string(), gt);
        let e = cg.asm.new_label();
        cg.func_labels.insert("_dev_exit".to_string(), e);
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
        let w = cg.asm.new_label();
        cg.func_labels.insert("_dev_write".to_string(), w);
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
        let wp = cg.asm.new_label();
        cg.func_labels.insert("_dev_waitpid".to_string(), wp);
        let lb = cg.asm.new_label();
        cg.func_labels.insert("_dev_lfence".to_string(), lb);
        let sb = cg.asm.new_label();
        cg.func_labels.insert("_dev_sfence".to_string(), sb);
        let mb = cg.asm.new_label();
        cg.func_labels.insert("_dev_mfence".to_string(), mb);
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
        // Freestanding entry: the `ENTRY` directive from a linker script,
        // falling back to the conventional `_start`.
        let entry_name = prog
            .config
            .as_ref()
            .map(|c| c.entry.as_str())
            .unwrap_or("_start");
        *cg.func_labels.get(entry_name).ok_or_else(|| {
            anyhow::anyhow!("freestanding mode requires a {} function", entry_name)
        })?
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
        .map(|g| {
            (
                g.name.clone(),
                *cg.global_labels.get(&g.name).unwrap(),
                g.init.clone(),
                g.ty.clone(),
            )
        })
        .collect();
    globals.sort_by_key(|(_, lab, _, _)| *lab);
    for (_, lab, init, ty) in globals {
        cg.bind_label(lab);
        let value = match init {
            Literal::Int(v) => {
                let size = ty.byte_size()?;
                match size {
                    1 => (v as i8).to_le_bytes().to_vec(),
                    2 => (v as i16).to_le_bytes().to_vec(),
                    _ => (v as i32).to_le_bytes().to_vec(),
                }
            }
            Literal::Bool(v) => {
                let val = if v { 1i8 } else { 0i8 };
                val.to_le_bytes().to_vec()
            }
            Literal::Char(v) => v.to_le_bytes().to_vec(),
            Literal::String(s) => {
                let s_lab = cg.string_label(&s);
                cg.global_string_patches.push((lab, s_lab));
                vec![0; 4]
            }
            Literal::Bytes(b) => {
                let s = unsafe { String::from_utf8_unchecked(b.clone()) };
                let s_lab = cg.string_label(&s);
                cg.global_string_patches.push((lab, s_lab));
                vec![0; 4]
            }
            _ => vec![0; 4],
        };
        let mut padded = value;
        padded.resize(4, 0);
        cg.asm.append_bytes(&padded);
    }

    let mut strings: Vec<(String, u32)> = cg.string_labels.drain().collect();
    strings.sort_by_key(|(_, lab)| *lab);
    for (s, lab) in strings {
        cg.bind_label(lab);
        cg.asm.append_bytes(s.as_bytes());
        cg.asm.push_byte(0);
    }

    let bytes = cg.asm.into_bytes()?;
    let split = *cg.label_offsets.get(&rodata_start).unwrap_or(&bytes.len());
    let mut code = bytes[..split].to_vec();
    let mut rodata = bytes[split..].to_vec();

    let text_offset = EHDR_SIZE + PHDR_SIZE * 2;
    let entry_vaddr =
        BASE_VADDR + text_offset + *cg.label_offsets.get(&start_label).unwrap_or(&0) as u32;

    for (patch_off, label) in &cg.string_patches {
        let label_off = *cg.label_offsets.get(label).unwrap_or(&0) as u32;
        let abs = BASE_VADDR + text_offset + label_off;
        code[*patch_off..*patch_off + 4].copy_from_slice(&abs.to_le_bytes());
    }

    let rodata_start_off = *cg.label_offsets.get(&rodata_start).unwrap_or(&0);
    let rodata_vaddr = BASE_VADDR + text_offset + code.len() as u32;
    for (g_lab, s_lab) in cg.global_string_patches {
        let g_off = *cg.label_offsets.get(&g_lab).unwrap_or(&0);
        let s_off = *cg.label_offsets.get(&s_lab).unwrap_or(&0);
        let slot = g_off - rodata_start_off;
        let addr = rodata_vaddr + (s_off - rodata_start_off) as u32;
        rodata[slot..slot + 4].copy_from_slice(&addr.to_le_bytes());
    }

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
        let arena_base = data_vaddr + data.len() as u32;
        data[bump_data_off as usize..bump_data_off as usize + 4]
            .copy_from_slice(&arena_base.to_le_bytes());
        bss_size = ARENA_SIZE;
    }

    Ok(Box::new(Elf32Writer::new(
        code,
        rodata,
        data,
        bss_size,
        entry_vaddr,
    )))
}

mod expr;
mod layout;
mod runtime;

use layout::compute_struct_layouts;

impl<'p> CodeGen<'p> {
    pub(super) fn type_size_bytes(&self, ty: &Type) -> usize {
        match ty {
            Type::Struct(name) => self
                .struct_layouts
                .get(name)
                .map(|l| l.size)
                .unwrap_or_else(|| type_size(ty, &self.struct_layouts)),
            _ => type_size(ty, &self.struct_layouts),
        }
    }

    /// Return the (size, align) for a local variable of the given type.
    /// Struct locals are allocated with their full byte size (minimum 4)
    /// so multi-field structs are laid out inline instead of being
    /// truncated to the first 4-byte slot.
    pub(super) fn var_size_align(&self, ty: &Type) -> (usize, usize) {
        match ty {
            Type::Struct(name) => {
                if let Some(layout) = self.struct_layouts.get(name) {
                    return (layout.size.max(4), 4);
                }
            }
            _ => {}
        }
        let size = type_size(ty, &self.struct_layouts);
        (size.max(4), 4)
    }

    /// Copy `size` bytes from the struct data at `[rbp+src]` into the struct
    /// data at `[rbp+dst]`.  Uses 4-byte moves for the aligned bulk of the
    /// copy, then a trailing 1/2/3-byte move for any remainder.  `size` must
    /// be > 0.
    pub(super) fn copy_struct_bytes(&mut self, dst: i32, src: i32, size: usize) -> Result<()> {
        self.asm.lea(Reg::Edi, Mem::base_disp(Reg::Ebp, dst))?;
        self.asm.lea(Reg::Esi, Mem::base_disp(Reg::Ebp, src))?;
        let mut remaining = size;
        while remaining >= 4 {
            self.asm.mov(Reg::Ecx, Mem::base(Reg::Esi))?;
            self.asm.store32(Mem::base(Reg::Edi), Reg::Ecx)?;
            self.asm.add(Reg::Edi, 4i32)?;
            self.asm.add(Reg::Esi, 4i32)?;
            remaining -= 4;
        }
        if remaining > 0 {
            match remaining {
                1 => {
                    self.asm.movzx8(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store8(Mem::base(Reg::Edi), Reg::Ecx)?;
                }
                2 => {
                    self.asm.mov(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store16(Mem::base(Reg::Edi), Reg::Ecx)?;
                }
                3 => {
                    self.asm.movzx8(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store8(Mem::base(Reg::Edi), Reg::Ecx)?;
                    self.asm.add(Reg::Esi, 1i32)?;
                    self.asm.add(Reg::Edi, 1i32)?;
                    self.asm.mov(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store16(Mem::base(Reg::Edi), Reg::Ecx)?;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    /// Copy `size` bytes of struct data from the address held in `src_reg`
    /// (i.e. `[*src_reg]`) into the local slot at `[rbp+dst]`.
    pub(super) fn copy_ptr_to_slot(&mut self, dst: i32, src_reg: Reg, size: usize) -> Result<()> {
        self.asm.lea(Reg::Edi, Mem::base_disp(Reg::Ebp, dst))?;
        self.asm.mov(Reg::Esi, src_reg)?;
        let mut remaining = size;
        while remaining >= 4 {
            self.asm.mov(Reg::Ecx, Mem::base(Reg::Esi))?;
            self.asm.store32(Mem::base(Reg::Edi), Reg::Ecx)?;
            self.asm.add(Reg::Edi, 4i32)?;
            self.asm.add(Reg::Esi, 4i32)?;
            remaining -= 4;
        }
        if remaining > 0 {
            match remaining {
                1 => {
                    self.asm.movzx8(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store8(Mem::base(Reg::Edi), Reg::Ecx)?;
                }
                2 => {
                    self.asm.mov(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store16(Mem::base(Reg::Edi), Reg::Ecx)?;
                }
                3 => {
                    self.asm.movzx8(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store8(Mem::base(Reg::Edi), Reg::Ecx)?;
                    self.asm.add(Reg::Esi, 1i32)?;
                    self.asm.add(Reg::Edi, 1i32)?;
                    self.asm.mov(Reg::Ecx, Mem::base(Reg::Esi))?;
                    self.asm.store16(Mem::base(Reg::Edi), Reg::Ecx)?;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    pub(super) fn new(prog: &'p Program, struct_layouts: HashMap<String, StructLayout>) -> Self {
        Self {
            prog,
            asm: Assembler::new(),
            label_offsets: HashMap::new(),
            func_labels: HashMap::new(),
            string_labels: HashMap::new(),
            global_labels: HashMap::new(),
            locals: HashMap::new(),
            frame_size: 0,
            struct_layouts,
            addr_tmp: 0,
            ret_label: 0,
            sret: false,
            string_patches: Vec::new(),
            global_string_patches: Vec::new(),
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

        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        let sub_imm_offset = self.asm.len() + 2; // opcode + modrm
        self.asm.sub(Reg::Esp, 0i32)?;

        let slot = self.alloc_slot(4, 4);
        self.addr_tmp = slot.offset;

        // i386 sret: a struct return larger than 4 bytes is written through a
        // hidden first argument (the caller-allocated struct pointer at
        // EBP+8); real named parameters start one slot higher.
        let sret = matches!(&f.ret, Type::Struct(_))
            && !Self::is_enum_struct(&f.ret)
            && self.value_size(&f.ret).map_or(false, |s| s > 4);
        self.sret = sret;
        let arg0 = if sret { 8 } else { 0 };

        for (name, _ty) in &f.params {
            self.alloc_named_slot(name, 4, 4);
        }
        for (i, (name, _ty)) in f.params.iter().enumerate() {
            let slot = self.locals.get(name).unwrap();
            self.asm.mov(
                Reg::Eax,
                Mem::base_disp(Reg::Ebp, (8 + arg0 + i * 4) as i32),
            )?;
            self.asm
                .mov(Mem::base_disp(Reg::Ebp, slot.offset), Reg::Eax)?;
        }

        self.ret_label = self.asm.new_label();

        for s in &f.body {
            self.emit_stmt(s)?;
        }

        self.bind_label(self.ret_label);
        self.asm.leave();
        self.asm.ret()?;

        let frame = align_up(self.frame_size, 16);
        self.asm.patch_i32(sub_imm_offset, frame as i32);

        Ok(())
    }

    pub(super) fn emit_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, ty, init } => {
                let (size, align) = self.var_size_align(ty);
                let slot = self.alloc_named_slot(name, size, align);
                if let Some(e) = init {
                    // Struct-typed initializers are laid out inline in the
                    // local's slot (not via the 4-byte EAX round-trip), so
                    // multi-field struct copies preserve every field.  The
                    // source address is computed without going through EAX so
                    // values larger than 4 bytes round-trip correctly.
                    // Synthetic `__enum_*` structs are excluded: their values
                    // are 4-byte pointers and must round-trip as scalars.
                    if let Type::Struct(_) = &e.ty {
                        if Self::is_enum_struct(&e.ty) {
                            self.eval_expr(e)?;
                            self.store_scalar(slot.offset)?;
                            return Ok(());
                        }
                        let src_off = match &e.kind {
                            ExprKind::Var(n) => self
                                .locals
                                .get(n)
                                .map(|s| s.offset)
                                .ok_or_else(|| anyhow::anyhow!("unknown variable: {}", n))?,
                            _ => {
                                // Struct literal / enum variant / block: eval
                                // leaves the source address in EAX.
                                self.eval_expr(e)?;
                                self.copy_ptr_to_slot(slot.offset, Reg::Eax, size)?;
                                return Ok(());
                            }
                        };
                        self.copy_struct_bytes(slot.offset, src_off, size)?;
                    } else {
                        self.eval_expr(e)?;
                        self.store_scalar(slot.offset)?;
                    }
                }
                let _ = (size, align);
            }
            Stmt::StackAlloc {
                name,
                elem_ty,
                count,
            } => {
                let elem_size = elem_ty.byte_size()?;
                let raw_size = elem_size * *count;
                let align = elem_size.max(1);
                let raw_slot = self.alloc_slot(raw_size, align);
                let ptr_slot = self.alloc_named_slot(name, 4, 4);
                self.asm
                    .lea(Reg::Eax, Mem::base_disp(Reg::Ebp, raw_slot.offset))?;
                self.store_scalar(ptr_slot.offset)?;
            }
            Stmt::Assign { lhs, rhs } => {
                self.lvalue_addr(lhs)?; // address in EAX
                self.asm
                    .mov(Mem::base_disp(Reg::Ebp, self.addr_tmp), Reg::Eax)?;
                // Struct-typed assignments copy the full struct (the RHS
                // evaluates to a source address in EAX) instead of a single
                // 32-bit store, so multi-field structs are preserved.
                if let Type::Struct(_) = &rhs.ty {
                    if Self::is_enum_struct(&rhs.ty) {
                        self.eval_expr(rhs)?; // pointer value in EAX
                        self.asm
                            .mov(Reg::Edx, Mem::base_disp(Reg::Ebp, self.addr_tmp))?;
                        let width = self.lvalue_store_width(lhs);
                        self.store_width(width, Reg::Edx, Reg::Eax)?;
                    } else {
                        self.eval_expr(rhs)?; // source address in EAX
                        let size = self.value_size(&rhs.ty).unwrap_or(4).max(4);
                        self.asm.mov(Reg::Esi, Reg::Eax)?;
                        self.asm
                            .mov(Reg::Edi, Mem::base_disp(Reg::Ebp, self.addr_tmp))?;
                        self.copy_mem_to_mem(Reg::Edi, Reg::Esi, size)?;
                    }
                } else {
                    self.eval_expr(rhs)?; // value in EAX
                    self.asm
                        .mov(Reg::Edx, Mem::base_disp(Reg::Ebp, self.addr_tmp))?;
                    let width = self.lvalue_store_width(lhs);
                    self.store_width(width, Reg::Edx, Reg::Eax)?;
                }
            }
            Stmt::Return(None) => {
                self.asm.jmp(self.ret_label)?;
            }
            Stmt::Return(Some(e)) => {
                if self.sret && matches!(&e.ty, Type::Struct(_)) && !Self::is_enum_struct(&e.ty) {
                    // i386 sret: write the struct into `*[EBP+8]` and return
                    // that caller-allocated pointer in EAX.
                    self.eval_expr(e)?; // source address in EAX
                    let size = self.value_size(&e.ty).unwrap_or(4).max(4);
                    self.asm
                        .mov(Mem::base_disp(Reg::Ebp, self.addr_tmp), Reg::Eax)?;
                    // copy from [*EAX] into *[EBP+8] (the caller-allocated sret slot)
                    self.asm
                        .mov(Reg::Esi, Mem::base_disp(Reg::Ebp, self.addr_tmp))?;
                    self.asm.mov(Reg::Edi, Mem::base_disp(Reg::Ebp, 8))?;
                    self.copy_mem_to_mem(Reg::Edi, Reg::Esi, size)?;
                    // i386 ABI: return the caller-allocated pointer (arg0)
                    // that the struct was written into.
                    self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?;
                } else {
                    self.eval_expr(e)?;
                }
                self.asm.jmp(self.ret_label)?;
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
                self.asm.test(Reg::Eax, Reg::Eax)?;
                if let Some(l) = else_lab {
                    self.asm.je(l)?;
                } else {
                    self.asm.je(end_lab)?;
                }
                self.bind_label(then_lab);
                for st in then {
                    self.emit_stmt(st)?;
                }
                self.asm.jmp(end_lab)?;
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
                self.asm.test(Reg::Eax, Reg::Eax)?;
                self.asm.je(end)?;
                for st in body {
                    self.emit_stmt(st)?;
                }
                self.asm.jmp(head)?;
                self.bind_label(end);
                self.loop_head_stack.pop();
                self.loop_end_stack.pop();
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(i) = init {
                    self.emit_stmt(i)?;
                }
                let head = self.asm.new_label();
                let end = self.asm.new_label();
                self.loop_head_stack.push(head);
                self.loop_end_stack.push(end);
                self.bind_label(head);
                self.eval_expr(cond)?;
                self.asm.test(Reg::Eax, Reg::Eax)?;
                self.asm.je(end)?;
                for st in body {
                    self.emit_stmt(st)?;
                }
                if let Some(st) = step {
                    self.eval_expr(st)?;
                }
                self.asm.jmp(head)?;
                self.bind_label(end);
                self.loop_head_stack.pop();
                self.loop_end_stack.pop();
            }
            Stmt::Break => {
                let end = *self
                    .loop_end_stack
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("break outside of loop"))?;
                self.asm.jmp(end)?;
            }
            Stmt::Continue => {
                let head = *self
                    .loop_head_stack
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("continue outside of loop"))?;
                self.asm.jmp(head)?;
            }
            Stmt::Unsafe(b) => {
                for st in b {
                    self.emit_stmt(st)?;
                }
            }
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
pub(super) fn layout_struct(
    s: &StructDef,
    struct_layouts: &HashMap<String, StructLayout>,
) -> StructLayout {
    let mut offset = 0usize;
    let mut offsets = Vec::with_capacity(s.fields.len());
    for (_name, ty) in &s.fields {
        let size = type_size(ty, struct_layouts);
        let align = type_align(ty);
        offset = align_up(offset, align);
        offsets.push(offset);
        offset += size;
    }
    let align = s
        .fields
        .iter()
        .map(|(_, ty)| type_align(ty))
        .max()
        .unwrap_or(1);
    StructLayout {
        size: align_up(offset, align),
        align,
        offsets,
    }
}

pub(super) fn type_size(ty: &Type, struct_layouts: &HashMap<String, StructLayout>) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 4,
        Type::Struct(name) => struct_layouts.get(name).map(|l| l.size).unwrap_or(4),
        Type::Slice(_) => 4,
        _ => 4,
    }
}

impl<'p> CodeGen<'p> {
    pub(super) fn ptr_elem_size(&self, ty: &Type) -> Option<usize> {
        match ty {
            Type::Ptr(inner) => Some(self.type_size_bytes(inner)),
            _ => None,
        }
    }
}

pub(super) fn type_align(ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 => 2,
        Type::I32 | Type::U32 | Type::F32 => 4,
        Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) => 4,
        Type::Struct(_) => 4,
        _ => 4,
    }
}

pub(super) fn scalar_width(ty: &Type) -> u32 {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 | Type::F32 => 32,
        _ => 32,
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
    v.div_ceil(align) * align
}

pub(super) fn align_up_u32(v: u32, align: u32) -> u32 {
    if align == 0 {
        return v;
    }
    v.div_ceil(align) * align
}
