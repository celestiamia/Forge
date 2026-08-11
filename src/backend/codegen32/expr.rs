use super::*;

impl<'p> CodeGen<'p> {
    pub(super) fn eval_expr(&mut self, e: &Expr) -> Result<()> {
        match &e.kind {
            ExprKind::Lit(lit) => match lit {
                Literal::Int(v) => self.asm.mov(Reg::Eax, *v as i32)?,
                Literal::Bool(v) => self.asm.mov(Reg::Eax, if *v { 1i32 } else { 0i32 })?,
                Literal::Char(v) => self.asm.mov(Reg::Eax, *v as i32)?,
                Literal::String(s) => {
                    let lab = self.string_label(s);
                    let patch_off = self.asm.len() + 2; // C7 /0 imm32, offset of imm32
                    self.asm.mov(Reg::Eax, 0i32)?;
                    self.string_patches.push((patch_off, lab));
                }
                Literal::Float(_) => bail!("floating point is not implemented in the x86_32 backend"),
                Literal::Null => self.asm.mov(Reg::Eax, 0i32)?,
            },
            ExprKind::Var(name) => {
                if let Some(&lab) = self.global_labels.get(name) {
                    let patch_off = self.asm.len() + 2; // C7 /0 imm32
                    self.asm.mov(Reg::Eax, 0i32)?;
                    self.string_patches.push((patch_off, lab));
                    match &e.ty {
                        Type::I8 | Type::U8 | Type::Bool | Type::Char => {
                            self.asm.movsx8(Reg::Eax, Mem::base(Reg::Eax))?;
                        }
                        Type::I16 | Type::U16 => {
                            self.asm.movsx16(Reg::Eax, Mem::base(Reg::Eax))?;
                        }
                        _ => {
                            self.asm.mov(Reg::Eax, Mem::base(Reg::Eax))?;
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
                            .lea(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset))?;
                        self.load_from_addr(&e.ty)?;
                    }
                    _ => {
                        self.asm
                            .mov(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset))?;
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
                    self.asm.add(Reg::Eax, off as i32)?;
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
            ExprKind::SizeOf(ty) => {
                let size = self.type_size_bytes(ty);
                self.asm.mov(Reg::Eax, size as i32)?;
            }
            ExprKind::OffsetOf { ty, field } => {
                let off = match ty {
                    Type::Struct(name) => self.field_offset(name, *field)?,
                    _ => bail!("offsetof on non-struct type"),
                };
                self.asm.mov(Reg::Eax, off as i32)?;
            }
        }
        Ok(())
    }

    pub(super) fn eval_bin(&mut self, op: BinOp, left: &Expr, right: &Expr, _ty: &Type) -> Result<()> {
        if op.is_logical() {
            return self.eval_logical(op, left, right);
        }

        self.eval_expr(left)?;
        self.asm.mov(Reg::Ecx, Reg::Eax)?; // left -> Ecx
        self.asm.push(Reg::Ecx)?;          // preserve left across right evaluation
        self.eval_expr(right)?;           // right -> Eax
        self.asm.pop(Reg::Ecx)?;           // restore left

        if op.is_arithmetic() {
            match op {
                BinOp::Add => {
                    // C-style pointer arithmetic: when one operand is a
                    // pointer and the other an integer, scale the integer by
                    // the pointee size before adding.  All scalar sizes are
                    // powers of two, so the scaling is a shift.
                    if let (Some(elem), true) = (ptr_elem_size(&left.ty), right.ty.is_integer()) {
                        if elem > 1 {
                            self.asm.shl(Reg::Eax, elem.trailing_zeros() as i8)?;
                        }
                        self.asm.add(Reg::Eax, Reg::Ecx)?;
                    } else if let (Some(elem), true) = (ptr_elem_size(&right.ty), left.ty.is_integer()) {
                        if elem > 1 {
                            self.asm.shl(Reg::Ecx, elem.trailing_zeros() as i8)?;
                        }
                        self.asm.add(Reg::Eax, Reg::Ecx)?;
                    } else {
                        self.asm.add(Reg::Eax, Reg::Ecx)?;
                    }
                }
                BinOp::Sub => {
                    self.asm.mov(Reg::Edx, Reg::Eax)?; // right -> Edx
                    if let (Some(elem), true) = (ptr_elem_size(&left.ty), right.ty.is_integer()) {
                        if elem > 1 {
                            self.asm.shl(Reg::Edx, elem.trailing_zeros() as i8)?;
                        }
                    }
                    self.asm.mov(Reg::Eax, Reg::Ecx)?; // left -> Eax
                    self.asm.sub(Reg::Eax, Reg::Edx)?;
                }
                BinOp::Mul => {
                    self.asm.imul(Reg::Eax, Reg::Ecx)?; // Eax = right * left
                }
                BinOp::Div | BinOp::Mod => {
                    if left.ty.is_signed() {
                        self.asm.push(Reg::Eax)?;          // save divisor (right)
                        self.asm.mov(Reg::Eax, Reg::Ecx)?; // dividend (left)
                        self.asm.cdq()?;
                        self.asm.pop(Reg::Ecx)?; // divisor
                        self.asm.idiv(Reg::Ecx)?;
                    } else {
                        self.asm.push(Reg::Eax)?;
                        self.asm.mov(Reg::Eax, Reg::Ecx)?;
                        self.asm.xor(Reg::Edx, Reg::Edx)?;
                        self.asm.pop(Reg::Ecx)?;
                        self.asm.div(Reg::Ecx)?;
                    }
                    if op == BinOp::Mod {
                        self.asm.mov(Reg::Eax, Reg::Edx)?; // remainder
                    }
                }
                BinOp::FloorDiv => {
                    // Floor division: floor(a/b)
                    self.asm.push(Reg::Eax)?;          // save divisor
                    self.asm.mov(Reg::Eax, Reg::Ecx)?; // dividend
                    if left.ty.is_signed() {
                        self.asm.cdq()?;
                        self.asm.pop(Reg::Ecx)?; // divisor
                        self.asm.idiv(Reg::Ecx)?;
                        // Adjust: if remainder != 0 and quotient < 0, quotient -= 1
                        self.asm.push(Reg::Eax)?; // save quotient
                        self.asm.test(Reg::Edx, Reg::Edx)?;
                        let skip = self.asm.new_label();
                        self.asm.je(skip)?;
                        self.asm.pop(Reg::Eax)?;
                        self.asm.test(Reg::Eax, Reg::Eax)?;
                        self.asm.jcc(Cond::Ge, skip)?;
                        self.asm.dec(Reg::Eax)?;
                        self.bind_label(skip);
                    } else {
                        self.asm.xor(Reg::Edx, Reg::Edx)?;
                        self.asm.pop(Reg::Ecx)?;
                        self.asm.div(Reg::Ecx)?;
                    }
                }
                _ => bail!("unhandled binary op {:?}", op),
            }
            return Ok(());
        }

        match op {
            BinOp::BitAnd => {
                self.asm.and(Reg::Eax, Reg::Ecx)?;
                return Ok(());
            }
            BinOp::BitOr => {
                self.asm.or(Reg::Eax, Reg::Ecx)?;
                return Ok(());
            }
            BinOp::BitXor => {
                self.asm.xor(Reg::Eax, Reg::Ecx)?;
                return Ok(());
            }
            BinOp::Shl => {
                self.asm.mov(Reg::Edx, Reg::Ecx)?; // value
                self.asm.mov(Reg::Ecx, Reg::Eax)?; // count -> cl
                self.asm.mov(Reg::Eax, Reg::Edx)?; // value -> Eax
                self.asm.shl_cl(Reg::Eax)?;
                return Ok(());
            }
            BinOp::Shr => {
                self.asm.mov(Reg::Edx, Reg::Ecx)?; // value
                self.asm.mov(Reg::Ecx, Reg::Eax)?; // count -> cl
                self.asm.mov(Reg::Eax, Reg::Edx)?; // value -> Eax
                self.asm.sar_cl(Reg::Eax)?;
                return Ok(());
            }
            _ => {}
        }

        self.asm.cmp(Reg::Ecx, Reg::Eax)?;
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg::Eax.r8())?;
        self.asm.movzx8(Reg::Eax, Reg::Eax)?;
        Ok(())
    }

    pub(super) fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        let short = self.asm.new_label();
        let end = self.asm.new_label();
        self.eval_expr(left)?;
        self.asm.test(Reg::Eax, Reg::Eax)?;
        match op {
            BinOp::And => self.asm.je(short)?,
            BinOp::Or => self.asm.jne(short)?,
            _ => bail!("unhandled binary op {:?}", op),
        }
        self.eval_expr(right)?;
        self.asm.jmp(end)?;
        self.bind_label(short);
        match op {
            BinOp::And => self.asm.mov(Reg::Eax, 0i32)?,
            BinOp::Or => self.asm.mov(Reg::Eax, 1i32)?,
            _ => bail!("unhandled binary op {:?}", op),
        }
        self.bind_label(end);
        Ok(())
    }

