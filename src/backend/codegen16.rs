//! 16-bit real-mode code cgerator for bare-metal boot targets.
//!
//! This backend consumes the same typed IR as the x86-64 backend and emits
//! flat 16-bit x86 machine code suitable for a PC boot sector.  It supports a
//! small but complete subset of Forge: functions, calls, scalar locals,
//! control flow, raw pointer load/store, arithmetic, and string literals.

use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;

use crate::backend::ir::{
    BinOp, Expr, ExprKind, Func, LValue, Literal, Program, Stmt, StructDef, Type,
};

/// Compile an IR program to a 16-bit real-mode flat binary payload.
///
/// The returned bytes are the raw machine code and data; the caller is
/// responsible for padding to a boot sector and appending the signature.
pub fn compile_program(prog: &Program) -> Result<Vec<u8>> {
    let mut cg = CodeGen16::new(prog);
    cg.emit_program()?;
    cg.finish()
}

// -----------------------------------------------------------------------------
// Code cgerator
// -----------------------------------------------------------------------------

struct CodeGen16<'p> {
    prog: &'p Program,
    asm: Encoder,
    locals: HashMap<String, Slot16>,
    arrays: HashMap<String, i8>,
    frame_size: u8,
    func_labels: HashMap<String, u32>,
    string_labels: HashMap<String, u32>,
    ret_label: u32,
    loop_end_stack: Vec<u32>,
    loop_head_stack: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Slot16 {
    offset: i8,
    width: u8,
    signed: bool,
}

impl<'p> CodeGen16<'p> {
    fn new(prog: &'p Program) -> Self {
        Self {
            prog,
            asm: Encoder::new(),
            locals: HashMap::new(),
            arrays: HashMap::new(),
            frame_size: 0,
            func_labels: HashMap::new(),
            string_labels: HashMap::new(),
            ret_label: 0,
            loop_end_stack: Vec::new(),
            loop_head_stack: Vec::new(),
        }
    }

    fn emit_program(&mut self) -> Result<()> {
        // Ensure `_start` is emitted first so the boot sector entry point is at
        // the very beginning of the image.
        let mut funcs: Vec<&Func> = Vec::new();
        let mut start: Option<&Func> = None;
        for f in &self.prog.funcs {
            if f.name == "_start" {
                start = Some(f);
            } else {
                funcs.push(f);
            }
        }
        let start = start.ok_or_else(|| anyhow!("flat binary boot target requires a _start function"))?;

        // Reserve labels for user functions and tiny runtime helpers.
        let start_lab = self.asm.new_label();
        self.func_labels.insert("_start".to_string(), start_lab);
        for f in &funcs {
            let lab = self.asm.new_label();
            self.func_labels.insert(f.name.clone(), lab);
        }
        let teletype_lab = self.asm.new_label();
        let halt_lab = self.asm.new_label();
        let load_char_lab = self.asm.new_label();
        let serial_lab = self.asm.new_label();
        self.func_labels
            .insert("_dev_bios_teletype".to_string(), teletype_lab);
        self.func_labels.insert("_dev_halt".to_string(), halt_lab);
        self.func_labels.insert("_dev_load_char".to_string(), load_char_lab);
        self.func_labels.insert("_dev_serial_putc".to_string(), serial_lab);

        // Emit user functions, with _start at the front.
        self.emit_func(start, true)?;
        for f in &funcs {
            self.emit_func(f, false)?;
        }

        // Emit runtime helpers.
        self.emit_builtins(teletype_lab, halt_lab, load_char_lab, serial_lab)?;

        // Append string literals as data at the end of the image.
        let mut strings: Vec<(u32, &str)> = self
            .string_labels
            .iter()
            .map(|(s, lab)| (*lab, s.as_str()))
            .collect();
        strings.sort_by_key(|(lab, _)| *lab);
        for (lab, s) in strings {
            self.asm.bind(lab);
            self.asm.db_str(s);
        }

        Ok(())
    }

    fn finish(self) -> Result<Vec<u8>> {
        self.asm.into_bytes()
    }

    // -----------------------------------------------------------------------
    // Functions / locals
    // -----------------------------------------------------------------------

