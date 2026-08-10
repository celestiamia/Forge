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

mod expr;
mod stmt;

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
        self.collect_signatures(module);
        self.lower_function_bodies(module)
    }

    fn collect_signatures(&mut self, module: &ast::Module) {
        for item in &module.items {
            match item {
                ast::Item::Struct(s) => {
                    let fields = s
                        .fields
                        .iter()
                        .map(|f| Ok((f.name.clone(), self.lower_type(&f.ty)?)))
                        .collect::<Result<Vec<_>>>()
                        .expect("struct field type resolution");
                    self.structs.insert(s.name.clone(), ir::StructDef { name: s.name.clone(), fields });
                }
                ast::Item::Function(f) => {
                    let params = f
                        .params
                        .iter()
                        .map(|p| Ok((p.name.clone(), self.lower_type(&p.ty)?)))
                        .collect::<Result<Vec<_>>>()
                        .expect("function param type resolution");
                    let ret = f
                        .ret
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()
                        .expect("function return type resolution")
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
                        .collect::<Result<Vec<_>>>()
                        .expect("extern function param type resolution");
                    let ret = e
                        .ret
                        .as_ref()
                        .map(|t| self.lower_type(t))
                        .transpose()
                        .expect("extern function return type resolution")
                        .unwrap_or(ir::Type::Void);
                    self.externs.insert(e.name.clone(), (params, ret));
                }
                _ => {}
            }
        }
    }

    fn lower_function_bodies(&mut self, module: &ast::Module) -> Result<ir::Program> {
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
                    globals.push(ir::Global { name: c.name.clone(), ty: ty.clone(), init });
                    // Add to vars so the constant can be referenced in expressions
                    self.vars.insert(c.name.clone(), ty);
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
                "i8" | "int8" => Ok(ir::Type::I8),
                "i16" | "int16" => Ok(ir::Type::I16),
                "i32" | "int32" | "int" => Ok(ir::Type::I32),
                "i64" | "int64" => Ok(ir::Type::I64),
                "u8" | "uint8" | "byte" => Ok(ir::Type::U8),
                "u16" | "uint16" => Ok(ir::Type::U16),
                "u32" | "uint32" | "uint" => Ok(ir::Type::U32),
                "u64" | "uint64" => Ok(ir::Type::U64),
                "f32" | "float32" => Ok(ir::Type::F32),
                "f64" | "float64" | "float" => Ok(ir::Type::F64),
                "char" => Ok(ir::Type::Char),
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
            ast::TypeExpr::Slice(inner) => {
                let elem = self.lower_type(inner)?;
                Ok(ir::Type::Slice(Box::new(elem)))
            }
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

    fn fresh_temp(&mut self, prefix: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{}${}", prefix, n)
    }

    fn infer_expr_type(&self, expr: &ir::Expr) -> Result<ir::Type> {
        Ok(expr.ty.clone())
    }

    fn lower_struct_literal(
        &mut self,
        name: &str,
        fields: &Vec<(String, ast::Expr)>,
    ) -> Result<ir::Expr> {
        let struct_def = self.structs.get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?
            .clone();
        let ptr_ty = ir::Type::Ptr(Box::new(ir::Type::Struct(name.to_string())));

        // Calculate struct size (sum of field sizes, rounded up to 8)
        let total_size: usize = struct_def.fields.iter().map(|(_, ty)| {
            match ty {
                ir::Type::I8 | ir::Type::U8 | ir::Type::Char | ir::Type::Bool => 1,
                ir::Type::I16 | ir::Type::U16 => 2,
                ir::Type::I32 | ir::Type::U32 | ir::Type::F32 => 4,
                _ => 8,
            }
        }).sum();
        let count = ((total_size + 7) / 8).max(1);

        // Allocate stack space for the struct
        let slot_name = self.fresh_temp("$struct");
        let mut stmts = vec![ir::Stmt::StackAlloc {
            name: slot_name.clone(),
            elem_ty: ir::Type::I64,
            count,
        }];

        // Store each field at its offset
        for (fname, expr) in fields {
            let value = self.lower_expr(expr)?;
            let idx = struct_def.fields.iter().position(|(n, _)| n == fname)
                .ok_or_else(|| anyhow::anyhow!("unknown field {}.{}", name, fname))?;

            let var_expr = ir::Expr::new(
                ir::ExprKind::Var(slot_name.clone()),
                ptr_ty.clone(),
            );
            let gep = ir::Expr::new(
                ir::ExprKind::Gep {
                    base: Box::new(var_expr),
                    field: idx,
                },
                ptr_ty.clone(),
            );
            stmts.push(ir::Stmt::Assign {
                lhs: ir::LValue::Deref(gep),
                rhs: value,
            });
        }

        // Return a block expression that yields the pointer to the struct
        let result_expr = ir::Expr::new(
            ir::ExprKind::Var(slot_name),
            ptr_ty.clone(),
        );
        Ok(ir::Expr::new(
            ir::ExprKind::Block(stmts, Box::new(result_expr)),
            ptr_ty,
        ))
    }
}

fn expr_to_literal(expr: &ir::Expr) -> Result<ir::Literal> {
    match &expr.kind {
        ir::ExprKind::Lit(lit) => Ok(lit.clone()),
        _ => bail!("const initializer must be a literal"),
    }
}
