use super::*;

impl LowerCtx<'_> {
    pub(super) fn lower_block(&mut self, block: &ast::Block) -> Result<Vec<ir::Stmt>> {
        let mut stmts = Vec::new();
        for s in &block.stmts {
            stmts.extend(self.lower_stmt(s)?);
        }
        Ok(stmts)
    }

    pub(super) fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Result<Vec<ir::Stmt>> {
        match stmt {
             ast::Stmt::Let(l) => {
                self.lower_let(&l.pattern, l.ty.as_ref(), &l.value, false)
            }
            ast::Stmt::Var(v) => {
                self.lower_let(&v.pattern, v.ty.as_ref(), &v.value, false)
            }
            ast::Stmt::Assign(a) => {
                let lhs = self.lower_lvalue(&a.target)?;
                let rhs = self.lower_expr(&a.value)?;
                if let ir::ExprKind::Block(stmts, result) = rhs.kind {
                    let mut out = stmts.clone();
                    out.push(ir::Stmt::Assign {
                        lhs,
                        rhs: result.as_ref().clone(),
                    });
                    Ok(out)
                } else {
                    Ok(vec![ir::Stmt::Assign { lhs, rhs }])
                }
            }
            ast::Stmt::CompoundAssign(c) => {
                // `t op= v` desugars to `t = t op v` (op read once).
                let op = self.lower_binop(c.op)?;
                let lhs = self.lower_lvalue(&c.target)?;
                let target_expr = self.lower_expr(&c.target)?;
                let rhs_expr = self.lower_expr(&c.value)?;
                let ty = target_expr.ty.clone();
                let rhs = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op,
                        left: Box::new(target_expr),
                        right: Box::new(rhs_expr),
                    },
                    ty,
                );
                Ok(vec![ir::Stmt::Assign { lhs, rhs }])
            }
            ast::Stmt::Expr(e) => Ok(vec![ir::Stmt::Expr(self.lower_expr(e)?)]),
            ast::Stmt::Return(e) => {
                let expr = e.as_ref().map(|x| self.lower_expr(x)).transpose()?;
                Ok(vec![ir::Stmt::Return(expr)])
            }
            ast::Stmt::If(i) => {
                let cond = self.lower_expr(&i.condition)?;
                let then = self.lower_block(&i.then_block)?;
                let mut else_: Option<Vec<ir::Stmt>> = None;

                // Flatten elif/else into nested if statements.
                if !i.elifs.is_empty() || i.else_block.is_some() {
                    let mut nested = Vec::new();
                    if let Some(ref b) = i.else_block {
                        nested = self.lower_block(b)?;
                    }
                    for (cond, block) in i.elifs.iter().rev() {
                        let then = self.lower_block(block)?;
                        nested = vec![ir::Stmt::If {
                            cond: self.lower_expr(cond)?,
                            then,
                            else_: if nested.is_empty() {
                                None
                            } else {
                                Some(nested)
                            },
                        }];
                    }
                    else_ = if nested.is_empty() {
                        None
                    } else {
                        Some(nested)
                    };
                }

                Ok(vec![ir::Stmt::If { cond, then, else_ }])
            }
            ast::Stmt::While(w) => {
                let cond = self.lower_expr(&w.condition)?;
                let body = self.lower_block(&w.body)?;
                Ok(vec![ir::Stmt::While { cond, body }])
            }
            ast::Stmt::For(f) => {
                // `for var in start..end:` desugars to:
                //   let var = start
                //   while var < end:
                //     body
                //     var = var + 1
                let (start, end, inclusive) = self.lower_range(&f.iter)?;
                let loop_var = f.var.clone();
                let iter_ty = self.infer_expr_type(&start)?;
                self.vars.insert(loop_var.clone(), iter_ty.clone());

                let mut body = self.lower_block(&f.body)?;
                body.push(ir::Stmt::Assign {
                    lhs: ir::LValue::Var(loop_var.clone()),
                    rhs: ir::Expr::new(
                        ir::ExprKind::Bin {
                            op: ir::BinOp::Add,
                            left: Box::new(ir::Expr::new(
                                ir::ExprKind::Var(loop_var.clone()),
                                iter_ty.clone(),
                            )),
                            right: Box::new(ir::Expr::new(
                                ir::ExprKind::Lit(ir::Literal::Int(1)),
                                iter_ty.clone(),
                            )),
                        },
                        iter_ty.clone(),
                    ),
                });

                let cond_op = if inclusive {
                    ir::BinOp::Le
                } else {
                    ir::BinOp::Lt
                };
                let cond = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: cond_op,
                        left: Box::new(ir::Expr::new(
                            ir::ExprKind::Var(loop_var.clone()),
                            iter_ty.clone(),
                        )),
                        right: Box::new(end),
                    },
                    ir::Type::Bool,
                );

                Ok(vec![
                    ir::Stmt::Let {
                        name: loop_var,
                        ty: iter_ty,
                        init: Some(start),
                    },
                    ir::Stmt::While { cond, body },
                ])
            }
            ast::Stmt::UnsafeBlock(b) => {
                let body = self.lower_block(b)?;
                Ok(vec![ir::Stmt::Unsafe(body)])
            }
            ast::Stmt::Loop(b) => {
                let cond =
                    ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Bool(true)), ir::Type::Bool);
                let body = self.lower_block(b)?;
                Ok(vec![ir::Stmt::While { cond, body }])
            }
            ast::Stmt::Break => Ok(vec![ir::Stmt::Break]),
            ast::Stmt::Continue => Ok(vec![ir::Stmt::Continue]),
            ast::Stmt::Match(m) => self.lower_match_stmt(m),
        }
    }

    pub(super) fn lower_let(
        &mut self,
        pattern: &ast::Pattern,
        declared: Option<&ast::TypeExpr>,
        value: &Option<ast::Expr>,
        _mutable: bool,
    ) -> Result<Vec<ir::Stmt>> {
        match pattern {
            ast::Pattern::Ident(name) => {
                let ty =
                    declared.map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::I64);
                let init = value.as_ref().map(|e| self.lower_expr(e)).transpose()?;
                self.vars.insert(name.clone(), ty.clone());
                self.lower_binding(name, declared, ty, init)
            }
            ast::Pattern::Tuple(pats) => {
                self.lower_tuple_destructure(pats, declared, value)
            }
            ast::Pattern::Wildcard => {
                if let Some(v) = value {
                    Ok(vec![ir::Stmt::Expr(self.lower_expr(v)?)])
                } else {
                    Ok(vec![])
                }
            }
            ast::Pattern::Literal(_) => {
                bail!("literal patterns in let bindings are not supported")
            }
        }
    }

    fn lower_tuple_destructure(
        &mut self,
        pats: &[ast::Pattern],
        declared: Option<&ast::TypeExpr>,
        value: &Option<ast::Expr>,
    ) -> Result<Vec<ir::Stmt>> {
        let init = value
            .as_ref()
            .map(|e| self.lower_expr(e))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("tuple destructuring requires an initializer"))?;

        // The lowered tuple value is a pointer to a synthetic struct.
        // If the initializer produced a Block (e.g. a nested tuple literal),
        // inline those statements and use the block's result as the pointer.
        let mut stmts = Vec::new();
        let tuple_ptr = if let ir::ExprKind::Block(blocks, result) = &init.kind {
            stmts.extend(blocks.clone());
            result.as_ref().clone()
        } else {
            let tmp = self.fresh_temp("$tuple");
            let ptr_ty = init.ty.clone();
            stmts.push(ir::Stmt::Let {
                name: tmp.clone(),
                ty: ptr_ty.clone(),
                init: Some(init.clone()),
            });
            ir::Expr::new(ir::ExprKind::Var(tmp), ptr_ty)
        };

        let sub_declared = declared.and_then(|d| {
            if let ast::TypeExpr::Tuple(tys) = d {
                Some(tys)
            } else {
                None
            }
        });

        self.destructure_tuple(&tuple_ptr, pats, sub_declared, &mut stmts)?;
        Ok(stmts)
    }

    /// Recursively desctructure a tuple pointer into the given patterns.
    /// `sub_declared` is the type-level tuple corresponding to `pats` (if any).
    fn destructure_tuple(
        &mut self,
        ptr: &ir::Expr,
        pats: &[ast::Pattern],
        sub_declared: Option<&Vec<ast::TypeExpr>>,
        stmts: &mut Vec<ir::Stmt>,
    ) -> Result<()> {
        let struct_name = match &ptr.ty {
            ir::Type::Ptr(inner) => match inner.as_ref() {
                ir::Type::Struct(n) => n.clone(),
                _ => bail!("cannot destructure non-tuple value"),
            },
            _ => bail!("cannot destructure non-tuple value"),
        };

        let struct_def = self
            .structs
            .get(&struct_name)
            .ok_or_else(|| anyhow::anyhow!("unknown tuple struct: {}", struct_name))?
            .clone();

        if pats.len() != struct_def.fields.len() {
            bail!(
                "tuple pattern has {} elements, but tuple has {}",
                pats.len(),
                struct_def.fields.len()
            );
        }

        for (i, pat) in pats.iter().enumerate() {
            let field_ty = struct_def.fields[i].1.clone();

            let gep = ir::Expr::new(
                ir::ExprKind::Gep {
                    base: Box::new(ptr.clone()),
                    field: i,
                },
                ir::Type::Ptr(Box::new(field_ty.clone())),
            );
            let load = ir::Expr::new(ir::ExprKind::Load(Box::new(gep)), field_ty.clone());

            match pat {
                ast::Pattern::Ident(name) => {
                    let ty = sub_declared
                        .and_then(|tys| tys.get(i))
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or_else(|| field_ty.clone());
                    self.vars.insert(name.clone(), ty.clone());
                    stmts.push(ir::Stmt::Let {
                        name: name.clone(),
                        ty,
                        init: Some(load),
                    });
                }
                ast::Pattern::Tuple(sub_pats) => {
                    // The field value is itself a tuple pointer; recurse.
                    let sub_decl = sub_declared.and_then(|tys| tys.get(i)).and_then(|t| {
                        if let ast::TypeExpr::Tuple(ts) = t {
                            Some(ts)
                        } else {
                            None
                        }
                    });

                    // If the loaded value is a Block (nested tuple literal),
                    // inline the block statements and use the result pointer.
                    let field_ptr = if let ir::ExprKind::Block(blocks, result) = &load.kind {
                        stmts.extend(blocks.clone());
                        result.as_ref().clone()
                    } else {
                        load
                    };

                    self.destructure_tuple(&field_ptr, sub_pats, sub_decl, stmts)?;
                }
                ast::Pattern::Wildcard => {
                    stmts.push(ir::Stmt::Expr(load));
                }
                ast::Pattern::Literal(_) => {
                    bail!("literal patterns in tuple destructuring are not supported");
                }
            }
        }

        Ok(())
    }

    /// Lower a `let`/`var` binding, emitting a `StackAlloc` when the declared
    /// type is an array so the backend allocates backing storage for it.  An
    /// array initializer is evaluated into a temporary pointer and then copied
    /// element-by-element into the variable's own storage, so that the variable
    /// never aliases a callee's stack frame.
    pub(super) fn lower_binding(
        &mut self,
        name: &str,
        declared: Option<&ast::TypeExpr>,
        ty: ir::Type,
        init: Option<ir::Expr>,
    ) -> Result<Vec<ir::Stmt>> {
        // If the initializer is a struct literal, inline its block and bind
        // the resulting pointer to the variable.
        if let Some(ir::Expr {
            kind: ir::ExprKind::Block(stmts, result),
            ..
        }) = init
        {
            let mut stmts = stmts.clone();
            let var_ty = result.ty.clone();
            stmts.push(ir::Stmt::Let {
                name: name.to_string(),
                ty: var_ty.clone(),
                init: Some(*result.clone()),
            });
            self.vars.insert(name.to_string(), var_ty);
            return Ok(stmts);
        }
        if let Some(ast::TypeExpr::Array(elem, size)) = declared {
            let count = match size.as_ref() {
                ast::Expr::Literal(ast::Literal::Int(n)) => *n as usize,
                _ => bail!("array size must be an integer constant"),
            };
            let elem_ty = self.lower_type(elem)?;
            let mut stmts = vec![ir::Stmt::StackAlloc {
                name: name.to_string(),
                elem_ty: elem_ty.clone(),
                count,
            }];
            if let Some(init) = init {
                // temp = init; i = 0; while i < count: name[i] = temp[i]; i++
                let tmp = self.fresh_temp(name);
                let idx = self.fresh_temp(name);
                stmts.push(ir::Stmt::Let {
                    name: tmp.clone(),
                    ty: ty.clone(),
                    init: Some(init),
                });
                stmts.push(ir::Stmt::Let {
                    name: idx.clone(),
                    ty: ir::Type::I64,
                    init: Some(ir::Expr::new(
                        ir::ExprKind::Lit(ir::Literal::Int(0)),
                        ir::Type::I64,
                    )),
                });
                let var_tmp = ir::Expr::new(ir::ExprKind::Var(tmp.clone()), ty.clone());
                let var_idx = ir::Expr::new(ir::ExprKind::Var(idx.clone()), ir::Type::I64);
                let var_name = ir::Expr::new(ir::ExprKind::Var(name.to_string()), ty.clone());
                let src = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::Add,
                        left: Box::new(var_tmp),
                        right: Box::new(var_idx.clone()),
                    },
                    ty.clone(),
                );
                let dst = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::Add,
                        left: Box::new(var_name),
                        right: Box::new(var_idx.clone()),
                    },
                    ty.clone(),
                );
                let cond = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::Lt,
                        left: Box::new(var_idx.clone()),
                        right: Box::new(ir::Expr::new(
                            ir::ExprKind::Lit(ir::Literal::Int(count as i64)),
                            ir::Type::I64,
                        )),
                    },
                    ir::Type::Bool,
                );
                let one = ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(1)), ir::Type::I64);
                let step = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: ir::BinOp::Add,
                        left: Box::new(var_idx.clone()),
                        right: Box::new(one),
                    },
                    ir::Type::I64,
                );
                stmts.push(ir::Stmt::While {
                    cond,
                    body: vec![
                        ir::Stmt::Assign {
                            lhs: ir::LValue::Deref(dst),
                            rhs: ir::Expr::new(ir::ExprKind::Load(Box::new(src)), elem_ty.clone()),
                        },
                        ir::Stmt::Assign {
                            lhs: ir::LValue::Var(idx),
                            rhs: step,
                        },
                    ],
                });
            }
            return Ok(stmts);
        }
        Ok(vec![ir::Stmt::Let {
            name: name.to_string(),
            ty,
            init,
        }])
    }

    pub(super) fn lower_range(&mut self, expr: &ast::Expr) -> Result<(ir::Expr, ir::Expr, bool)> {
        match expr {
            ast::Expr::Range(r) => {
                let start = r
                    .start
                    .as_ref()
                    .map(|e| self.lower_expr(e))
                    .transpose()?
                    .unwrap_or_else(|| {
                        ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(0)), ir::Type::I64)
                    });
                let end = r
                    .end
                    .as_ref()
                    .map(|e| self.lower_expr(e))
                    .transpose()?
                    .ok_or_else(|| anyhow::anyhow!("range must have an end"))?;
                Ok((start, end, r.inclusive))
            }
            _ => bail!("for-loop iterator must be a range expression"),
        }
    }

    pub(super) fn lower_match_stmt(&mut self, m: &ast::MatchStmt) -> Result<Vec<ir::Stmt>> {
        let cond = self.lower_expr(&m.scrutinee)?;
        let mut cases = Vec::new();
        let mut default = None;
        for case in &m.cases {
            let body = self.lower_block(&case.body)?;
            match &case.pattern {
                Pattern::Wildcard => default = Some(body),
                Pattern::Literal(lit) => cases.push((self.lower_literal(lit)?, body)),
                Pattern::Ident(name) => {
                    // A bare identifier in a pattern acts like a wildcard binding,
                    // which we treat as the default case for the first milestone.
                    let _ = name;
                    default = Some(body);
                }
                Pattern::Tuple(_) => bail!("tuple patterns are not supported in match lowering"),
            }
        }
        self.desugar_match_to_if(cond, cases, default)
    }

    pub(super) fn lower_match_expr(&mut self, m: &ast::MatchExpr) -> Result<ir::Expr> {
        let cond = self.lower_expr(&m.scrutinee)?;
        let result_ty = self.infer_match_result_ty(m)?;
        let tmp = self.fresh_temp("match");
        self.vars.insert(tmp.clone(), result_ty.clone());

        let mut cases = Vec::new();
        let mut default = None;
        for case in &m.cases {
            let (prefix, value) = self.lower_match_case_value(&case.body)?;
            let body = {
                let mut b = prefix;
                b.push(ir::Stmt::Assign {
                    lhs: ir::LValue::Var(tmp.clone()),
                    rhs: value,
                });
                b
            };
            match &case.pattern {
                Pattern::Wildcard => default = Some(body),
                Pattern::Literal(lit) => cases.push((self.lower_literal(lit)?, body)),
                Pattern::Ident(name) => {
                    let _ = name;
                    default = Some(body);
                }
                Pattern::Tuple(_) => bail!("tuple patterns are not supported in match lowering"),
            }
        }

        let init = ir::Stmt::Let {
            name: tmp.clone(),
            ty: result_ty.clone(),
            init: None,
        };
        let mut chain = self.desugar_match_to_if(cond, cases, default)?;
        let mut body = vec![init];
        body.append(&mut chain);
        Ok(ir::Expr::new(
            ir::ExprKind::Block(
                body,
                Box::new(ir::Expr::new(ir::ExprKind::Var(tmp), result_ty.clone())),
            ),
            result_ty,
        ))
    }

    pub(super) fn lower_match_case_value(
        &mut self,
        block: &ast::Block,
    ) -> Result<(Vec<ir::Stmt>, ir::Expr)> {
        if block.stmts.is_empty() {
            bail!("match case body is empty");
        }
        let (last, prefix) = block.stmts.split_last().unwrap();
        let value = match last {
            ast::Stmt::Expr(e) => self.lower_expr(e)?,
            _ => bail!("last statement of a match case body must be an expression"),
        };
        let mut stmts = Vec::new();
        for s in prefix {
            stmts.extend(self.lower_stmt(s)?);
        }
        Ok((stmts, value))
    }

    pub(super) fn infer_match_result_ty(&mut self, m: &ast::MatchExpr) -> Result<ir::Type> {
        let mut ty: Option<ir::Type> = None;
        for case in &m.cases {
            let last = case
                .body
                .stmts
                .last()
                .ok_or_else(|| anyhow::anyhow!("match case body is empty"))?;
            let candidate = match last {
                ast::Stmt::Expr(e) => self.lower_expr(e)?.ty,
                _ => bail!("last statement of a match case body must be an expression"),
            };
            if ty.is_none() {
                ty = Some(candidate);
            }
        }
        ty.ok_or_else(|| anyhow::anyhow!("cannot infer match result type"))
    }
    pub(super) fn desugar_match_to_if(
        &self,
        cond: ir::Expr,
        cases: Vec<(ir::Literal, Vec<ir::Stmt>)>,
        default: Option<Vec<ir::Stmt>>,
    ) -> Result<Vec<ir::Stmt>> {
        let mut chain: Option<Vec<ir::Stmt>> = default;
        for (lit, body) in cases.into_iter().rev() {
            let test = ir::Expr::new(
                ir::ExprKind::Bin {
                    op: ir::BinOp::Eq,
                    left: Box::new(cond.clone()),
                    right: Box::new(ir::Expr::new(ir::ExprKind::Lit(lit), cond.ty.clone())),
                },
                ir::Type::Bool,
            );
            chain = Some(vec![ir::Stmt::If {
                cond: test,
                then: body,
                else_: chain,
            }]);
        }
        Ok(chain.unwrap_or_default())
    }
}