    pub(super) fn eval_call(&mut self, func: &str, args: &[Expr]) -> Result<()> {
        let target = *self
            .func_labels
            .get(func)
            .ok_or_else(|| anyhow::anyhow!("unknown function: {}", func))?;

        let mut arg_slots = Vec::new();
        for a in args {
            self.eval_expr(a)?;
            let slot = self.alloc_slot(4, 4);
            self.store_scalar(slot.offset)?;
            arg_slots.push(slot);
        }

        for slot in arg_slots.iter().rev() {
            self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset))?;
            self.asm.push(Reg::Eax)?;
        }

        self.asm.call(target)?;

        if !arg_slots.is_empty() {
            self.asm.add(Reg::Esp, (arg_slots.len() * 4) as i32)?;
        }
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
            (Type::I8, Type::I32) => {
                self.asm.movsx8(Reg::Eax, Reg::Eax)?;
            }
            (Type::I16, Type::I32) => {
                self.asm.movsx16(Reg::Eax, Reg::Eax)?;
            }
            (Type::U8 | Type::Char | Type::Bool, Type::U32) => {
                self.asm.movzx8(Reg::Eax, Reg::Eax)?;
            }
            (Type::U16, Type::U32) => {
                self.asm.movzx16(Reg::Eax, Reg::Eax)?;
            }
            (_, Type::I32 | Type::U32) => {}
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
                self.asm.lea(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset))?;
            }
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?;
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Eax, off as i32)?;
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
                self.asm.lea(Reg::Eax, Mem::base_disp(Reg::Ebp, slot.offset))?;
            }
            LValue::Deref(ptr) => {
                self.eval_expr(ptr)?; // pointer value is already the address
            }
            LValue::Field { base, field } => {
                self.eval_expr(base)?; // pointer to struct
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Eax, off as i32)?;
                }
            }
        }
        Ok(())
    }

}
