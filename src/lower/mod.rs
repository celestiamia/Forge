//! Lower the Python-like Forge AST to the native backend IR.
//!
//! This is intentionally minimal: it resolves names and types, checks basic
//! shape, and emits the backend IR consumed by `backend::codegen`.  Full
//! semantic analysis lives in `sema`; the lowerer is a pragmatic bridge for the
//! first milestone.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};

use crate::ast;
use crate::ast::Pattern;
use crate::backend::ir;

/// Lower an AST module to a backend program.
pub fn lower(module: &ast::Module, hosted: bool) -> Result<ir::Program> {
    let mut ctx = LowerCtx::new(module, hosted);
    ctx.lower_module(module)
}

fn enum_struct_name(enum_name: &str) -> String {
    format!("__enum_{}", enum_name)
}

struct LowerCtx<'a> {
    #[allow(dead_code)]
    module: &'a ast::Module,
    hosted: bool,
    structs: HashMap<String, ir::StructDef>,
    enums: HashMap<String, ir::EnumDef>,
    funcs: HashMap<String, (Vec<ir::Type>, ir::Type)>,
    externs: HashMap<String, (Vec<ir::Type>, ir::Type)>,
    vars: HashMap<String, ir::Type>,
    global_vars: HashSet<String>,
}

mod expr;
mod stmt;

impl<'a> LowerCtx<'a> {
    fn new(module: &'a ast::Module, hosted: bool) -> Self {
        Self {
            module,
            hosted,
            structs: HashMap::new(),
            enums: HashMap::new(),
            funcs: HashMap::new(),
            externs: HashMap::new(),
            vars: HashMap::new(),
            global_vars: HashSet::new(),
        }
    }

    fn lower_module(&mut self, module: &ast::Module) -> Result<ir::Program> {
        self.collect_signatures(module)?;
        self.lower_function_bodies(module)
    }

