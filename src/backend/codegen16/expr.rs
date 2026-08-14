use super::*;

impl<'p> CodeGen16<'p> {
    pub(super) fn eval_expr(&mut self, e: &Expr) -> Result<()> {
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
                Literal::Bytes(_) => bail!("embedded data is not supported by the 16-bit backend"),
                Literal::Null => self.asm.mov16_imm(Reg16::Ax, 0),
            },
            ExprKind::Var(name) => {
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown variable: {}", name))?;
                self.load_slot(*slot)?;
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
            ExprKind::SizeOf(ty) => {
                let size = type_size_16(ty);
                self.asm.mov16_imm(Reg16::Ax, size as u16);
            }
            ExprKind::OffsetOf { ty, field } => {
                let off = match ty {
                    Type::Struct(name) => self.field_offset(ty, *field)? as u16,
                    _ => bail!("offsetof on non-struct type"),
                };
                self.asm.mov16_imm(Reg16::Ax, off);
            }
        }
        Ok(())
    }

    pub(super) fn eval_bin(&mut self, op: BinOp, left: &Expr, right: &Expr, _result_ty: &Type) -> Result<()> {
        if op.is_logical() {
            return self.eval_logical(op, left, right);
        }

        self.eval_expr(left)?;
        self.asm.push(Reg16::Ax);
        self.eval_expr(right)?;
        self.asm.pop(Reg16::Bx);

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
                _ => bail!("unhandled binary op {:?}", op),
            }
            return Ok(());
        }

        self.asm.cmp_rr(Reg16::Bx, Reg16::Ax);
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg8::Al);
        self.asm.xor_ah_ah();
        Ok(())
    }

    pub(super) fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        let shortcut = self.asm.new_label();
        let end = self.asm.new_label();
        self.eval_expr(left)?;
        self.asm.test_ax_ax();
        match op {
            BinOp::And => self.asm.je_short_lab(shortcut),
            BinOp::Or => self.asm.jne_short_lab(shortcut),
            _ => bail!("unhandled binary op {:?}", op),
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
            _ => bail!("unhandled binary op {:?}", op),
        }
        self.asm.bind(end);
        Ok(())
    }

    pub(super) fn eval_call(&mut self, func: &str, args: &[Expr]) -> Result<()> {
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

    pub(super) fn eval_cast(&mut self, from: &Type, to: &Type) -> Result<()> {
        let _ = (from, to);
        Ok(())
    }

    pub(super) fn expr_addr(&mut self, e: &Expr) -> Result<()> {
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

    pub(super) fn lvalue_addr(&mut self, lv: &LValue) -> Result<()> {
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

    pub(super) fn lvalue_width(&self, lv: &LValue) -> Result<(u8, bool)> {
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

}
