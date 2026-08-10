use super::*;

impl<'p> CodeGen<'p> {
    pub(super) fn eval_expr(&mut self, e: &Expr) -> Result<()> {
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
                if let Some(&lab) = self.global_labels.get(name) {
                    self.asm.lea_rip(Reg::Rax, lab);
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

    pub(super) fn eval_bin(&mut self, op: BinOp, left: &Expr, right: &Expr, _ty: &Type) -> Result<()> {
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
                BinOp::Add => {
                    // C-style pointer arithmetic: when one operand is a
                    // pointer and the other an integer, scale the integer by
                    // the pointee size before adding.  All scalar sizes are
                    // powers of two, so the scaling is a shift.
                    if let (Some(elem), true) = (ptr_elem_size(&left.ty), right.ty.is_integer()) {
                        if elem > 1 {
                            self.asm.shl(Reg::Rax, elem.trailing_zeros() as i8);
                        }
                        self.asm.add(Reg::Rax, Reg::R10);
                    } else if let (Some(elem), true) = (ptr_elem_size(&right.ty), left.ty.is_integer()) {
                        if elem > 1 {
                            self.asm.mov(Reg::R11, Reg::R10);
                            self.asm.shl(Reg::R11, elem.trailing_zeros() as i8);
                            self.asm.add(Reg::Rax, Reg::R11);
                        } else {
                            self.asm.add(Reg::Rax, Reg::R10);
                        }
                    } else {
                        self.asm.add(Reg::Rax, Reg::R10);
                    }
                }
                BinOp::Sub => {
                    self.asm.mov(Reg::Rdx, Reg::Rax); // right
                    if let (Some(elem), true) = (ptr_elem_size(&left.ty), right.ty.is_integer()) {
                        if elem > 1 {
                            self.asm.shl(Reg::Rdx, elem.trailing_zeros() as i8);
                        }
                    }
                    self.asm.mov(Reg::Rax, Reg::R10); // left
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

        self.asm.cmp(Reg::R10, Reg::Rax);
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg::Rax.r8());
        self.asm.movzx8(Reg::Rax, Reg::Rax);
        Ok(())
    }

    pub(super) fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
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

    pub(super) fn eval_call(&mut self, func: &str, args: &[Expr]) -> Result<()> {
        if args.len() > 6 {
            bail!("functions with more than 6 arguments are not supported");
        }
        let target = *self
            .func_labels
            .get(func)
            .ok_or_else(|| anyhow::anyhow!("unknown function: {}", func))?;

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

    pub(super) fn eval_cast(&mut self, expr: &Expr, to: &Type) -> Result<()> {
        self.eval_expr(expr)?;
        if &expr.ty == to {
            return Ok(());
        }
        match (expr.ty.clone(), to.clone()) {
            (Type::Ptr(_), _) if to.is_integer() => {}
            (_, Type::Ptr(_)) if expr.ty.is_integer() => {}
            (Type::I8, Type::I64) | (Type::I8, Type::I32) => {
                self.asm.movsx8(Reg::Rax, Reg::Rax);
            }
            (Type::I16, Type::I64) | (Type::I16, Type::I32) => {
                self.asm.movsx16(Reg::Rax, Reg::Rax);
            }
            (Type::I32, Type::I64) => {
                self.asm.movsxd(Reg::Rax, Reg::Rax);
            }
            (Type::U8 | Type::Char | Type::Bool, _) if to.is_integer() => {
                self.asm.movzx8(Reg::Rax, Reg::Rax);
            }
            (Type::U16, _) if to.is_integer() => {
                self.asm.movzx8(Reg::Rax, Reg::Rax); // movzx16 missing; use 8 as placeholder
            }
            (_, Type::I64 | Type::U64) => {}
            (_, _) => {}
        }
        Ok(())
    }

    pub(super) fn expr_addr(&mut self, e: &Expr) -> Result<()> {
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

    pub(super) fn lvalue_addr(&mut self, lv: &LValue) -> Result<()> {
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

}
