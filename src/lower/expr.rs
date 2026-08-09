use super::*;

impl LowerCtx<'_> {
    pub(super) fn lower_lvalue(
        &mut self,
        expr: &ast::Expr,
    ) -> Result<ir::LValue> {
        match expr {
            ast::Expr::Ident(name) => Ok(ir::LValue::Var(name.clone())),
            ast::Expr::Unary(u) if u.op == ast::UnOp::Deref => Ok(ir::LValue::Deref(self.lower_expr(&u.operand)?)),
            ast::Expr::Deref(d) => Ok(ir::LValue::Deref(self.lower_expr(&d.expr)?)),
            ast::Expr::Field(f) => {
                let base = self.lower_expr(&f.object)?;
                let (struct_name, idx) = self.resolve_field(&base.ty, &f.field)?;
                let struct_ty = ir::Type::Struct(struct_name.clone());
                let ptr_struct = ir::Type::Ptr(Box::new(struct_ty));
                // If the base is already a pointer to the struct, use it
                // directly; otherwise take its address.
                let base_ptr = if matches!(base.ty, ir::Type::Ptr(_)) {
                    base
                } else {
                    ir::Expr::new(ir::ExprKind::AddrOf(Box::new(base)), ptr_struct.clone())
                };
                Ok(ir::LValue::Field {
                    base: base_ptr,
                    field: idx,
                })
            }
            ast::Expr::Index(i) => {
                let object = self.lower_expr(&i.object)?;
                let index = self.lower_expr(&i.index)?;
                let ptr_ty = object.ty.clone();
                Ok(ir::LValue::Deref(ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::Add,
                        left: Box::new(object),
                        right: Box::new(index),
                    },
                    ptr_ty,
                )))
            }
            _ => bail!("invalid assignment target"),
        }
    }

    pub(super) fn resolve_field(&self, ty: &ir::Type, field: &str) -> Result<(String, usize)> {
        let name = match ty {
            ir::Type::Struct(n) => n.clone(),
            ir::Type::Ptr(inner) => match inner.as_ref() {
                ir::Type::Struct(n) => n.clone(),
                _ => bail!("field access on non-struct pointer"),
            },
            _ => bail!("field access on non-struct type: {:?}", ty),
        };
        let def = self.structs.get(&name).ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?;
        let idx = def.fields.iter().position(|(n, _)| n == field).ok_or_else(|| anyhow::anyhow!("unknown field {}.{}", name, field))?;
        Ok((name, idx))
    }

    pub(super) fn lower_expr(&mut self, expr: &ast::Expr) -> Result<ir::Expr> {
        match expr {
            ast::Expr::Literal(l) => self.lower_literal_expr(l),
            ast::Expr::Ident(name) => self.lower_ident(name),
            ast::Expr::Binary(b) => self.lower_binary(b),
            ast::Expr::Unary(u) => self.lower_unary(u),
            ast::Expr::Deref(d) => self.lower_deref(d),
            ast::Expr::Ref(r) => self.lower_ref(r),
            ast::Expr::Call(c) => self.lower_call(c),
            ast::Expr::Cast(c) => self.lower_cast(c),
            ast::Expr::Field(f) => self.lower_field(f),
            ast::Expr::Index(i) => self.lower_index(i),
            ast::Expr::Asm(a) => self.lower_asm(a),
            ast::Expr::Match(m) => self.lower_match_expr(m),
            ast::Expr::SizeOf(_) => bail!("sizeof is not supported in the first milestone"),
            ast::Expr::OffsetOf(_) => bail!("offsetof is not supported in the first milestone"),
            ast::Expr::If(_) => bail!("if-expressions are not supported in the first milestone"),
            ast::Expr::Block(_) => bail!("block expressions are not supported in the first milestone"),
            ast::Expr::Loop(_) => bail!("loop expressions are not supported in the first milestone"),
            ast::Expr::Break => bail!("break is not supported in expression position"),
            ast::Expr::Continue => bail!("continue is not supported in expression position"),
            ast::Expr::Tuple(_) => bail!("tuples are not supported in the first milestone"),
            ast::Expr::Array(_) => bail!("array literals are not supported in the first milestone"),
            ast::Expr::Range(_) => bail!("range expressions are only allowed in for-loops"),
            ast::Expr::RefMut(_) => bail!("refmut expressions are not supported in the first milestone"),
            ast::Expr::StructLiteral { .. } => bail!("struct literals are not supported in the first milestone"),
            ast::Expr::UnsafeBlock(_) => bail!("unsafe block expressions are not supported in the first milestone"),
        }
    }

    fn lower_literal_expr(&self, lit: &ast::Literal) -> Result<ir::Expr> {
        let lit = self.lower_literal(lit)?;
        let ty = match lit {
            ir::Literal::Int(_) => ir::Type::I64,
            ir::Literal::Float(_) => ir::Type::F64,
            ir::Literal::Bool(_) => ir::Type::Bool,
            ir::Literal::Char(_) => ir::Type::Char,
            ir::Literal::String(_) => ir::Type::Ptr(Box::new(ir::Type::Char)),
            ir::Literal::Null => ir::Type::Ptr(Box::new(ir::Type::Void)),
        };
        Ok(ir::Expr::new(ir::ExprKind::Lit(lit), ty))
    }

    fn lower_ident(&self, name: &str) -> Result<ir::Expr> {
        let ty = self.vars.get(name).cloned().unwrap_or(ir::Type::I64);
        Ok(ir::Expr::new(ir::ExprKind::Var(name.to_string()), ty))
    }

    fn lower_binary(&mut self, b: &ast::BinaryExpr) -> Result<ir::Expr> {
        let left = self.lower_expr(&b.left)?;
        let right = self.lower_expr(&b.right)?;
        let op = self.lower_binop(b.op)?;
        let ty = if op.is_comparison() || op.is_logical() { ir::Type::Bool } else { left.ty.clone() };
        Ok(ir::Expr::new(
            ir::ExprKind::Bin {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        ))
    }

    fn lower_unary(&mut self, u: &ast::UnaryExpr) -> Result<ir::Expr> {
        match u.op {
            ast::UnOp::Neg => {
                let operand = self.lower_expr(&u.operand)?;
                let ty = operand.ty.clone();
                let zero = ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(0)), ty.clone());
                Ok(ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::Sub,
                        left: Box::new(zero),
                        right: Box::new(operand),
                    },
                    ty,
                ))
            }
            ast::UnOp::Not => {
                let operand = self.lower_expr(&u.operand)?;
                let one = ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Bool(true)), ir::Type::Bool);
                let ty = operand.ty.clone();
                Ok(ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::BitXor,
                        left: Box::new(one),
                        right: Box::new(operand),
                    },
                    ty,
                ))
            }
            ast::UnOp::BitNot => {
                let operand = self.lower_expr(&u.operand)?;
                let ty = operand.ty.clone();
                let all_ones = ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(-1)), ty.clone());
                Ok(ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::BitXor,
                        left: Box::new(all_ones),
                        right: Box::new(operand),
                    },
                    ty,
                ))
            }
            ast::UnOp::Deref => {
                let ptr = self.lower_expr(&u.operand)?;
                let ty = match &ptr.ty {
                    ir::Type::Ptr(inner) => *inner.clone(),
                    _ => bail!("dereference of non-pointer"),
                };
                Ok(ir::Expr::new(ir::ExprKind::Load(Box::new(ptr)), ty))
            }
            ast::UnOp::Ref => {
                let inner = self.lower_expr(&u.operand)?;
                let ty = ir::Type::Ptr(Box::new(inner.ty.clone()));
                Ok(ir::Expr::new(ir::ExprKind::AddrOf(Box::new(inner)), ty))
            }
        }
    }

    fn lower_deref(&mut self, d: &ast::DerefExpr) -> Result<ir::Expr> {
        let ptr = self.lower_expr(&d.expr)?;
        let ty = match &ptr.ty {
            ir::Type::Ptr(inner) => *inner.clone(),
            _ => bail!("dereference of non-pointer"),
        };
        Ok(ir::Expr::new(ir::ExprKind::Load(Box::new(ptr)), ty))
    }

    fn lower_ref(&mut self, r: &ast::RefExpr) -> Result<ir::Expr> {
        let inner = self.lower_expr(&r.expr)?;
        let ty = ir::Type::Ptr(Box::new(inner.ty.clone()));
        Ok(ir::Expr::new(ir::ExprKind::AddrOf(Box::new(inner)), ty))
    }

    fn lower_call(&mut self, c: &ast::CallExpr) -> Result<ir::Expr> {
        let name = match c.callee.as_ref() {
            ast::Expr::Ident(n) => n.clone(),
            _ => bail!("only direct function calls are supported in the first milestone"),
        };
        let args = c
            .args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<Result<Vec<_>>>()?;
        let ret = self
            .funcs
            .get(&name)
            .or_else(|| self.externs.get(&name))
            .map(|(_, r)| r.clone())
            .unwrap_or(ir::Type::Void);
        Ok(ir::Expr::new(ir::ExprKind::Call { func: name, args }, ret))
    }

    fn lower_cast(&mut self, c: &ast::CastExpr) -> Result<ir::Expr> {
        let expr = self.lower_expr(&c.expr)?;
        let ty = self.lower_type(&c.ty)?;
        Ok(ir::Expr::new(ir::ExprKind::Cast { expr: Box::new(expr), ty: ty.clone() }, ty))
    }

    fn lower_field(&mut self, f: &ast::FieldExpr) -> Result<ir::Expr> {
        let base = self.lower_expr(&f.object)?;
        let (struct_name, idx) = self.resolve_field(&base.ty, &f.field)?;
        let struct_ty = ir::Type::Struct(struct_name.clone());
        let ptr_ty = ir::Type::Ptr(Box::new(struct_ty.clone()));
        let base_ptr = if matches!(base.ty, ir::Type::Ptr(_)) {
            base
        } else {
            ir::Expr::new(ir::ExprKind::AddrOf(Box::new(base)), ptr_ty.clone())
        };
        let field_ty = self.structs.get(&struct_name).unwrap().fields[idx].1.clone();
        let ptr_ty = ir::Type::Ptr(Box::new(field_ty.clone()));
        let gep = ir::Expr::new(
            ir::ExprKind::Gep {
                base: Box::new(base_ptr),
                field: idx,
            },
            ptr_ty,
        );
        Ok(ir::Expr::new(ir::ExprKind::Load(Box::new(gep)), field_ty))
    }

    fn lower_index(&mut self, i: &ast::IndexExpr) -> Result<ir::Expr> {
        let object = self.lower_expr(&i.object)?;
        let index = self.lower_expr(&i.index)?;
        let elem_ty = match &object.ty {
            ir::Type::Ptr(inner) => *inner.clone(),
            _ => bail!("indexing a non-pointer expression"),
        };
        let object_ty = object.ty.clone();
        let ptr = ir::Expr::new(
            ir::ExprKind::Bin {
                op: ir::BinOp::Add,
                left: Box::new(object),
                right: Box::new(index),
            },
            object_ty,
        );
        Ok(ir::Expr::new(ir::ExprKind::Load(Box::new(ptr)), elem_ty))
    }

    fn lower_asm(&mut self, a: &ast::AsmExpr) -> Result<ir::Expr> {
        let mut inputs = Vec::new();
        for op in &a.inputs {
            let expr = self.lower_expr(&op.expr)?;
            inputs.push((expr, op.constraint.clone()));
        }
        if a.outputs.len() > 1 {
            bail!("inline assembly with multiple outputs is not supported");
        }
        let output = a.outputs.get(0).map(|o| (ir::Type::Void, o.constraint.clone()));
        Ok(ir::Expr::new(
            ir::ExprKind::Asm {
                template: a.template.clone(),
                constraints: String::new(),
                inputs,
                output,
                clobbers: a.clobbers.clone(),
            },
            ir::Type::Void,
        ))
    }

    pub(super) fn lower_literal(&self, lit: &ast::Literal) -> Result<ir::Literal> {
        match lit {
            ast::Literal::Int(i) => Ok(ir::Literal::Int(*i)),
            ast::Literal::Float(f) => Ok(ir::Literal::Float(*f)),
            ast::Literal::String(s) => Ok(ir::Literal::String(s.clone())),
            ast::Literal::Char(c) => Ok(ir::Literal::Char(*c as u8)),
            ast::Literal::Bool(b) => Ok(ir::Literal::Bool(*b)),
            ast::Literal::Null => Ok(ir::Literal::Null),
        }
    }

    pub(super) fn lower_binop(&self, op: ast::BinOp) -> Result<ir::BinOp> {
        match op {
            ast::BinOp::Add => Ok(ir::BinOp::Add),
            ast::BinOp::Sub => Ok(ir::BinOp::Sub),
            ast::BinOp::Mul => Ok(ir::BinOp::Mul),
            ast::BinOp::Div => Ok(ir::BinOp::Div),
            ast::BinOp::Mod => Ok(ir::BinOp::Mod),
            ast::BinOp::And => Ok(ir::BinOp::And),
            ast::BinOp::Or => Ok(ir::BinOp::Or),
            ast::BinOp::BitAnd => Ok(ir::BinOp::BitAnd),
            ast::BinOp::BitOr => Ok(ir::BinOp::BitOr),
            ast::BinOp::BitXor => Ok(ir::BinOp::BitXor),
            ast::BinOp::Shl => Ok(ir::BinOp::Shl),
            ast::BinOp::Shr => Ok(ir::BinOp::Shr),
            ast::BinOp::Eq => Ok(ir::BinOp::Eq),
            ast::BinOp::Ne => Ok(ir::BinOp::Ne),
            ast::BinOp::Lt => Ok(ir::BinOp::Lt),
            ast::BinOp::Le => Ok(ir::BinOp::Le),
            ast::BinOp::Gt => Ok(ir::BinOp::Gt),
            ast::BinOp::Ge => Ok(ir::BinOp::Ge),
            ast::BinOp::Assign => bail!("assignment is a statement, not an expression"),
            ast::BinOp::FloorDiv => bail!("floor division is not supported in the first milestone"),
            ast::BinOp::Power => bail!("power operator is not supported in the first milestone"),
        }
    }
}
