//! Lower the Python-like Forge AST to the native backend IR.
//!
//! This is intentionally minimal: it resolves names and types, checks basic
//! shape, and emits the backend IR consumed by `backend::codegen`.  Full
//! semantic analysis lives in `sema`; the lowerer is a pragmatic bridge for the
//! first milestone.

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::ast;
use crate::ast::Pattern;
use crate::backend::ir;

/// Lower an AST module to a backend program.
pub fn lower(module: &ast::Module, hosted: bool) -> Result<ir::Program> {
    let mut ctx = LowerCtx::new(module, hosted);
    ctx.lower_module(module)
}

struct LowerCtx<'a> {
    module: &'a ast::Module,
    hosted: bool,
    structs: HashMap<String, ir::StructDef>,
    funcs: HashMap<String, (Vec<ir::Type>, ir::Type)>,
    externs: HashMap<String, (Vec<ir::Type>, ir::Type)>,
    vars: HashMap<String, ir::Type>,
}

impl<'a> LowerCtx<'a> {
    fn new(module: &'a ast::Module, hosted: bool) -> Self {
        Self {
            module,
            hosted,
            structs: HashMap::new(),
            funcs: HashMap::new(),
            externs: HashMap::new(),
            vars: HashMap::new(),
        }
    }

    fn lower_module(&mut self, module: &ast::Module) -> Result<ir::Program> {
        // First pass: collect struct and function signatures.
        for item in &module.items {
            match item {
                ast::Item::Struct(s) => {
                    let fields = s
                        .fields
                        .iter()
                        .map(|f| Ok((f.name.clone(), self.lower_type(&f.ty)?)))
                        .collect::<Result<Vec<_>>>()?;
                    self.structs.insert(s.name.clone(), ir::StructDef { name: s.name.clone(), fields });
                }
                ast::Item::Function(f) => {
                    let params = f
                        .params
                        .iter()
                        .map(|p| Ok((p.name.clone(), self.lower_type(&p.ty)?)))
                        .collect::<Result<Vec<_>>>()?;
                    let ret = f
                        .ret
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::Void);
                    let name = if self.hosted && f.name == "main" && f.vis == ast::Visibility::Public {
                        "_forge_main".to_string()
                    } else {
                        f.name.clone()
                    };
                    self.funcs.insert(name, (params.iter().map(|(_, t)| t.clone()).collect(), ret));
                }
                ast::Item::ExternFn(e) => {
                    let params = e
                        .params
                        .iter()
                        .map(|p| self.lower_type(&p.ty))
                        .collect::<Result<Vec<_>>>()?;
                    let ret = e
                        .ret
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::Void);
                    self.externs.insert(e.name.clone(), (params, ret));
                }
                _ => {}
            }
        }

        // Second pass: lower function bodies.
        let mut funcs = Vec::new();
        let mut globals = Vec::new();
        let mut externs = Vec::new();

        for item in &module.items {
            match item {
                ast::Item::Function(f) => {
                    let name = if self.hosted && f.name == "main" && f.vis == ast::Visibility::Public {
                        "_forge_main".to_string()
                    } else {
                        f.name.clone()
                    };
                    let params = f
                        .params
                        .iter()
                        .map(|p| Ok((p.name.clone(), self.lower_type(&p.ty)?)))
                        .collect::<Result<Vec<_>>>()?;
                    let ret = f
                        .ret
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::Void);

                    self.vars.clear();
                    for (n, t) in &params {
                        self.vars.insert(n.clone(), t.clone());
                    }

                    let body = if let Some(ref b) = f.body {
                        self.lower_block(b)?
                    } else {
                        Vec::new()
                    };
                    funcs.push(ir::Func { name, params, ret, body });
                }
                ast::Item::ExternFn(e) => {
                    // Extern declarations are recorded so that calls to them
                    // can be typed.  The hosted runtime emits the `_dev_*`
                    // helpers; user code should call them through the stdlib
                    // wrappers in `core/io.dev`.
                    let params = e
                        .params
                        .iter()
                        .map(|p| Ok((p.name.clone(), self.lower_type(&p.ty)?)))
                        .collect::<Result<Vec<_>>>()?;
                    let ret = e
                        .ret
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::Void);
                    externs.push(ir::ExternFunc {
                        name: e.name.clone(),
                        params,
                        ret,
                        varargs: false,
                    });
                }
                ast::Item::Const(c) => {
                    let ty = c
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()?
                        .unwrap_or(ir::Type::I64);
                    let init_expr = self.lower_expr(&c.value)?;
                    let init = expr_to_literal(&init_expr)?;
                    globals.push(ir::Global { name: c.name.clone(), ty, init });
                }
                _ => {}
            }
        }

        let structs: Vec<ir::StructDef> = self.structs.values().cloned().collect();

        Ok(ir::Program {
            name: module.package.clone(),
            structs,
            globals,
            externs,
            funcs,
            hosted: self.hosted,
            target: None,
            arch: None,
            obj_format: None,
        })
    }

    fn lower_type(&self, ty: &ast::TypeExpr) -> Result<ir::Type> {
        match ty {
            ast::TypeExpr::Name(n) => match n.as_str() {
                "void" => Ok(ir::Type::Void),
                "bool" => Ok(ir::Type::Bool),
                "int8" => Ok(ir::Type::I8),
                "int16" => Ok(ir::Type::I16),
                "int32" => Ok(ir::Type::I32),
                "int64" => Ok(ir::Type::I64),
                "uint8" => Ok(ir::Type::U8),
                "uint16" => Ok(ir::Type::U16),
                "uint32" => Ok(ir::Type::U32),
                "uint64" => Ok(ir::Type::U64),
                "float32" => Ok(ir::Type::F32),
                "float64" => Ok(ir::Type::F64),
                "char" => Ok(ir::Type::Char),
                "byte" => Ok(ir::Type::U8),
                "usize" => Ok(ir::Type::U64),
                "isize" => Ok(ir::Type::I64),
                other => {
                    if self.structs.contains_key(other) {
                        Ok(ir::Type::Struct(other.to_string()))
                    } else {
                        bail!("unknown type: {}", other)
                    }
                }
            },
            ast::TypeExpr::Pointer(inner)
            | ast::TypeExpr::Own(inner)
            | ast::TypeExpr::Ref(inner)
            | ast::TypeExpr::RefMut(inner) => Ok(ir::Type::Ptr(Box::new(self.lower_type(inner)?))),
            ast::TypeExpr::Slice(_) => bail!("slices are not supported in the first milestone"),
            ast::TypeExpr::Array(inner, size) => {
                let _count = match &size.as_ref() {
                    ast::Expr::Literal(ast::Literal::Int(n)) => *n as usize,
                    _ => bail!("array size must be an integer constant"),
                };
                let elem = self.lower_type(inner)?;
                // The backend IR does not model arrays as first-class types yet; represent them
                // as a pointer to the element type.  Stack allocation will use the count.
                Ok(ir::Type::Ptr(Box::new(elem)))
            }
            ast::TypeExpr::Tuple(_) => bail!("tuples are not supported in the first milestone"),
            ast::TypeExpr::Function { .. } => bail!("function types are not supported in the first milestone"),
        }
    }

    fn lower_block(&mut self, block: &ast::Block) -> Result<Vec<ir::Stmt>> {
        let mut stmts = Vec::new();
        for s in &block.stmts {
            stmts.extend(self.lower_stmt(s)?);
        }
        Ok(stmts)
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Result<Vec<ir::Stmt>> {
        match stmt {
            ast::Stmt::Let(l) => {
                let ty = l
                    .ty
                    .as_ref()
                    .map(|t| self.lower_type(t))
                    .transpose()?
                    .unwrap_or(ir::Type::I64);
                let init = l.value.as_ref().map(|e| self.lower_expr(e)).transpose()?;
                self.vars.insert(l.name.clone(), ty.clone());
                Ok(vec![ir::Stmt::Let { name: l.name.clone(), ty, init }])
            }
            ast::Stmt::Var(v) => {
                let ty = v
                    .ty
                    .as_ref()
                    .map(|t| self.lower_type(t))
                    .transpose()?
                    .unwrap_or(ir::Type::I64);
                let init = v.value.as_ref().map(|e| self.lower_expr(e)).transpose()?;
                self.vars.insert(v.name.clone(), ty.clone());
                Ok(vec![ir::Stmt::Let { name: v.name.clone(), ty, init }])
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
                            else_: if nested.is_empty() { None } else { Some(nested) },
                        }];
                    }
                    else_ = if nested.is_empty() { None } else { Some(nested) };
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
                            left: Box::new(ir::Expr::new(ir::ExprKind::Var(loop_var.clone()), iter_ty.clone())),
                            right: Box::new(ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(1)), iter_ty.clone())),
                        },
                        iter_ty.clone(),
                    ),
                });

                let cond_op = if inclusive { ir::BinOp::Le } else { ir::BinOp::Lt };
                let cond = ir::Expr::new(
                    ir::ExprKind::Bin {
                        op: cond_op,
                        left: Box::new(ir::Expr::new(ir::ExprKind::Var(loop_var.clone()), iter_ty.clone())),
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
                let cond = ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Bool(true)), ir::Type::Bool);
                let body = self.lower_block(b)?;
                Ok(vec![ir::Stmt::While { cond, body }])
            }
            ast::Stmt::Break => Ok(vec![ir::Stmt::Break]),
            ast::Stmt::Continue => Ok(vec![ir::Stmt::Continue]),
            ast::Stmt::Match(m) => self.lower_match_stmt(m),
        }
    }

    fn lower_range(&mut self, expr: &ast::Expr) -> Result<(ir::Expr, ir::Expr, bool)> {
        match expr {
            ast::Expr::Range(r) => {
                let start = r
                    .start
                    .as_ref()
                    .map(|e| self.lower_expr(e))
                    .transpose()?
                    .unwrap_or_else(|| ir::Expr::new(ir::ExprKind::Lit(ir::Literal::Int(0)), ir::Type::I64));
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

    fn lower_match_stmt(&mut self, m: &ast::MatchStmt) -> Result<Vec<ir::Stmt>> {
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

    fn lower_match_expr(&mut self, m: &ast::MatchExpr) -> Result<ir::Expr> {
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
            ir::ExprKind::Block(body, Box::new(ir::Expr::new(ir::ExprKind::Var(tmp), result_ty.clone()))),
            result_ty,
        ))
    }

    fn lower_match_case_value(&mut self, block: &ast::Block) -> Result<(Vec<ir::Stmt>, ir::Expr)> {
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

    fn infer_match_result_ty(&mut self, m: &ast::MatchExpr) -> Result<ir::Type> {
        let mut ty: Option<ir::Type> = None;
        for case in &m.cases {
            let last = case.body.stmts.last().ok_or_else(|| anyhow::anyhow!("match case body is empty"))?;
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

    fn fresh_temp(&mut self, prefix: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{}${}", prefix, n)
    }

    fn desugar_match_to_if(
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

    fn lower_lvalue(
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
            _ => bail!("invalid assignment target"),
        }
    }

    fn resolve_field(&self, ty: &ir::Type, field: &str) -> Result<(String, usize)> {
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

    fn lower_expr(&mut self, expr: &ast::Expr) -> Result<ir::Expr> {
        match expr {
            ast::Expr::Literal(l) => {
                let lit = self.lower_literal(l)?;
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
            ast::Expr::Ident(name) => {
                let ty = self.vars.get(name).cloned().unwrap_or(ir::Type::I64);
                Ok(ir::Expr::new(ir::ExprKind::Var(name.clone()), ty))
            }
            ast::Expr::Binary(b) => {
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
            ast::Expr::Unary(u) => match u.op {
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
            ast::Expr::Deref(d) => {
                let ptr = self.lower_expr(&d.expr)?;
                let ty = match &ptr.ty {
                    ir::Type::Ptr(inner) => *inner.clone(),
                    _ => bail!("dereference of non-pointer"),
                };
                Ok(ir::Expr::new(ir::ExprKind::Load(Box::new(ptr)), ty))
            }
            ast::Expr::Ref(r) => {
                let inner = self.lower_expr(&r.expr)?;
                let ty = ir::Type::Ptr(Box::new(inner.ty.clone()));
                Ok(ir::Expr::new(ir::ExprKind::AddrOf(Box::new(inner)), ty))
            },
            ast::Expr::Call(c) => {
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
            ast::Expr::Cast(c) => {
                let expr = self.lower_expr(&c.expr)?;
                let ty = self.lower_type(&c.ty)?;
                Ok(ir::Expr::new(ir::ExprKind::Cast { expr: Box::new(expr), ty: ty.clone() }, ty))
            }
            ast::Expr::Field(f) => {
                let base = self.lower_expr(&f.object)?;
                let (struct_name, idx) = self.resolve_field(&base.ty, &f.field)?;
                let struct_ty = ir::Type::Struct(struct_name.clone());
                let ptr_ty = ir::Type::Ptr(Box::new(struct_ty.clone()));
                // If the base is already a pointer to the struct, use it
                // directly; otherwise take its address.
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
            ast::Expr::Index(_) => bail!("indexing is not supported in the first milestone"),
            ast::Expr::SizeOf(_) => bail!("sizeof is not supported in the first milestone"),
            ast::Expr::OffsetOf(_) => bail!("offsetof is not supported in the first milestone"),
            ast::Expr::Asm(a) => {
                // Inline assembly is only meaningful in bare-metal / flat-binary
                // targets.  We preserve the template text verbatim; the backend
                // will either assemble it (16-bit boot mode) or report an error.
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
            ast::Expr::Match(m) => self.lower_match_expr(m),
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

    fn lower_literal(&self, lit: &ast::Literal) -> Result<ir::Literal> {
        match lit {
            ast::Literal::Int(i) => Ok(ir::Literal::Int(*i)),
            ast::Literal::Float(f) => Ok(ir::Literal::Float(*f)),
            ast::Literal::String(s) => Ok(ir::Literal::String(s.clone())),
            ast::Literal::Char(c) => Ok(ir::Literal::Char(*c as u8)),
            ast::Literal::Bool(b) => Ok(ir::Literal::Bool(*b)),
            ast::Literal::Null => Ok(ir::Literal::Null),
        }
    }

    fn lower_binop(&self, op: ast::BinOp) -> Result<ir::BinOp> {
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

    fn infer_expr_type(&self, expr: &ir::Expr) -> Result<ir::Type> {
        Ok(expr.ty.clone())
    }
}

fn expr_to_literal(expr: &ir::Expr) -> Result<ir::Literal> {
    match &expr.kind {
        ir::ExprKind::Lit(lit) => Ok(lit.clone()),
        _ => bail!("const initializer must be a literal"),
    }
}