    fn emit_func(&mut self, f: &Func, is_start: bool) -> Result<()> {
        let lab = *self.func_labels.get(&f.name).unwrap();
        self.asm.bind(lab);

        // Precompute the stack frame.
        self.locals.clear();
        self.arrays.clear();
        self.frame_size = 0;
        self.scan_func(f)?;
        let frame = align_up_u8(self.frame_size, 2);

        // Boot-sector entry point: set up the segment registers and stack before
        // anything else touches memory.
        if is_start {
            self.emit_segment_setup();
        }

        // Standard BP-based frame.
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        if frame > 0 {
            self.asm.sub_sp_imm(frame as i16)?;
        }

        // Copy incoming arguments from the stack into their local slots.
        for (i, (name, _ty)) in f.params.iter().enumerate() {
            let slot = *self.locals.get(name).ok_or_else(|| anyhow!("missing param slot: {}", name))?;
            let arg_off = (4 + i * 2) as i8;
            self.asm.load16_bp(Reg16::Ax, arg_off);
            self.store_slot(slot, Reg16::Ax, Reg8::Al);
        }

        self.ret_label = self.asm.new_label();
        for s in &f.body {
            self.emit_stmt(s)?;
        }
        self.asm.bind(self.ret_label);
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp); // mov sp, bp
        self.asm.pop(Reg16::Bp);
        self.asm.ret();
        Ok(())
    }

    fn emit_segment_setup(&mut self) {
        self.asm.xor_ax_ax();
        self.asm.mov_seg_ax(SegReg::Ds);
        self.asm.mov_seg_ax(SegReg::Es);
        self.asm.mov_seg_ax(SegReg::Ss);
        self.asm.mov16_imm(Reg16::Sp, 0x7C00);
    }

    fn emit_builtins(
        &mut self,
        teletype_lab: u32,
        halt_lab: u32,
        load_char_lab: u32,
        serial_lab: u32,
    ) -> Result<()> {
        // _dev_bios_teletype(c: char) -> void
        // AH=0x0E, BH=0x00, BL=0x07, AL=character.
        self.asm.bind(teletype_lab);
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        self.asm.load8_bp(Reg8::Al, 4);
        self.asm.mov8_imm(Reg8::Ah, 0x0E);
        self.asm.mov8_imm(Reg8::Bh, 0x00);
        self.asm.mov8_imm(Reg8::Bl, 0x07);
        self.asm.int(0x10);
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
        self.asm.pop(Reg16::Bp);
        self.asm.ret();

        // _dev_halt() -> never returns
        self.asm.bind(halt_lab);
        self.asm.cli();
        self.asm.hlt();

        // _dev_load_char(p: ptr[char]) -> char
        self.asm.bind(load_char_lab);
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        self.asm.load16_bp(Reg16::Si, 4);
        self.asm.load8_si(Reg8::Al);
        self.asm.xor_ah_ah();
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
        self.asm.pop(Reg16::Bp);
        self.asm.ret();

        // _dev_serial_putc(c: char) -> void
        // Writes the character to COM1 so -nographic captures it on stdout.
        self.asm.bind(serial_lab);
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        self.asm.load8_bp(Reg8::Al, 4);
        self.asm.out_imm8_al(0xE9); // Bochs/QEMU debug port
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
        self.asm.pop(Reg16::Bp);
        self.asm.ret();
        Ok(())
    }

    fn scan_func(&mut self, f: &Func) -> Result<()> {
        // Parameters are also mutable local slots.
        for (name, ty) in &f.params {
            self.alloc_named(name, ty)?;
        }
        for s in &f.body {
            self.scan_stmt(s)?;
        }
        Ok(())
    }

    fn scan_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, ty, init: _ } => {
                let _ = self.alloc_named(name, ty)?;
            }
            Stmt::StackAlloc {
                name,
                elem_ty,
                count,
            } => {
                let (size, _signed) = type_info(elem_ty)?;
                let raw_size = size as usize * *count;
                let align = size.max(1);
                let raw_off = self.alloc_slot(raw_size as u8, align);
                let ptr_off = self.alloc_named(name, &Type::Ptr(Box::new(elem_ty.clone())))?;
                self.arrays.insert(name.clone(), raw_off);
                let _ = ptr_off;
            }
            Stmt::If { then, else_, .. } => {
                for s in then {
                    self.scan_stmt(s)?;
                }
                if let Some(b) = else_ {
                    for s in b {
                        self.scan_stmt(s)?;
                    }
                }
            }
            Stmt::While { body, .. } | Stmt::Unsafe(body) => {
                for s in body {
                    self.scan_stmt(s)?;
                }
            }
            Stmt::For { init, body, step, .. } => {
                if let Some(i) = init {
                    self.scan_stmt(i)?;
                }
                for s in body {
                    self.scan_stmt(s)?;
                }
                let _ = step;
            }
            _ => {}
        }
        Ok(())
    }

    fn alloc_named(&mut self, name: &str, ty: &Type) -> Result<Slot16> {
        let (size, signed) = type_info(ty)?;
        let align = size.max(1);
        let offset = self.alloc_slot(size, align);
        let slot = Slot16 {
            offset,
            width: size,
            signed,
        };
        self.locals.insert(name.to_string(), slot);
        Ok(slot)
    }

    fn alloc_slot(&mut self, size: u8, align: u8) -> i8 {
        let aligned = align_up_u8(self.frame_size, align);
        self.frame_size = aligned + size;
        -(self.frame_size as i8)
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    fn emit_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, init, .. } => {
                let slot = *self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown local: {}", name))?;
                if let Some(e) = init {
                    self.eval_expr(e)?;
                    self.store_slot(slot, Reg16::Ax, Reg8::Al);
                }
            }
            Stmt::StackAlloc { name, .. } => {
                let raw_off = *self
                    .arrays
                    .get(name)
                    .ok_or_else(|| anyhow!("missing array base: {}", name))?;
                let slot = *self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("missing array pointer slot: {}", name))?;
                self.asm.lea_bp(Reg16::Ax, raw_off);
                self.store_slot(slot, Reg16::Ax, Reg8::Al);
            }
            Stmt::Assign { lhs, rhs } => {
                self.lvalue_addr(lhs)?;
                self.asm.push(Reg16::Ax);
                self.eval_expr(rhs)?;
                self.asm.pop(Reg16::Si);
                let (width, _) = self.lvalue_width(lhs)?;
                match width {
                    1 => self.asm.store8_si(Reg8::Al),
                    2 => self.asm.store16_si(Reg16::Ax),
                    _ => bail!("unsupported store width: {}", width),
                }
            }
            Stmt::Return(None) => self.asm.jmp_short_lab(self.ret_label),
            Stmt::Return(Some(e)) => {
                self.eval_expr(e)?;
                self.asm.jmp_short_lab(self.ret_label);
            }
            Stmt::Expr(e) => {
                self.eval_expr(e)?;
            }
            Stmt::If { cond, then, else_ } => self.emit_if(cond, then, else_.as_deref())?,
            Stmt::While { cond, body } => self.emit_while(cond, body)?,
            Stmt::For { init, cond, step, body } => self.emit_for(init.as_deref(), cond, step.as_ref(), body)?,
            Stmt::Break => {
                let end = *self.loop_end_stack.last()
                    .ok_or_else(|| anyhow!("break outside of loop"))?;
                self.asm.jmp_short_lab(end);
            }
            Stmt::Continue => {
                let head = *self.loop_head_stack.last()
                    .ok_or_else(|| anyhow!("continue outside of loop"))?;
                self.asm.jmp_short_lab(head);
            }
            Stmt::Unsafe(body) => {
                for s in body {
                    self.emit_stmt(s)?;
                }
            }
        }
        Ok(())
    }

    fn emit_if(&mut self, cond: &Expr, then: &[Stmt], else_: Option<&[Stmt]>) -> Result<()> {
        let end_lab = self.asm.new_label();
        self.eval_expr(cond)?;
        self.asm.test_ax_ax();
        if let Some(else_body) = else_ {
            let else_lab = self.asm.new_label();
            self.asm.je_short_lab(else_lab);
            for s in then {
                self.emit_stmt(s)?;
            }
            self.asm.jmp_short_lab(end_lab);
            self.asm.bind(else_lab);
            for s in else_body {
                self.emit_stmt(s)?;
            }
        } else {
            self.asm.je_short_lab(end_lab);
            for s in then {
                self.emit_stmt(s)?;
            }
        }
        self.asm.bind(end_lab);
        Ok(())
    }

    fn emit_while(&mut self, cond: &Expr, body: &[Stmt]) -> Result<()> {
        let head = self.asm.new_label();
        let end = self.asm.new_label();
        self.loop_head_stack.push(head);
        self.loop_end_stack.push(end);
        self.asm.bind(head);
        self.eval_expr(cond)?;
        self.asm.test_ax_ax();
        self.asm.je_short_lab(end);
        for s in body {
            self.emit_stmt(s)?;
        }
        self.asm.jmp_short_lab(head);
        self.asm.bind(end);
        self.loop_head_stack.pop();
        self.loop_end_stack.pop();
        Ok(())
    }

    fn emit_for(
        &mut self,
        init: Option<&Stmt>,
        cond: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
    ) -> Result<()> {
        let head = self.asm.new_label();
        let end = self.asm.new_label();
        if let Some(i) = init {
            self.emit_stmt(i)?;
        }
        self.loop_head_stack.push(head);
        self.loop_end_stack.push(end);
        self.asm.bind(head);
        self.eval_expr(cond)?;
        self.asm.test_ax_ax();
        self.asm.je_short_lab(end);
        for s in body {
            self.emit_stmt(s)?;
        }
        if let Some(st) = step {
            self.eval_expr(st)?;
        }
        self.asm.jmp_short_lab(head);
        self.asm.bind(end);
        self.loop_head_stack.pop();
        self.loop_end_stack.pop();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    fn eval_expr(&mut self, e: &Expr) -> Result<()> {
        match &e.kind {
            ExprKind::Lit(lit) => match lit {
                Literal::Int(v) => {
                    if *v < i16::MIN as i64 || *v > u16::MAX as i64 {
                        bail!("16-bit constant {} out of range", v);
                    }
                    self.asm.mov16_imm(Reg16::Ax, *v as u16);
                }
                Literal::Bool(b) => self.asm.mov16_imm(Reg16::Ax, if *b { 1 } else { 0 }),
                Literal::Char(c) => self.asm.mov16_imm(Reg16::Ax, *c as u16),
                Literal::String(s) => {
                    let lab = self.string_label(s);
                    self.asm.mov16_imm_label(Reg16::Ax, lab);
                }
                Literal::Float(_) => bail!("floating point is not supported by the 16-bit backend"),
                Literal::Null => self.asm.mov16_imm(Reg16::Ax, 0),
            },
            ExprKind::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown variable: {}", name))?;
                self.load_slot(*slot);
            }
            ExprKind::Bin { op, left, right } => self.eval_bin(*op, left, right, &e.ty)?,
            ExprKind::Call { func, args } => self.eval_call(func, args)?,
            ExprKind::Cast { expr, ty } => {
                self.eval_expr(expr)?;
                self.eval_cast(&expr.ty, ty)?;
            }
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?;
                let off = self.field_offset(&base.ty, *field)? as i16;
                if off != 0 {
                    self.asm.add_ax_imm(off)?;
                }
            }
            ExprKind::Load(ptr) => {
                self.eval_expr(ptr)?;
                self.asm.mov16_rr(Reg16::Si, Reg16::Ax);
                let (width, signed) = type_info(&e.ty)?;
                match width {
                    1 => {
                        self.asm.load8_si(Reg8::Al);
                        if signed {
                            self.asm.cbw();
                        } else {
                            self.asm.xor_ah_ah();
                        }
                    }
                    2 => self.asm.load16_si(Reg16::Ax),
                    _ => bail!("unsupported load width: {}", width),
                }
            }
            ExprKind::AddrOf(inner) => self.expr_addr(inner)?,
            ExprKind::Block(stmts, trailing) => {
                for st in stmts {
                    self.emit_stmt(st)?;
                }
                self.eval_expr(trailing)?;
            }
            ExprKind::Asm { .. } => bail!("inline assembly is not supported by the 16-bit backend"),
        }
        Ok(())
    }

    fn eval_bin(&mut self, op: BinOp, left: &Expr, right: &Expr, _result_ty: &Type) -> Result<()> {
        if op.is_logical() {
            return self.eval_logical(op, left, right);
        }

        self.eval_expr(left)?;
        self.asm.push(Reg16::Ax);
        self.eval_expr(right)?;
        self.asm.pop(Reg16::Bx);
        // Left operand is now in BX, right operand in AX.

        if op.is_arithmetic() {
            match op {
                BinOp::Add => {
                    self.asm.add_rr(Reg16::Ax, Reg16::Bx);
                }
                BinOp::Sub => {
                    self.asm.sub_rr(Reg16::Bx, Reg16::Ax);
                    self.asm.mov16_rr(Reg16::Ax, Reg16::Bx);
                }
                BinOp::Mul => {
                    self.asm.imul_r16(Reg16::Bx);
                }
                BinOp::Div | BinOp::Mod => {
                    if left.ty.is_signed() {
                        self.asm.cwd();
                        self.asm.idiv_r16(Reg16::Bx);
                    } else {
                        self.asm.xor_dx_dx();
                        self.asm.div_r16(Reg16::Bx);
                    }
                    if op == BinOp::Mod {
                        self.asm.mov16_rr(Reg16::Ax, Reg16::Dx);
                    }
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        // Comparisons.
        self.asm.cmp_rr(Reg16::Bx, Reg16::Ax);
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg8::Al);
        self.asm.xor_ah_ah();
        Ok(())
    }

    fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        let shortcut = self.asm.new_label();
        let end = self.asm.new_label();
        self.eval_expr(left)?;
        self.asm.test_ax_ax();
        match op {
            BinOp::And => self.asm.je_short_lab(shortcut),
            BinOp::Or => self.asm.jne_short_lab(shortcut),
            _ => unreachable!(),
        }
        self.eval_expr(right)?;
        self.asm.test_ax_ax();
        self.asm.mov16_imm(Reg16::Ax, 0);
        self.asm.setcc(Cond::Ne, Reg8::Al);
        self.asm.jmp_short_lab(end);
        self.asm.bind(shortcut);
        match op {
            BinOp::And => self.asm.mov16_imm(Reg16::Ax, 0),
            BinOp::Or => self.asm.mov16_imm(Reg16::Ax, 1),
            _ => unreachable!(),
        }
        self.asm.bind(end);
        Ok(())
    }

    fn eval_call(&mut self, func: &str, args: &[Expr]) -> Result<()> {
        let lab = *self
            .func_labels
            .get(func)
            .ok_or_else(|| anyhow!("unknown function: {}", func))?;
        for a in args {
            self.eval_expr(a)?;
            self.asm.push(Reg16::Ax);
        }
        self.asm.call_near_lab(lab);
        if !args.is_empty() {
            self.asm.add_sp_imm((args.len() * 2) as i16)?;
        }
        Ok(())
    }

    fn eval_cast(&mut self, from: &Type, to: &Type) -> Result<()> {
        let _ = (from, to);
        // In 16-bit real mode all supported values already live in the low bits
        // of AX; no extra code is required for integer/pointer casts.
        Ok(())
    }

    fn expr_addr(&mut self, e: &Expr) -> Result<()> {
        match &e.kind {
            ExprKind::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown variable: {}", name))?;
                self.asm.lea_bp(Reg16::Ax, slot.offset);
            }
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?;
                let off = self.field_offset(&base.ty, *field)? as i16;
                if off != 0 {
                    self.asm.add_ax_imm(off)?;
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
                    .ok_or_else(|| anyhow!("unknown variable: {}", name))?;
                self.asm.lea_bp(Reg16::Ax, slot.offset);
            }
            LValue::Deref(ptr) => {
                self.eval_expr(ptr)?;
            }
            LValue::Field { base, field } => {
                self.eval_expr(base)?;
                let off = self.field_offset(&base.ty, *field)? as i16;
                if off != 0 {
                    self.asm.add_ax_imm(off)?;
                }
            }
        }
        Ok(())
    }

    fn lvalue_width(&self, lv: &LValue) -> Result<(u8, bool)> {
        match lv {
            LValue::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown variable: {}", name))?;
                Ok((slot.width, slot.signed))
            }
            LValue::Deref(ptr) => type_info(&ptr.ty).and_then(|(w, s)| {
                if w == 0 {
                    bail!("dereference of void pointer")
                }
                Ok((w, s))
            }),
            LValue::Field { base, field } => {
                let name = match &base.ty {
                    Type::Ptr(inner) => match inner.as_ref() {
                        Type::Struct(n) => n.clone(),
                        _ => bail!("field access on non-struct pointer"),
                    },
                    Type::Struct(n) => n.clone(),
                    _ => bail!("field access on non-struct type"),
                };
                let def = self
                    .prog
                    .structs
                    .iter()
                    .find(|s| s.name == name)
                    .ok_or_else(|| anyhow!("unknown struct: {}", name))?;
                let ty = &def.fields.get(*field).ok_or_else(|| anyhow!("field index out of range"))?.1;
                type_info(ty)
            }
        }
    }

    fn field_offset(&self, ptr_ty: &Type, field: usize) -> Result<u16> {
        let name = match ptr_ty {
            Type::Ptr(inner) => match inner.as_ref() {
                Type::Struct(n) => n.clone(),
                _ => bail!("field access on non-struct pointer"),
            },
            Type::Struct(n) => n.clone(),
            _ => bail!("field access on non-struct type"),
        };
        let def = self
            .prog
            .structs
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("unknown struct: {}", name))?;
        let layout = layout_struct(def);
        layout
            .offsets
            .get(field)
            .copied()
            .ok_or_else(|| anyhow!("field index out of range"))
    }

    fn load_slot(&mut self, slot: Slot16) {
        match slot.width {
            1 => {
                self.asm.load8_bp(Reg8::Al, slot.offset);
                if slot.signed {
                    self.asm.cbw();
                } else {
                    self.asm.xor_ah_ah();
                }
            }
            2 => self.asm.load16_bp(Reg16::Ax, slot.offset),
            _ => unreachable!(),
        }
    }

    fn store_slot(&mut self, slot: Slot16, _src16: Reg16, src8: Reg8) {
        match slot.width {
            1 => self.asm.store8_bp(slot.offset, src8),
            2 => self.asm.store16_bp(slot.offset, Reg16::Ax),
            _ => unreachable!(),
        }
    }

    fn string_label(&mut self, s: &str) -> u32 {
        if let Some(&lab) = self.string_labels.get(s) {
            return lab;
        }
        let lab = self.asm.new_label();
        self.string_labels.insert(s.to_string(), lab);
        lab
    }
}

