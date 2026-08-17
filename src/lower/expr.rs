use super::*;

impl LowerCtx<'_> {
    pub(super) fn lower_lvalue(&mut self, expr: &ast::Expr) -> Result<ir::LValue> {
        match expr {
            ast::Expr::Ident(name) => Ok(ir::LValue::Var(name.clone())),
            ast::Expr::Unary(u) if u.op == ast::UnOp::Deref => {
                Ok(ir::LValue::Deref(self.lower_expr(&u.operand)?))
            }
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
        let def = self
            .structs
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?;
        let idx = def
            .fields
            .iter()
            .position(|(n, _)| n == field)
            .ok_or_else(|| anyhow::anyhow!("unknown field {}.{}", name, field))?;
        Ok((name, idx))
    }

    pub(super) fn resolve_enum_variant<'a>(
        &self,
        ty: &'a ir::Type,
        field: &str,
    ) -> Result<(&'a str, i64)> {
        let struct_name = match ty {
            ir::Type::Struct(n) => n.as_str(),
            _ => bail!("variant access on non-struct type: {:?}", ty),
        };
        // Synthetic enum structs are named __enum_<EnumName>
        let enum_name = struct_name.strip_prefix("__enum_").unwrap_or(struct_name);
        let def = self
            .enums
            .get(enum_name)
            .ok_or_else(|| anyhow::anyhow!("unknown enum: {}", enum_name))?;
        let variant = def
            .variants
            .iter()
            .find(|v| v.name == field)
            .ok_or_else(|| anyhow::anyhow!("unknown variant {}.{}", enum_name, field))?;
        Ok((struct_name, variant.discriminant))
    }

    /// Lower an enum variant constructor like `Color.Red` into a struct literal
    /// that sets the tag field to the variant's discriminant.
    pub(super) fn lower_enum_variant(
        &mut self,
        struct_name: &str,
        discriminant: i64,
    ) -> Result<ir::Expr> {
        self.lower_struct_literal_with_tag(struct_name, discriminant, None)
    }

    pub(super) fn lower_enum_variant_with_payload_ir(
        &mut self,
        struct_name: &str,
        discriminant: i64,
        payload: ir::Expr,
    ) -> Result<ir::Expr> {
        self.lower_struct_literal_with_tag(struct_name, discriminant, Some(payload))
    }

    /// Check if source integer type can be cast to target integer type
    /// without data loss concerns (both are integers, possibly different widths).
    fn is_compatible_integer(&self, source: &ir::Type, target: &ir::Type) -> bool {
        source.is_integer() && target.is_integer()
    }

    /// Build a synthetic enum struct on the stack with tag (and optionally payload).
    fn lower_struct_literal_with_tag(
        &mut self,
        struct_name: &str,
        tag_value: i64,
        payload: Option<ir::Expr>,
    ) -> Result<ir::Expr> {
        let def = self
            .structs
            .get(struct_name)
            .ok_or_else(|| anyhow::anyhow!("unknown enum struct: {}", struct_name))?;
        let ptr_ty = ir::Type::Ptr(Box::new(ir::Type::Struct(struct_name.to_string())));

        let total_size: usize = def
            .fields
            .iter()
            .map(|(_, ty)| match ty {
                ir::Type::I8 | ir::Type::U8 | ir::Type::Char | ir::Type::Bool => 1,
                ir::Type::I16 | ir::Type::U16 => 2,
                ir::Type::I32 | ir::Type::U32 | ir::Type::F32 => 4,
                _ => 8,
            })
            .sum();
        let count = total_size.div_ceil(8).max(1);

        let slot_name = self.fresh_temp("$enum");
        let mut stmts = vec![ir::Stmt::StackAlloc {
            name: slot_name.clone(),
            elem_ty: ir::Type::I64,
            count,
        }];

        // Store tag (field 0)
        let var_expr = ir::Expr::new(ir::ExprKind::Var(slot_name.clone()), ptr_ty.clone());
        let gep_tag = ir::Expr::new(
            ir::ExprKind::Gep {
                base: Box::new(var_expr.clone()),
                field: 0,
            },
            ir::Type::Ptr(Box::new(ir::Type::I32)),
        );
        let lit_tag = ir::Expr::new(
            ir::ExprKind::Lit(ir::Literal::Int(tag_value)),
            ir::Type::I64,
        );
        let cast_tag = ir::Expr::new(
            ir::ExprKind::Cast {
                expr: Box::new(lit_tag),
                ty: ir::Type::I32,
            },
            ir::Type::I32,
        );
        stmts.push(ir::Stmt::Assign {
            lhs: ir::LValue::Deref(gep_tag),
            rhs: cast_tag,
        });

        // Store payload (field 1) if present
        if let Some(payload_expr) = payload {
            let gep_payload = ir::Expr::new(
                ir::ExprKind::Gep {
                    base: Box::new(var_expr),
                    field: 1,
                },
                ir::Type::Ptr(Box::new(payload_expr.ty.clone())),
            );
            stmts.push(ir::Stmt::Assign {
                lhs: ir::LValue::Deref(gep_payload),
                rhs: payload_expr,
            });
        }

        let result_expr = ir::Expr::new(ir::ExprKind::Var(slot_name), ptr_ty.clone());
        Ok(ir::Expr::new(
            ir::ExprKind::Block(stmts, Box::new(result_expr)),
            ptr_ty,
        ))
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
            ast::Expr::GenericApp { name, .. } => {
                bail!("generic type `{}` cannot be used as a standalone expression", name)
            }
            ast::Expr::Cast(c) => self.lower_cast(c),
            ast::Expr::Field(f) => self.lower_field(f),
            ast::Expr::Index(i) => self.lower_index(i),
            ast::Expr::Match(m) => self.lower_match_expr(m),
            ast::Expr::SizeOf(s) => {
                let ty = self.lower_type(&s.ty)?;
                Ok(ir::Expr::new(ir::ExprKind::SizeOf(ty), ir::Type::U64))
            }
            ast::Expr::OffsetOf(o) => {
                let ty = self.lower_type(&o.ty)?;
                let field_idx = match &ty {
                    ir::Type::Struct(name) => {
                        let def = self
                            .structs
                            .get(name)
                            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?;
                        def.fields
                            .iter()
                            .position(|(n, _)| n == &o.field)
                            .ok_or_else(|| anyhow::anyhow!("unknown field {}.{}", name, o.field))?
                    }
                    _ => bail!("offsetof on non-struct type"),
                };
                Ok(ir::Expr::new(
                    ir::ExprKind::OffsetOf {
                        ty,
                        field: field_idx,
                    },
                    ir::Type::U64,
                ))
            }
            ast::Expr::If(_) => bail!("if-expressions are not supported in the first milestone"),
            ast::Expr::Block(b) => self.lower_block_expr(b),
            ast::Expr::Loop(_) => {
                bail!("loop expressions are not supported in the first milestone")
            }
            ast::Expr::Break => bail!("break is not supported in expression position"),
            ast::Expr::Continue => bail!("continue is not supported in expression position"),
            ast::Expr::Tuple(elems) => self.lower_tuple_expr(elems),
            ast::Expr::Array(elems) => {
                if elems.is_empty() {
                    bail!("empty array literals are not supported");
                }
                let elem_ty = self.lower_expr(&elems[0])?.ty.clone();
                let count = elems.len();
                let ptr_ty = ir::Type::Ptr(Box::new(elem_ty.clone()));

                // Allocate stack space
                let slot_name = self.fresh_temp("$arr");
                let mut stmts = vec![ir::Stmt::StackAlloc {
                    name: slot_name.clone(),
                    elem_ty: elem_ty.clone(),
                    count,
                }];

                // Store each element using pointer arithmetic
                for (i, elem) in elems.iter().enumerate() {
                    let value = self.lower_expr(elem)?;
                    let var_expr =
                        ir::Expr::new(ir::ExprKind::Var(slot_name.clone()), ptr_ty.clone());
                    let idx_expr =
                        ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(i as i64)), ir::Type::I64);
                    let addr = ir::Expr::new(
                        ir::ExprKind::Bin {
                            op: ir::BinOp::Add,
                            left: Box::new(var_expr),
                            right: Box::new(idx_expr),
                        },
                        ptr_ty.clone(),
                    );
                    stmts.push(ir::Stmt::Assign {
                        lhs: ir::LValue::Deref(addr),
                        rhs: value,
                    });
                }

                // Return a block expression yielding the pointer
                let result_expr = ir::Expr::new(ir::ExprKind::Var(slot_name), ptr_ty.clone());
                Ok(ir::Expr::new(
                    ir::ExprKind::Block(stmts, Box::new(result_expr)),
                    ptr_ty,
                ))
            }
            ast::Expr::Range(_) => bail!("range expressions are only allowed in for-loops"),
            ast::Expr::RefMut(_) => {
                bail!("refmut expressions are not supported in the first milestone")
            }
            ast::Expr::StructLiteral {
                name,
                generic_args,
                fields,
            } => {
                let resolved = if generic_args.is_empty() {
                    name.clone()
                } else {
                    let arg_tys: Vec<crate::ty::Type> = generic_args
                        .iter()
                        .map(|t| {
                            let ty = self.lower_type(t)?;
                            Ok(self.ir_to_sema(&ty))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    self.ensure_mono_struct(name, &arg_tys)?
                };
                self.lower_struct_literal(&resolved, fields)
            }
            ast::Expr::UnsafeBlock(b) => self.lower_block_expr(b),
        }
    }

    pub(super) fn lower_literal_expr(&self, lit: &ast::Literal) -> Result<ir::Expr> {
        let lit = self.lower_literal(lit)?;
        let ty = match lit {
            ir::Literal::Int(_) => ir::Type::I64,
            ir::Literal::Float(_) => ir::Type::F64,
            ir::Literal::Bool(_) => ir::Type::Bool,
            ir::Literal::Char(_) => ir::Type::Char,
            ir::Literal::String(_) => ir::Type::Ptr(Box::new(ir::Type::Char)),
            ir::Literal::Bytes(_) => ir::Type::Ptr(Box::new(ir::Type::U8)),
            ir::Literal::Null => ir::Type::Ptr(Box::new(ir::Type::Void)),
        };
        Ok(ir::Expr::new(ir::ExprKind::Lit(lit), ty))
    }

    fn lower_ident(&self, name: &str) -> Result<ir::Expr> {
        if let Some(ty) = self.vars.get(name) {
            Ok(ir::Expr::new(
                ir::ExprKind::Var(name.to_string()),
                ty.clone(),
            ))
        } else if self.enums.contains_key(name) {
            let struct_name = enum_struct_name(name);
            Ok(ir::Expr::new(
                ir::ExprKind::Var(name.to_string()),
                ir::Type::Struct(struct_name),
            ))
        } else {
            Ok(ir::Expr::new(
                ir::ExprKind::Var(name.to_string()),
                ir::Type::I64,
            ))
        }
    }

    fn lower_binary(&mut self, b: &ast::BinaryExpr) -> Result<ir::Expr> {
        // Power desugars to a runtime call
        if b.op == ast::BinOp::Power {
            let left = self.lower_expr(&b.left)?;
            let right = self.lower_expr(&b.right)?;
            if !left.ty.is_integer() || !right.ty.is_integer() {
                bail!(
                    "power operator requires integer operands, found `{:?}` and `{:?}`",
                    left.ty,
                    right.ty
                );
            }
            let ty = left.ty.clone();
            return Ok(ir::Expr::new(
                ir::ExprKind::Call {
                    func: "__forge_pow".to_string(),
                    args: vec![left, right],
                },
                ty,
            ));
        }
        let left = self.lower_expr(&b.left)?;
        let right = self.lower_expr(&b.right)?;
        let op = self.lower_binop(b.op)?;
        let ty = if op.is_comparison() || op.is_logical() {
            ir::Type::Bool
        } else {
            left.ty.clone()
        };
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
        // Check if this is an enum variant constructor with payload: Color.Some(x)
        if let ast::Expr::Field(field) = c.callee.as_ref()
            && let ast::Expr::Ident(enum_name) = field.object.as_ref()
        {
            let enum_info = self.enums.get(enum_name).cloned();
            if let Some(info) = enum_info
                && let Some(variant) = info.variants.iter().find(|v| v.name == field.field)
            {
                if let Some(payload_ty) = &variant.payload {
                    if c.args.len() != 1 {
                        bail!(
                            "variant `{}` expects 1 argument, got {}",
                            field.field,
                            c.args.len()
                        );
                    }
                    let struct_name = enum_struct_name(enum_name);
                    let payload_expr = self.lower_expr(&c.args[0])?;
                    // Allow integer literals to match any integer type by inserting a cast
                    let casted_payload = if payload_expr.ty != *payload_ty
                        && self.is_compatible_integer(&payload_expr.ty, payload_ty)
                    {
                        ir::Expr::new(
                            ir::ExprKind::Cast {
                                expr: Box::new(payload_expr.clone()),
                                ty: payload_ty.clone(),
                            },
                            payload_ty.clone(),
                        )
                    } else if payload_expr.ty != *payload_ty {
                        bail!(
                            "variant `{}` payload type mismatch: expected {:?}, got {:?}",
                            field.field,
                            payload_ty,
                            payload_expr.ty
                        )
                    } else {
                        payload_expr
                    };

                    // Store the payload type for the struct field
                    let lowered_payload = casted_payload;

                    return self.lower_enum_variant_with_payload_ir(
                        &struct_name,
                        variant.discriminant,
                        lowered_payload,
                    );
                } else {
                    bail!(
                        "variant `{}` has no payload; use `{}`.{} without arguments",
                        field.field,
                        enum_name,
                        field.field
                    );
                }
            }
        }

        let name = match c.callee.as_ref() {
            ast::Expr::Ident(n) => n.clone(),
            _ => bail!("only direct function calls are supported in the first milestone"),
        };
        let args = c
            .args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<Result<Vec<_>>>()?;

        // Call to a generic function: infer the concrete type arguments from
        // the argument types and route to the monomorphized instance.
        if let Some(def) = self.generic_defs.get(&name).cloned() {
            let arg_tys: Vec<crate::ty::Type> = args
                .iter()
                .map(|a| self.ir_to_sema(&a.ty))
                .collect();
            let mut mapping: std::collections::HashMap<String, crate::ty::Type> =
                std::collections::HashMap::new();
            for (p, a) in def.patterns.iter().zip(arg_tys.iter()) {
                if self.lower_collect(p, a, &mut mapping).is_none() {
                    bail!("could not infer generic arguments for `{}`", name);
                }
            }
            let generic_args: Vec<crate::ty::Type> = def
                .generics
                .iter()
                .map(|g| mapping.get(g).cloned().unwrap_or(crate::ty::Type::Unknown))
                .collect();
            let mangled = self.register_instance(&name, &def, generic_args)?;
            let ret = self
                .funcs
                .get(&mangled)
                .map(|(_, r)| r.clone())
                .unwrap_or(ir::Type::Void);
            let ret = match &ret {
                ir::Type::Struct(s) if !s.starts_with("__enum_") => {
                    ir::Type::Ptr(Box::new(ret))
                }
                _ => ret,
            };
            return Ok(ir::Expr::new(ir::ExprKind::Call { func: mangled, args }, ret));
        }

        let ret = self
            .funcs
            .get(&name)
            .or_else(|| self.externs.get(&name))
            .map(|(_, r)| r.clone())
            .unwrap_or(ir::Type::Void);
        // Struct-returning calls produce a pointer to the caller's scratch
        // slot (return-by-pointer ABI); the synthetic `__enum_*` structs are
        // returned inline as before.
        let ret = match &ret {
            ir::Type::Struct(s) if !s.starts_with("__enum_") => {
                ir::Type::Ptr(Box::new(ret))
            }
            _ => ret,
        };
        Ok(ir::Expr::new(ir::ExprKind::Call { func: name, args }, ret))
    }

    fn lower_cast(&mut self, c: &ast::CastExpr) -> Result<ir::Expr> {
        let expr = self.lower_expr(&c.expr)?;
        let ty = self.lower_type(&c.ty)?;
        Ok(ir::Expr::new(
            ir::ExprKind::Cast {
                expr: Box::new(expr),
                ty: ty.clone(),
            },
            ty,
        ))
    }

    fn lower_field(&mut self, f: &ast::FieldExpr) -> Result<ir::Expr> {
        let base = self.lower_expr(&f.object)?;

        // Check if this is an enum variant constructor: Color.Red, Option.Some
        if let ir::Type::Struct(name) = &base.ty
            && name.starts_with("__enum_")
            && let Ok((struct_name, discriminant)) = self.resolve_enum_variant(&base.ty, &f.field)
        {
            return self.lower_enum_variant(struct_name, discriminant);
        }

        let (struct_name, idx) = self.resolve_field(&base.ty, &f.field)?;
        let struct_ty = ir::Type::Struct(struct_name.clone());
        let ptr_ty = ir::Type::Ptr(Box::new(struct_ty.clone()));
        let base_ptr = if matches!(base.ty, ir::Type::Ptr(_)) {
            base
        } else {
            ir::Expr::new(ir::ExprKind::AddrOf(Box::new(base)), ptr_ty.clone())
        };
        let field_ty = self.structs.get(&struct_name).unwrap().fields[idx]
            .1
            .clone();
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

    /// Lower a block expression `{ ...; trailing }` or `unsafe { ...; trailing }`.
    ///
    /// Mirrors the pattern used by `lower_match_case_value`: split the block
    /// into prefix statements and a trailing statement.  If the last statement
    /// is an expression, it becomes the block's value; otherwise the block's
    /// value is `void` (a null literal).  Nested `Block` trailers (e.g. from
    /// struct/tuple literals) are flattened so the resulting `ir::ExprKind::Block`
    /// is single-level, matching what the codegen and `lower_binding` expect.
    fn lower_block_expr(&mut self, block: &ast::Block) -> Result<ir::Expr> {
        let mut stmts = Vec::new();

        if block.stmts.is_empty() {
            let trailing = ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Null), ir::Type::Void);
            return Ok(ir::Expr::new(
                ir::ExprKind::Block(stmts, Box::new(trailing)),
                ir::Type::Void,
            ));
        }

        let (last, prefix) = block.stmts.split_last().unwrap();
        for s in prefix {
            stmts.extend(self.lower_stmt(s)?);
        }

        let trailing = match last {
            ast::Stmt::Expr(e) => {
                let expr = self.lower_expr(e)?;
                match expr.kind {
                    ir::ExprKind::Block(inner, result) => {
                        stmts.extend(inner);
                        *result
                    }
                    _ => expr,
                }
            }
            _ => {
                stmts.extend(self.lower_stmt(last)?);
                ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Null), ir::Type::Void)
            }
        };

        let ty = trailing.ty.clone();
        Ok(ir::Expr::new(
            ir::ExprKind::Block(stmts, Box::new(trailing)),
            ty,
        ))
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
            ast::BinOp::FloorDiv => Ok(ir::BinOp::FloorDiv),
            ast::BinOp::Power => Ok(ir::BinOp::Power),
        }
    }
}
