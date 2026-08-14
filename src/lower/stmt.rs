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
                let ty =
                    l.ty.as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::I64);
                let init = l.value.as_ref().map(|e| self.lower_expr(e)).transpose()?;
                self.vars.insert(l.name.clone(), ty.clone());
                self.lower_binding(&l.name, l.ty.as_ref(), ty, init)
            }
            ast::Stmt::Var(v) => {
                let ty =
                    v.ty.as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::I64);
                let init = v.value.as_ref().map(|e| self.lower_expr(e)).transpose()?;
                self.vars.insert(v.name.clone(), ty.clone());
                self.lower_binding(&v.name, v.ty.as_ref(), ty, init)
            }
            ast::Stmt::Assign(a) => {
                let lhs = self.lower_lvalue(&a.target)?;
                let rhs = self.lower_expr(&a.value)?;
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
