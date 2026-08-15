use super::*;

impl<'p> CodeGen<'p> {
    pub(super) fn eval_expr(&mut self, e: &Expr) -> Result<()> {
        match &e.kind {
            ExprKind::Lit(lit) => match lit {
                Literal::Int(v) => self.asm.mov(Reg::Rax, *v)?,
                Literal::Bool(v) => self.asm.mov(Reg::Rax, if *v { 1i32 } else { 0i32 })?,
                Literal::Char(v) => self.asm.mov(Reg::Rax, *v as i32)?,
                Literal::String(s) => {
                    let lab = self.string_label(s);
                    self.asm.lea_rip(Reg::Rax, lab)?;
                }
                Literal::Bytes(b) => {
                    let s = unsafe { String::from_utf8_unchecked(b.clone()) };
                    let lab = self.string_label(&s);
                    self.asm.lea_rip(Reg::Rax, lab)?;
                }
                Literal::Float(v) => {
                    let bits = v.to_bits();
                    self.asm.movabs(Reg::Rax, bits)?;
                }
                Literal::Null => self.asm.mov(Reg::Rax, 0i32)?,
            },
            ExprKind::Var(name) => {
                if let Some(&lab) = self.global_labels.get(name) {
                    self.asm.lea_rip(Reg::Rax, lab)?;
                    match &e.ty {
                        Type::I8 | Type::U8 | Type::Bool | Type::Char => {
                            self.asm.movsx8(Reg::Rax, Mem::base(Reg::Rax))?;
                        }
                        Type::I16 | Type::U16 => {
                            self.asm.movsx16(Reg::Rax, Mem::base(Reg::Rax))?;
                        }
                        Type::I32 | Type::U32 => {
                            self.asm.mov32(Reg::Rax, Mem::base(Reg::Rax))?;
                        }
                        _ => {
                            self.asm.mov(Reg::Rax, Mem::base(Reg::Rax))?;
                        }
                    }
                    return Ok(());
                }
                let slot = self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("unknown variable: {}", name))?;
                match &e.ty {
                    Type::I8
                    | Type::I16
                    | Type::I32
                    | Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::Bool
                    | Type::Char => {
                        self.asm
                            .lea(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset))?;
                        self.load_from_addr(&e.ty)?;
                    }
                    Type::F32 | Type::F64 => {
                        // Load float bits from stack into RAX
                        self.asm
                            .mov(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset))?;
                    }
                    Type::Slice(_) => {
                        #[allow(unreachable_code)]
                        // For now, just load the data pointer
                        self.asm
                            .mov(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset))?;
                    }
                    _ => {
                        self.asm
                            .mov(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset))?;
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
                    self.asm.add(Reg::Rax, off as i32)?;
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
            ExprKind::SizeOf(ty) => {
                let size = self.type_size_bytes(ty);
                self.asm.mov(Reg::Rax, size as i32)?;
            }
            ExprKind::OffsetOf { ty, field } => {
                let off = match ty {
                    Type::Struct(name) => self.field_offset(name, *field)?,
                    _ => bail!("offsetof on non-struct type"),
                };
                self.asm.mov(Reg::Rax, off as i32)?;
            }
        }
        Ok(())
    }

    pub(super) fn eval_bin(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        _ty: &Type,
    ) -> Result<()> {
        if op.is_logical() {
            return self.eval_logical(op, left, right);
        }

        // Floating-point arithmetic: route through SSE
        if left.ty.is_float() && (op.is_arithmetic() || op.is_comparison()) {
            self.require_floats()?;
            return self.eval_float_bin(op, left, right);
        }

        self.eval_expr(left)?;
        self.asm.mov(Reg::R10, Reg::Rax)?; // left -> R10
        self.asm.push(Reg::R10)?; // preserve left across right evaluation
        self.eval_expr(right)?; // right -> RAX
        self.asm.pop(Reg::R10)?; // restore left

        if op.is_arithmetic() {
            match op {
                BinOp::Add => {
                    // C-style pointer arithmetic: when one operand is a
                    // pointer and the other an integer, scale the integer by
                    // the pointee size before adding.  All scalar sizes are
                    // powers of two, so the scaling is a shift.
                    if let (Some(elem), true) =
                        (self.ptr_elem_size(&left.ty), right.ty.is_integer())
                    {
                        if elem > 1 {
                            self.asm.shl(Reg::Rax, elem.trailing_zeros() as i8)?;
                        }
                        self.asm.add(Reg::Rax, Reg::R10)?;
                    } else if let (Some(elem), true) =
                        (self.ptr_elem_size(&right.ty), left.ty.is_integer())
                    {
                        if elem > 1 {
                            self.asm.mov(Reg::R11, Reg::R10)?;
                            self.asm.shl(Reg::R11, elem.trailing_zeros() as i8)?;
                            self.asm.add(Reg::Rax, Reg::R11)?;
                        } else {
                            self.asm.add(Reg::Rax, Reg::R10)?;
                        }
                    } else {
                        self.asm.add(Reg::Rax, Reg::R10)?;
                    }
                }
                BinOp::Sub => {
                    self.asm.mov(Reg::Rdx, Reg::Rax)?; // right
                    if let (Some(elem), true) =
                        (self.ptr_elem_size(&left.ty), right.ty.is_integer())
                        && elem > 1
                    {
                        self.asm.shl(Reg::Rdx, elem.trailing_zeros() as i8)?;
                    }
                    self.asm.mov(Reg::Rax, Reg::R10)?; // left
                    self.asm.sub(Reg::Rax, Reg::Rdx)?;
                }
                BinOp::Mul => {
                    self.asm.mov(Reg::Rdx, Reg::Rax)?;
                    self.asm.mov(Reg::Rax, Reg::R10)?;
                    self.asm.imul(Reg::Rax, Reg::Rdx)?;
                }
                BinOp::Div | BinOp::Mod => {
                    self.asm.mov(Reg::R11, Reg::Rax)?;
                    self.asm.mov(Reg::Rax, Reg::R10)?; // dividend
                    if left.ty.is_signed() {
                        self.asm.cqo()?;
                        self.asm.idiv(Reg::R11)?;
                    } else {
                        self.asm.xor(Reg::Rdx, Reg::Rdx)?;
                        self.asm.div(Reg::R11)?;
                    }
                    if op == BinOp::Mod {
                        self.asm.mov(Reg::Rax, Reg::Rdx)?; // remainder
                    }
                }
                BinOp::FloorDiv => {
                    // Floor division: floor(a/b)
                    // For unsigned: same as trunc division
                    // For signed: if remainder != 0 and quotient < 0, subtract 1
                    self.asm.mov(Reg::R11, Reg::Rax)?; // divisor
                    self.asm.mov(Reg::Rax, Reg::R10)?; // dividend
                    if left.ty.is_signed() {
                        self.asm.cqo()?;
                        self.asm.idiv(Reg::R11)?; // rax = quotient, rdx = remainder
                        // Adjust: if remainder != 0 and quotient < 0, quotient -= 1
                        self.asm.push(Reg::Rax)?; // save quotient
                        self.asm.test(Reg::Rdx, Reg::Rdx)?;
                        let skip = self.asm.new_label();
                        self.asm.je(skip)?; // remainder == 0, no adjustment
                        self.asm.pop(Reg::Rax)?; // restore quotient
                        self.asm.test(Reg::Rax, Reg::Rax)?;
                        self.asm.jcc(Cond::Ge, skip)?; // quotient >= 0, signs were same
                        self.asm.dec(Reg::Rax)?; // floor = trunc - 1
                        self.bind_label(skip);
                    } else {
                        self.asm.xor(Reg::Rdx, Reg::Rdx)?;
                        self.asm.div(Reg::R11)?;
                    }
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        if matches!(
            op,
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
        ) {
            match op {
                BinOp::BitAnd => self.asm.and(Reg::Rax, Reg::R10)?,
                BinOp::BitOr => self.asm.or(Reg::Rax, Reg::R10)?,
                BinOp::BitXor => self.asm.xor(Reg::Rax, Reg::R10)?,
                BinOp::Shl => {
                    self.asm.mov(Reg::Rcx, Reg::Rax)?;
                    self.asm.mov(Reg::Rax, Reg::R10)?;
                    self.asm.shl_cl(Reg::Rax)?;
                }
                BinOp::Shr => {
                    self.asm.mov(Reg::Rcx, Reg::Rax)?;
                    self.asm.mov(Reg::Rax, Reg::R10)?;
                    self.asm.sar_cl(Reg::Rax)?;
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        self.asm.cmp(Reg::R10, Reg::Rax)?;
        let cond = cond_for_cmp(op, &left.ty)?;
        self.asm.setcc(cond, Reg::Rax.r8())?;
        self.asm.movzx8(Reg::Rax, Reg::Rax)?;
        Ok(())
    }

    pub(super) fn eval_float_bin(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        // Evaluate left and right, each producing f64 bits in RAX.
        // Move to XMM registers via stack, perform operation, return bits in RAX.
        self.eval_expr(left)?;
        self.asm.push(Reg::Rax)?;
        self.asm.movsd_xmm_mem(Reg::Xmm0, Mem::base(Reg::Rsp))?; // left -> XMM0
        self.asm.pop(Reg::Rax)?;

        self.eval_expr(right)?;
        self.asm.push(Reg::Rax)?;
        self.asm.movsd_xmm_mem(Reg::Xmm1, Mem::base(Reg::Rsp))?; // right -> XMM1
        self.asm.pop(Reg::Rax)?;

        match op {
            BinOp::Add => self.asm.addsd(Reg::Xmm0, Reg::Xmm1)?,
            BinOp::Sub => self.asm.subsd(Reg::Xmm0, Reg::Xmm1)?,
            BinOp::Mul => self.asm.mulsd(Reg::Xmm0, Reg::Xmm1)?,
            BinOp::Div => self.asm.divsd(Reg::Xmm0, Reg::Xmm1)?,
            BinOp::Mod => {
                // a mod b = a - (a/b)*b, but for simplicity: a - trunc(a/b)*b
                // Use: XMM0 = a, XMM1 = b
                // XMM2 = a / b
                self.asm.movsd_xmm_xmm(Reg::Xmm2, Reg::Xmm0)?;
                self.asm.divsd(Reg::Xmm2, Reg::Xmm1)?;
                // Truncate to integer and back
                self.asm.push(Reg::Rax)?;
                self.asm.movsd_mem_xmm(Mem::base(Reg::Rsp), Reg::Xmm2)?;
                self.asm.pop(Reg::Rax)?;
                // For now, just do a - (a/b)*b using integer truncation
                self.asm.cvttsd2si(Reg::Rax, Reg::Xmm2)?;
                self.asm.cvtsi2sd(Reg::Xmm2, Reg::Rax)?;
                self.asm.mulsd(Reg::Xmm2, Reg::Xmm1)?;
                self.asm.subsd(Reg::Xmm0, Reg::Xmm2)?;
            }
            _ => {
                // Comparison ops
                self.asm.ucomisd(Reg::Xmm0, Reg::Xmm1)?;
                let cond = match op {
                    BinOp::Eq => Cond::E,
                    BinOp::Ne => Cond::Ne,
                    BinOp::Lt => Cond::B, // below (unordered-safe: use B for lt)
                    BinOp::Le => Cond::Be,
                    BinOp::Gt => Cond::A,
                    BinOp::Ge => Cond::Ae,
                    _ => unreachable!(),
                };
                self.asm.setcc(cond, Reg::Rax.r8())?;
                self.asm.movzx8(Reg::Rax, Reg::Rax)?;
                return Ok(());
            }
        }

        // Store XMM0 result to stack, load bits into RAX
        self.asm.push(Reg::Rax)?;
        self.asm.movsd_mem_xmm(Mem::base(Reg::Rsp), Reg::Xmm0)?;
        self.asm.pop(Reg::Rax)?;
        Ok(())
    }

    pub(super) fn eval_logical(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<()> {
        let short = self.asm.new_label();
        let end = self.asm.new_label();
        self.eval_expr(left)?;
        self.asm.test(Reg::Rax, Reg::Rax)?;
        match op {
            BinOp::And => self.asm.je(short)?,
            BinOp::Or => self.asm.jne(short)?,
            _ => unreachable!(),
        }
        self.eval_expr(right)?;
        self.asm.jmp(end)?;
        self.bind_label(short);
        match op {
            BinOp::And => self.asm.mov(Reg::Rax, 0i32)?,
            BinOp::Or => self.asm.mov(Reg::Rax, 1i32)?,
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
            self.store_scalar(slot.offset)?;
            arg_slots.push(slot);
        }

        for (i, slot) in arg_slots.iter().enumerate() {
            let reg = abi_reg(i)?;
            self.asm.mov(reg, Mem::base_disp(Reg::Rbp, slot.offset))?;
        }

        self.asm.call(target)?;
        Ok(())
    }

    pub(super) fn eval_cast(&mut self, expr: &Expr, to: &Type) -> Result<()> {
        self.eval_expr(expr)?;
        if &expr.ty == to {
            return Ok(());
        }
        if expr.ty.is_float() || to.is_float() {
            self.require_floats()?;
        }
        match (expr.ty.clone(), to.clone()) {
            (Type::Ptr(_), _) if to.is_integer() => {}
            (_, Type::Ptr(_)) if expr.ty.is_integer() => {}
            (Type::I8, Type::I64) | (Type::I8, Type::I32) => {
                self.asm.movsx8(Reg::Rax, Reg::Rax)?;
            }
            (Type::I16, Type::I64) | (Type::I16, Type::I32) => {
                self.asm.movsx16(Reg::Rax, Reg::Rax)?;
            }
            (Type::I32, Type::I64) => {
                self.asm.movsxd(Reg::Rax, Reg::Rax)?;
            }
            (Type::U8 | Type::Char | Type::Bool, _) if to.is_integer() => {
                self.asm.movzx8(Reg::Rax, Reg::Rax)?;
            }
            (Type::U16, _) if to.is_integer() => {
                self.asm.movzx16(Reg::Rax, Reg::Rax)?;
            }
            // Integer to float cast
            (src, Type::F64) if src.is_integer() => {
                // src is integer in RAX, convert to double in XMM7, return bits in RAX
                self.asm.cvtsi2sd(Reg::Xmm7, Reg::Rax)?;
                self.asm.push(Reg::Rax)?;
                self.asm.movsd_mem_xmm(Mem::base(Reg::Rsp), Reg::Xmm7)?;
                self.asm.pop(Reg::Rax)?;
            }
            (src, Type::F32) if src.is_integer() => {
                self.asm.cvtsi2sd(Reg::Xmm7, Reg::Rax)?;
                self.asm.push(Reg::Rax)?;
                self.asm.movsd_mem_xmm(Mem::base(Reg::Rsp), Reg::Xmm7)?;
                self.asm.pop(Reg::Rax)?;
            }
            // Float to integer cast
            (Type::F64 | Type::F32, to) if to.is_integer() => {
                // f64 bits in RAX, move to XMM, convert, result in RAX
                self.asm.push(Reg::Rax)?;
                self.asm.movsd_xmm_mem(Reg::Xmm7, Mem::base(Reg::Rsp))?;
                self.asm.pop(Reg::Rax)?;
                self.asm.cvttsd2si(Reg::Rax, Reg::Xmm7)?;
            }
            // Float to float (f32 <-> f64, treat as bits)
            (Type::F32, Type::F64) | (Type::F64, Type::F32) => {
                // For now, just pass through bits (both are 64-bit on stack)
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
                self.asm
                    .lea(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset))?;
            }
            ExprKind::Gep { base, field } => {
                self.eval_expr(base)?;
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Rax, off as i32)?;
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
                self.asm
                    .lea(Reg::Rax, Mem::base_disp(Reg::Rbp, slot.offset))?;
            }
            LValue::Deref(ptr) => {
                self.eval_expr(ptr)?; // pointer value is already the address
            }
            LValue::Field { base, field } => {
                self.eval_expr(base)?; // pointer to struct
                let (struct_name, _) = self.struct_ptr_layout(&base.ty)?;
                let off = self.field_offset(&struct_name, *field)?;
                if off != 0 {
                    self.asm.add(Reg::Rax, off as i32)?;
                }
            }
        }
        Ok(())
    }
}