// -----------------------------------------------------------------------------
// Type helpers
// -----------------------------------------------------------------------------

fn type_info(ty: &Type) -> Result<(u8, bool)> {
    match ty {
        Type::I8 => Ok((1, true)),
        Type::U8 | Type::Char | Type::Bool => Ok((1, false)),
        Type::I16 => Ok((2, true)),
        Type::U16 | Type::Ptr(_) => Ok((2, false)),
        _ => bail!("type {:?} is not supported by the 16-bit backend", ty),
    }
}

fn align_up_u8(value: u8, align: u8) -> u8 {
    if align == 0 || value % align == 0 {
        value
    } else {
        value + align - value % align
    }
}

fn layout_struct(s: &StructDef) -> StructLayout {
    let mut offset = 0u16;
    let mut offsets = Vec::with_capacity(s.fields.len());
    for (_, ty) in &s.fields {
        let size = match type_info(ty) {
            Ok((w, _)) => w as u16,
            Err(_) => 2, // unsupported fields default to 16-bit
        };
        offset = align_up_u16(offset, size.max(1) as u16);
        offsets.push(offset);
        offset += size;
    }
    let align = s
        .fields
        .iter()
        .map(|(_, ty)| match type_info(ty) {
            Ok((w, _)) => w.max(1),
            Err(_) => 1,
        })
        .max()
        .unwrap_or(1);
    StructLayout {
        size: align_up_u16(offset, align as u16),
        align: align as u16,
        offsets,
    }
}