    fn collect_signatures(&mut self, module: &ast::Module) -> Result<()> {
        // Structs first so function signatures can reference any struct
        // regardless of item order in the merged module.
        for item in &module.items {
            if let ast::Item::Struct(s) = item {
                if !s.generics.is_empty() {
                    bail!("generic struct `{}` is not supported yet", s.name);
                }
                // Register an empty skeleton first so a struct can reference
                // itself by value; the cycle is then rejected cleanly during
                // codegen layout instead of surfacing as an unknown type.
                self.structs.insert(
                    s.name.clone(),
                    ir::StructDef {
                        name: s.name.clone(),
                        fields: Vec::new(),
                    },
                );
                let fields = s
                    .fields
                    .iter()
                    .map(|f| Ok((f.name.clone(), self.lower_type(&f.ty)?)))
                    .collect::<Result<Vec<_>>>()?;
                self.structs.insert(
                    s.name.clone(),
                    ir::StructDef {
                        name: s.name.clone(),
                        fields,
                    },
                );
            }
        }
        // Register enums, creating synthetic struct layouts for each. Enums are
        // collected before function signatures below so that enum types can
        // appear in parameter/return annotations (mirroring struct registration).
        for item in &module.items {
            if let ast::Item::Enum(e) = item {
                if !e.generics.is_empty() {
                    bail!("generic enum `{}` is not supported yet", e.name);
                }
                let struct_name = enum_struct_name(&e.name);
                let payload_ty = e.variants.iter().filter_map(|v| v.payload.as_ref()).next();
                let has_payload = payload_ty.is_some();
                let mut fields = Vec::new();
                fields.push(("tag".to_string(), ir::Type::I32));
                if has_payload {
                    let payload_ir = self.lower_type(payload_ty.unwrap())?;
                    fields.push(("payload".to_string(), payload_ir));
                }
                self.structs.insert(
                    struct_name.clone(),
                    ir::StructDef {
                        name: struct_name.clone(),
                        fields: fields.clone(),
                    },
                );
                let variants: Vec<ir::EnumVariant> = e
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| ir::EnumVariant {
                        name: v.name.clone(),
                        discriminant: i as i64,
                        payload: v
                            .payload
                            .as_ref()
                            .map(|t| self.lower_type(t))
                            .transpose()
                            .ok()
                            .flatten(),
                    })
                    .collect();
                self.enums.insert(
                    e.name.clone(),
                    ir::EnumDef {
                        name: e.name.clone(),
                        variants,
                    },
                );
                self.vars
                    .insert(e.name.clone(), ir::Type::Struct(struct_name.clone()));
            }
        }
        for item in &module.items {
            match item {
                ast::Item::Struct(_) => {}
                ast::Item::Function(f) => {
                    if !f.generics.is_empty() {
                        bail!("generic function `{}` is not supported yet", f.name);
                    }
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
                    let name =
                        if self.hosted && f.name == "main" && f.vis == ast::Visibility::Public {
                            "_forge_main".to_string()
                        } else {
                            f.name.clone()
                        };
                    self.funcs
                        .insert(name, (params.iter().map(|(_, t)| t.clone()).collect(), ret));
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
                ast::Item::Const(c) => {
                    let ty =
                        c.ty.as_ref()
                            .map(|t| self.lower_type(t))
                            .transpose()?
                            .unwrap_or(ir::Type::I64);
                    self.vars.insert(c.name.clone(), ty);
                    self.global_vars.insert(c.name.clone());
                }
                ast::Item::Embed(e) => {
                    let ptr_ty = ir::Type::Ptr(Box::new(ir::Type::U8));
                    self.vars.insert(e.name.clone(), ptr_ty);
                    let len_name = format!("{}_LEN", e.name);
                    self.vars.insert(len_name.clone(), ir::Type::I64);
                    self.global_vars.insert(e.name.clone());
                    self.global_vars.insert(len_name);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn lower_function_bodies(&mut self, module: &ast::Module) -> Result<ir::Program> {
        let mut funcs = Vec::new();
        let mut globals = Vec::new();
        let mut externs = Vec::new();

        for item in &module.items {
            match item {
                ast::Item::Function(f) => {
                    let name =
                        if self.hosted && f.name == "main" && f.vis == ast::Visibility::Public {
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

                    // Drop the previous function's locals but keep module-level
                    // globals (consts, embeds) so they can be referenced inside
                    // function bodies.
                    self.vars.retain(|k, _| self.global_vars.contains(k));
                    for (n, t) in &params {
                        self.vars.insert(n.clone(), t.clone());
                    }

                    let body = if let Some(ref b) = f.body {
                        self.lower_block(b)?
                    } else {
                        Vec::new()
                    };
                    funcs.push(ir::Func {
                        name,
                        params,
                        ret,
                        body,
                    });
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
                    let ty =
                        c.ty.as_ref()
                            .map(|t| self.lower_type(t))
                            .transpose()?
                            .unwrap_or(ir::Type::I64);
                    let init_expr = self.lower_expr(&c.value)?;
                    let init = match &init_expr.kind {
                        // Negative literals lower to `0 - <literal>`; fold them
                        // back into a negative literal for const initializers.
                        ir::ExprKind::Bin {
                            op: ir::BinOp::Sub,
                            left,
                            right,
                        } if matches!(left.kind, ir::ExprKind::Lit(ir::Literal::Int(0))) => {
                            match &right.kind {
                                ir::ExprKind::Lit(ir::Literal::Int(n)) => ir::Literal::Int(-n),
                                _ => bail!("const initializer must be a literal"),
                            }
                        }
                        _ => expr_to_literal(&init_expr)?,
                    };
                    globals.push(ir::Global {
                        name: c.name.clone(),
                        ty: ty.clone(),
                        init,
                    });
                }
                ast::Item::Embed(e) => {
                    // `embed NAME = "file"` emits two globals: `NAME` (a
                    // pointer slot patched to the raw bytes in .rodata) and
                    // the implicit `NAME_LEN` length constant.
                    let ptr_ty = ir::Type::Ptr(Box::new(ir::Type::U8));
                    globals.push(ir::Global {
                        name: e.name.clone(),
                        ty: ptr_ty.clone(),
                        init: ir::Literal::Bytes(e.data.clone()),
                    });
                    let len_name = format!("{}_LEN", e.name);
                    globals.push(ir::Global {
                        name: len_name.clone(),
                        ty: ir::Type::I64,
                        init: ir::Literal::Int(e.data.len() as i64),
                    });
                }
                _ => {}
            }
        }

        let structs: Vec<ir::StructDef> = self.structs.values().cloned().collect();
        let enums: Vec<ir::EnumDef> = self.enums.values().cloned().collect();

        Ok(ir::Program {
            name: module.package.clone(),
            structs,
            enums,
            globals,
            externs,
            funcs,
            hosted: self.hosted,
            target: None,
            arch: None,
            obj_format: None,
            config: None,
        })
    }

    fn lower_type(&mut self, ty: &ast::TypeExpr) -> Result<ir::Type> {
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
                "i128" | "int128" | "u128" | "uint128" => {
                    bail!("128-bit integers are not supported by any backend yet")
                }
                "f32" | "float32" => Ok(ir::Type::F32),
                "f64" | "float64" | "float" => Ok(ir::Type::F64),
                "char" => Ok(ir::Type::Char),
                "usize" => Ok(ir::Type::U64),
                "isize" => Ok(ir::Type::I64),
                other => {
                    if self.structs.contains_key(other) {
                        Ok(ir::Type::Struct(other.to_string()))
                    } else if self.enums.contains_key(other) {
                        let struct_name = enum_struct_name(other);
                        Ok(ir::Type::Struct(struct_name))
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
            ast::TypeExpr::Tuple(elems) => {
                let inner_types: Vec<ir::Type> = elems
                    .iter()
                    .map(|t| self.lower_type(t))
                    .collect::<Result<Vec<_>>>()?;
                let name = self.ensure_tuple_struct(&inner_types);
                Ok(ir::Type::Struct(name))
            }
            ast::TypeExpr::Function { .. } => {
                bail!("function types are not supported in the first milestone")
            }
        }
    }

    fn ensure_tuple_struct(&mut self, elem_types: &[ir::Type]) -> String {
        let name = format!(
            "__tuple_{}",
            elem_types
                .iter()
                .map(|t| format!("{:?}", t))
                .collect::<Vec<_>>()
                .join("_")
        );
        if !self.structs.contains_key(&name) {
            let fields: Vec<(String, ir::Type)> = elem_types
                .iter()
                .enumerate()
                .map(|(i, t)| (i.to_string(), t.clone()))
                .collect();
            self.structs.insert(
                name.clone(),
                ir::StructDef {
                    name: name.clone(),
                    fields,
                },
            );
        }
        name
    }

    fn lower_tuple_expr(&mut self, elems: &[ast::Expr]) -> Result<ir::Expr> {
        let mut elem_types = Vec::new();
        let mut lowered_elems = Vec::new();
        for elem in elems {
            let lowered = self.lower_expr(elem)?;
            elem_types.push(lowered.ty.clone());
            lowered_elems.push(lowered);
        }
        let name = self.ensure_tuple_struct(&elem_types);
        let struct_def = self.structs.get(&name).unwrap().clone();
        let ptr_ty = ir::Type::Ptr(Box::new(ir::Type::Struct(name.clone())));

        let total_size: usize = struct_def
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

        let slot_name = self.fresh_temp("$tuple");
        let mut stmts = vec![ir::Stmt::StackAlloc {
            name: slot_name.clone(),
            elem_ty: ir::Type::I64,
            count,
        }];

        for (i, value) in lowered_elems.iter().enumerate() {
            let var_expr = ir::Expr::new(ir::ExprKind::Var(slot_name.clone()), ptr_ty.clone());
            let gep = ir::Expr::new(
                ir::ExprKind::Gep {
                    base: Box::new(var_expr),
                    field: i,
                },
                ir::Type::Ptr(Box::new(elem_types[i].clone())),
            );
            if let ir::ExprKind::Block(pre, result) = &value.kind {
                for pre_stmt in pre {
                    stmts.push(pre_stmt.clone());
                }
                stmts.push(ir::Stmt::Assign {
                    lhs: ir::LValue::Deref(gep),
                    rhs: result.as_ref().clone(),
                });
            } else {
                stmts.push(ir::Stmt::Assign {
                    lhs: ir::LValue::Deref(gep),
                    rhs: value.clone(),
                });
            }
        }

        let result_expr = ir::Expr::new(ir::ExprKind::Var(slot_name), ptr_ty.clone());
        Ok(ir::Expr::new(
            ir::ExprKind::Block(stmts, Box::new(result_expr)),
            ptr_ty,
        ))
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
        let struct_def = self
            .structs
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?
            .clone();
        let ptr_ty = ir::Type::Ptr(Box::new(ir::Type::Struct(name.to_string())));

        // Calculate struct size (sum of field sizes, rounded up to 8)
        let total_size: usize = struct_def
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
            let idx = struct_def
                .fields
                .iter()
                .position(|(n, _)| n == fname)
                .ok_or_else(|| anyhow::anyhow!("unknown field {}.{}", name, fname))?;

            let var_expr = ir::Expr::new(ir::ExprKind::Var(slot_name.clone()), ptr_ty.clone());
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
        let result_expr = ir::Expr::new(ir::ExprKind::Var(slot_name), ptr_ty.clone());
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