fn align_up_u16(value: u16, align: u16) -> u16 {
    if align == 0 {
        return value;
    }
    ((value + align - 1) / align) * align
}

struct StructLayout {
    #[allow(dead_code)]
    size: u16,
    #[allow(dead_code)]
    align: u16,
    offsets: Vec<u16>,
}

fn cond_for_cmp(op: BinOp, ty: &Type) -> Result<Cond> {
    let signed = ty.is_signed();
    Ok(match op {
        BinOp::Eq => Cond::E,
        BinOp::Ne => Cond::Ne,
        BinOp::Lt if signed => Cond::L,
        BinOp::Le if signed => Cond::Le,
        BinOp::Gt if signed => Cond::G,
        BinOp::Ge if signed => Cond::Ge,
        BinOp::Lt => Cond::B,
        BinOp::Le => Cond::Be,
        BinOp::Gt => Cond::A,
        BinOp::Ge => Cond::Ae,
        _ => bail!("unsupported comparison operator"),
    })
}

// -----------------------------------------------------------------------------
// Low-level encoder
// -----------------------------------------------------------------------------

struct Encoder {
    bytes: Vec<u8>,
    labels: HashMap<u32, usize>,
    short_fixups: Vec<(u32, usize)>,
    rel16_fixups: Vec<(u32, usize)>,
    imm16_fixups: Vec<(u32, usize)>,
    next_label: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Reg16 {
    Ax = 0,
    Cx = 1,
    Dx = 2,
    Bx = 3,
    Sp = 4,
    Bp = 5,
    Si = 6,
    Di = 7,
}

impl Reg16 {
    /// 16-bit memory-addressing r/m code for indirect operands.
    /// This differs from the register-field encoding: BP is encoded as 110
    /// and SI as 100 when used as a base/index in a ModR/M memory operand.
    fn rm16(self) -> u8 {
        match self {
            Reg16::Si => 4,
            Reg16::Di => 5,
            Reg16::Bp => 6,
            Reg16::Bx => 7,
            _ => self as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Reg8 {
    Al = 0,
    Cl = 1,
    Dl = 2,
    Bl = 3,
    Ah = 4,
    Ch = 5,
    Dh = 6,
    Bh = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum SegReg {
    Es = 0,
    Cs = 1,
    Ss = 2,
    Ds = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum Cond {
    E = 0x4,
    Ne = 0x5,
    B = 0x2,
    Be = 0x6,
    A = 0x7,
    Ae = 0x3,
    L = 0xC,
    Le = 0xE,
    G = 0xF,
    Ge = 0xD,
}

impl Encoder {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            labels: HashMap::new(),
            short_fixups: Vec::new(),
            rel16_fixups: Vec::new(),
            imm16_fixups: Vec::new(),
            next_label: 1,
        }
    }

    fn new_label(&mut self) -> u32 {
        let lab = self.next_label;
        self.next_label += 1;
        lab
    }

    fn bind(&mut self, lab: u32) {
        self.labels.insert(lab, self.bytes.len());
    }

    fn into_bytes(self) -> Result<Vec<u8>> {
        let mut bytes = self.bytes;
        for (lab, off) in self.short_fixups {
            let target = *self
                .labels
                .get(&lab)
                .ok_or_else(|| anyhow!("undefined short jump label {}", lab))?;
            let pc = off + 1;
            let rel = target as i64 - pc as i64;
            if rel < -128 || rel > 127 {
                bail!(
                    "16-bit backend: short jump to label {} is out of range ({} bytes)",
                    lab,
                    rel
                );
            }
            bytes[off] = rel as u8;
        }
        for (lab, off) in self.rel16_fixups {
            let target = *self
                .labels
                .get(&lab)
                .ok_or_else(|| anyhow!("undefined rel16 label {}", lab))?;
            let pc = off + 2;
            let rel = (target as i64 - pc as i64) as i16;
            bytes[off..off + 2].copy_from_slice(&rel.to_le_bytes());
        }
        for (lab, off) in self.imm16_fixups {
            let target = *self
                .labels
                .get(&lab)
                .ok_or_else(|| anyhow!("undefined imm16 label {}", lab))?;
            let addr = (0x7C00u32 + target as u32) as u16;
            bytes[off..off + 2].copy_from_slice(&addr.to_le_bytes());
        }
        Ok(bytes)
    }

    fn emit(&mut self, b: u8) {
        self.bytes.push(b);
    }

    fn emit_slice(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn emit_imm8(&mut self, v: i8) {
        self.bytes.push(v as u8);
    }

    fn emit_imm16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    fn modrm(&mut self, mode: u8, reg: u8, rm: u8) {
        self.emit(((mode & 3) << 6) | ((reg & 7) << 3) | (rm & 7));
    }

    fn sib8(&mut self, v: i8) {
        self.emit(v as u8);
    }

    // Registers
    fn push(&mut self, r: Reg16) {
        self.emit(0x50 + r as u8);
    }

    fn pop(&mut self, r: Reg16) {
        self.emit(0x58 + r as u8);
    }

    fn mov16_rr(&mut self, dst: Reg16, src: Reg16) {
        // mov r/m16, r16
        self.emit(0x89);
        self.modrm(3, src as u8, dst as u8);
    }

    fn mov16_rm(&mut self, dst: Reg16, src: Reg16) {
        // mov r16, r/m16
        self.emit(0x8B);
        self.modrm(3, dst as u8, src as u8);
    }

    fn mov16_imm(&mut self, r: Reg16, imm: u16) {
        self.emit(0xB8 + r as u8);
        self.emit_imm16(imm);
    }

    fn mov16_imm_label(&mut self, r: Reg16, lab: u32) {
        self.emit(0xB8 + r as u8);
        let off = self.bytes.len();
        self.emit_imm16(0);
        self.imm16_fixups.push((lab, off));
    }

    fn mov8_imm(&mut self, r: Reg8, imm: u8) {
        self.emit(0xB0 + r as u8);
        self.emit(imm);
    }

    fn mov_sp_imm(&mut self, imm: u16) {
        self.emit(0xBC);
        self.emit_imm16(imm);
    }

    fn mov_seg_ax(&mut self, seg: SegReg) {
        match seg {
            SegReg::Ds => self.emit_slice(&[0x8E, 0xD8]),
            SegReg::Es => self.emit_slice(&[0x8E, 0xC0]),
            SegReg::Ss => self.emit_slice(&[0x8E, 0xD0]),
            SegReg::Cs => unreachable!(),
        }
    }

    // BP-relative memory
    fn lea_bp(&mut self, dst: Reg16, off: i8) {
        self.emit(0x8D);
        self.modrm(1, dst as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    fn load16_bp(&mut self, dst: Reg16, off: i8) {
        self.emit(0x8B);
        self.modrm(1, dst as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    fn store16_bp(&mut self, off: i8, src: Reg16) {
        self.emit(0x89);
        self.modrm(1, src as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    fn load8_bp(&mut self, dst: Reg8, off: i8) {
        self.emit(0x8A);
        self.modrm(1, dst as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    fn store8_bp(&mut self, off: i8, src: Reg8) {
        self.emit(0x88);
        self.modrm(1, src as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    // SI-indirect memory
    fn load16_si(&mut self, dst: Reg16) {
        self.emit(0x8B);
        self.modrm(0, dst as u8, Reg16::Si.rm16());
    }

    fn store16_si(&mut self, src: Reg16) {
        self.emit(0x89);
        self.modrm(0, src as u8, Reg16::Si.rm16());
    }

    fn load8_si(&mut self, dst: Reg8) {
        self.emit(0x8A);
        self.modrm(0, dst as u8, Reg16::Si.rm16());
    }

    fn store8_si(&mut self, src: Reg8) {
        self.emit(0x88);
        self.modrm(0, src as u8, Reg16::Si.rm16());
    }

    // Arithmetic
    fn add_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x01);
        self.modrm(3, src as u8, dst as u8);
    }

    fn sub_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x29);
        self.modrm(3, src as u8, dst as u8);
    }

    fn cmp_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x39);
        self.modrm(3, src as u8, dst as u8);
    }

    fn add_ax_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xC0]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit(0x05);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    fn sub_ax_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xE8]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit(0x2D);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    fn add_sp_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xC4]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit_slice(&[0x81, 0xC4]);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    fn sub_sp_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xEC]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit_slice(&[0x81, 0xEC]);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    fn xor_ax_ax(&mut self) {
        self.emit_slice(&[0x31, 0xC0]);
    }

    fn xor_ah_ah(&mut self) {
        self.emit_slice(&[0x30, 0xE4]);
    }

    fn cbw(&mut self) {
        self.emit(0x98);
    }

    fn cwd(&mut self) {
        self.emit(0x99);
    }

    fn inc(&mut self, r: Reg16) {
        self.emit(0x40 + r as u8);
    }

    fn dec(&mut self, r: Reg16) {
        self.emit(0x48 + r as u8);
    }

    fn test_ax_ax(&mut self) {
        self.emit_slice(&[0x85, 0xC0]);
    }

    fn shl_ax_imm(&mut self, imm: u8) {
        self.emit_slice(&[0xC1, 0xE0]);
        self.emit(imm);
    }

    fn shr_ax_imm(&mut self, imm: u8) {
        self.emit_slice(&[0xC1, 0xE8]);
        self.emit(imm);
    }

    fn imul_r16(&mut self, r: Reg16) {
        self.emit(0xF7);
        self.modrm(3, 5, r as u8);
    }

    fn idiv_r16(&mut self, r: Reg16) {
        self.emit(0xF7);
        self.modrm(3, 7, r as u8);
    }

    fn div_r16(&mut self, r: Reg16) {
        self.emit(0xF7);
        self.modrm(3, 6, r as u8);
    }

    fn xor_dx_dx(&mut self) {
        self.emit_slice(&[0x31, 0xD2]);
    }

    fn setcc(&mut self, cond: Cond, r: Reg8) {
        self.emit(0x0F);
        self.emit(0x90 + cond as u8);
        self.modrm(3, 0, r as u8);
    }

    // Control flow
    fn jmp_short_lab(&mut self, lab: u32) {
        self.emit(0xEB);
        let off = self.bytes.len();
        self.emit(0);
        self.short_fixups.push((lab, off));
    }

    fn je_short_lab(&mut self, lab: u32) {
        self.jcc_short_lab(0x74, lab);
    }

    fn jne_short_lab(&mut self, lab: u32) {
        self.jcc_short_lab(0x75, lab);
    }

    fn jcc_short_lab(&mut self, opcode: u8, lab: u32) {
        self.emit(opcode);
        let off = self.bytes.len();
        self.emit(0);
        self.short_fixups.push((lab, off));
    }

    fn call_near_lab(&mut self, lab: u32) {
        self.emit(0xE8);
        let off = self.bytes.len();
        self.emit_imm16(0);
        self.rel16_fixups.push((lab, off));
    }

    fn ret(&mut self) {
        self.emit(0xC3);
    }

    fn cli(&mut self) {
        self.emit(0xFA);
    }

    fn hlt(&mut self) {
        self.emit(0xF4);
    }

    fn int(&mut self, imm: u8) {
        self.emit_slice(&[0xCD, imm]);
    }

    fn out_imm8_al(&mut self, port: u8) {
        self.emit_slice(&[0xE6, port]);
    }

    fn out_dx_al(&mut self) {
        self.emit(0xEE);
    }

    fn db(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn db_str(&mut self, s: &str) {
        self.bytes.extend_from_slice(s.as_bytes());
        self.emit(0);
    }
}

