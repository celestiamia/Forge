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
use crate::sema::check::typing::substitute;
use crate::sema::typed::MonoInstance;
use crate::ty::{self, Type as TyType};

/// Lower an AST module to a backend program.
pub fn lower(module: &ast::Module, hosted: bool) -> Result<ir::Program> {
    let mut ctx = LowerCtx::new(module, hosted);
    ctx.lower_module(module)
}

fn enum_struct_name(enum_name: &str) -> String {
    format!("__enum_{}", enum_name)
}

/// A generic function definition with its parameter/return types kept as
/// type patterns (`T` stays a `Type::Generic` placeholder) so call sites can
/// infer concrete arguments.
#[derive(Debug, Clone)]
struct GenericDef {
    generics: Vec<String>,
    patterns: Vec<TyType>,
    ret: TyType,
    func: ast::Function,
}

/// A generic function instance discovered at a call site, waiting for its
/// body to be lowered with `generic_map` set.
struct PendingInstance {
    mangled: String,
    func: ast::Function,
    args: Vec<TyType>,
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
    generic_defs: HashMap<String, GenericDef>,
    /// Active substitution of generic parameters for the function instance
    /// currently being lowered (`T` -> concrete IR type).
    generic_map: HashMap<String, ir::Type>,
    pending_instances: Vec<PendingInstance>,
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
            generic_defs: HashMap::new(),
            generic_map: HashMap::new(),
            pending_instances: Vec::new(),
        }
    }

    fn lower_module(&mut self, module: &ast::Module) -> Result<ir::Program> {
        self.collect_signatures(module)?;
        self.lower_function_bodies(module)
    }

    fn collect_signatures(&mut self, module: &ast::Module) -> Result<()> {
        // Structs first so function signatures can reference any struct
        // regardless of item order in the merged module.  Generic structs are
        // skipped here; their monomorphized instances are registered lazily
        // when a concrete type application is lowered.
        for item in &module.items {
            if let ast::Item::Struct(s) = item {
                if !s.generics.is_empty() {
                    continue;
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
                        // Generic functions keep their parameter/return types
                        // as patterns; concrete instances are registered when
                        // a call site infers their type arguments.
                        let generics: HashSet<String> = f.generics.iter().cloned().collect();
                        let patterns = f
                            .params
                            .iter()
                            .map(|p| self.pattern_type(&p.ty, &generics))
                            .collect::<Result<Vec<_>>>()?;
                        let ret = f
                            .ret
                            .as_ref()
                            .map(|t| self.pattern_type(t, &generics))
                            .transpose()?
                            .unwrap_or(TyType::Void);
                        self.generic_defs.insert(
                            f.name.clone(),
                            GenericDef {
                                generics: f.generics.clone(),
                                patterns,
                                ret,
                                func: f.clone(),
                            },
                        );
                        continue;
                    }
                    let params = f
                        .params
                        .iter()
                        .map(|p| Ok((p.name.clone(), self.lower_param_type(&p.ty)?)))
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
                        .map(|p| self.lower_param_type(&p.ty))
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
                ast::Item::Function(f) if f.generics.is_empty() => {
                    let name =
                        if self.hosted && f.name == "main" && f.vis == ast::Visibility::Public {
                            "_forge_main".to_string()
                        } else {
                            f.name.clone()
                        };
                    let params = f
                        .params
                        .iter()
                        .map(|p| Ok((p.name.clone(), self.lower_param_type(&p.ty)?)))
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
                        .map(|p| Ok((p.name.clone(), self.lower_param_type(&p.ty)?)))
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

        // Emit monomorphized generic function instances.  Instance bodies can
        // discover further instances at their own call sites, so drain the
        // worklist until no new ones appear.
        while let Some(inst) = self.pending_instances.pop() {
            let func = self.lower_instance_body(inst)?;
            funcs.push(func);
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

    /// Lower a parameter type.  Struct-typed parameters (other than the
    /// synthetic `__enum_*` structs, whose slots already hold pointers) are
    /// passed by pointer: the caller passes the struct's address and the
    /// callee's parameter slot holds that address.
    fn lower_param_type(&mut self, ty: &ast::TypeExpr) -> Result<ir::Type> {
        let t = self.lower_type(ty)?;
        Ok(match t {
            ir::Type::Struct(name) if !name.starts_with("__enum_") => {
                ir::Type::Ptr(Box::new(ir::Type::Struct(name)))
            }
            other => other,
        })
    }

    /// Register the signature of a monomorphized generic function instance
    /// (computing its mangled name from the inferred type arguments) and queue
    /// its body for lowering.  Idempotent per (function, arguments) pair.
    fn register_instance(
        &mut self,
        name: &str,
        def: &GenericDef,
        args: Vec<TyType>,
    ) -> Result<String> {
        let mangled = MonoInstance::new(name, args.clone()).mangled_name;
        if self.funcs.contains_key(&mangled) {
            return Ok(mangled);
        }
        let mapping: HashMap<String, TyType> = def
            .generics
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();
        let params: Vec<ir::Type> = def
            .patterns
            .iter()
            .map(|p| {
                let t = sema_to_ir(&substitute(p, &mapping))?;
                Ok(match t {
                    ir::Type::Struct(n) if !n.starts_with("__enum_") => {
                        ir::Type::Ptr(Box::new(ir::Type::Struct(n)))
                    }
                    other => other,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let ret = sema_to_ir(&substitute(&def.ret, &mapping))?;
        self.funcs.insert(mangled.clone(), (params, ret));
        self.pending_instances.push(PendingInstance {
            mangled: mangled.clone(),
            func: def.func.clone(),
            args,
        });
        Ok(mangled)
    }

    /// Lower the body of a monomorphized generic function instance with the
    /// generic parameters bound to their concrete types.
    fn lower_instance_body(&mut self, inst: PendingInstance) -> Result<ir::Func> {
        let mapping: HashMap<String, TyType> = inst
            .func
            .generics
            .iter()
            .cloned()
            .zip(inst.args.iter().cloned())
            .collect();
        for (g, a) in &mapping {
            self.generic_map.insert(g.clone(), sema_to_ir(a)?);
        }
        let name = inst.mangled.clone();

        let params = inst
            .func
            .params
            .iter()
            .map(|p| Ok((p.name.clone(), self.lower_param_type(&p.ty)?)))
            .collect::<Result<Vec<_>>>()?;
        let ret = inst
            .func
            .ret
            .as_ref()
            .map(|t| self.lower_type(t))
            .transpose()?
            .unwrap_or(ir::Type::Void);

        self.vars.retain(|k, _| self.global_vars.contains(k));
        for (n, t) in &params {
            self.vars.insert(n.clone(), t.clone());
        }
        let body = if let Some(ref b) = inst.func.body {
            self.lower_block(b)?
        } else {
            Vec::new()
        };
        self.generic_map.clear();
        Ok(ir::Func {
            name,
            params,
            ret,
            body,
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
                    if let Some(t) = self.generic_map.get(other) {
                        Ok(t.clone())
                    } else if self.structs.contains_key(other) {
                        Ok(ir::Type::Struct(other.to_string()))
                    } else if self.enums.contains_key(other) {
                        let struct_name = enum_struct_name(other);
                        Ok(ir::Type::Struct(struct_name))
                    } else {
                        bail!("unknown type: {}", other)
                    }
                }
            },
            ast::TypeExpr::GenericApp { base, args } => {
                let arg_tys: Vec<ir::Type> = args
                    .iter()
                    .map(|t| self.lower_type(t))
                    .collect::<Result<Vec<_>>>()?;
                let sema_args: Vec<TyType> = arg_tys.iter().map(|t| self.ir_to_sema(t)).collect();
                let name = self.ensure_mono_struct(base, &sema_args)?;
                Ok(ir::Type::Struct(name))
            }
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

    /// Register the monomorphized definition of a generic struct instantiation
    /// (`Pair` with `[i64]` -> struct name `Pair$i64`) by lowering the base
    /// struct's field types with its generic parameters bound to the concrete
    /// arguments.  Idempotent per name.
    fn ensure_mono_struct(&mut self, base: &str, args: &[TyType]) -> Result<String> {
        let name = ty::mono_struct_name(base, args);
        if self.structs.contains_key(&name) {
            return Ok(name);
        }
        let Some(sdef) = self.module.items.iter().find_map(|it| match it {
            ast::Item::Struct(s) if s.name == base => Some(s),
            _ => None,
        }) else {
            bail!("unknown struct: {}", base);
        };
        if sdef.generics.is_empty() {
            bail!("struct `{}` is not generic", base);
        }
        if sdef.generics.len() != args.len() {
            bail!(
                "struct `{}` expects {} type arguments, got {}",
                base,
                sdef.generics.len(),
                args.len()
            );
        }
        let ir_args: Vec<ir::Type> = args.iter().map(sema_to_ir).collect::<Result<Vec<_>>>()?;
        let old_map = std::mem::take(&mut self.generic_map);
        for (g, a) in sdef.generics.iter().zip(ir_args.iter()) {
            self.generic_map.insert(g.clone(), a.clone());
        }
        let fields = sdef
            .fields
            .iter()
            .map(|f| Ok((f.name.clone(), self.lower_type(&f.ty)?)))
            .collect::<Result<Vec<_>>>();
        self.generic_map = old_map;
        self.structs.insert(
            name.clone(),
            ir::StructDef {
                name: name.clone(),
                fields: fields?,
            },
        );
        Ok(name)
    }

    /// Resolve a type expression into a sema `Type` "pattern" for generic
    /// function signatures: generic parameters stay `Type::Generic`, and
    /// generic struct applications stay `Type::StructApp` until concrete.
    fn pattern_type(&mut self, ty: &ast::TypeExpr, generics: &HashSet<String>) -> Result<TyType> {
        match ty {
            ast::TypeExpr::Name(n) => {
                if generics.contains(n) {
                    return Ok(TyType::Generic { name: n.clone() });
                }
                if let Some(t) = crate::sema::check::typing::primitive_type(n) {
                    return Ok(t);
                }
                if self.structs.contains_key(n) {
                    return Ok(TyType::Struct {
                        name: n.clone(),
                        fields: Vec::new(),
                    });
                }
                bail!("unknown type: {}", n)
            }
            ast::TypeExpr::Pointer(inner) => {
                Ok(TyType::pointer(self.pattern_type(inner, generics)?))
            }
            ast::TypeExpr::Own(inner) => Ok(TyType::own(self.pattern_type(inner, generics)?)),
            ast::TypeExpr::Ref(inner) => Ok(TyType::refr(self.pattern_type(inner, generics)?)),
            ast::TypeExpr::RefMut(inner) => {
                Ok(TyType::ref_mut(self.pattern_type(inner, generics)?))
            }
            ast::TypeExpr::Slice(inner) => Ok(TyType::slice(self.pattern_type(inner, generics)?)),
            ast::TypeExpr::Array(inner, size) => {
                let size = match size.as_ref() {
                    ast::Expr::Literal(ast::Literal::Int(n)) if *n >= 0 => *n as u64,
                    _ => bail!("array size must be a constant non-negative integer literal"),
                };
                Ok(TyType::array(self.pattern_type(inner, generics)?, size))
            }
            ast::TypeExpr::Tuple(elems) => Ok(TyType::tuple(
                elems
                    .iter()
                    .map(|t| self.pattern_type(t, generics))
                    .collect::<Result<Vec<_>>>()?,
            )),
            ast::TypeExpr::GenericApp { base, args } => {
                let args: Vec<TyType> = args
                    .iter()
                    .map(|t| self.pattern_type(t, generics))
                    .collect::<Result<Vec<_>>>()?;
                if args
                    .iter()
                    .any(|a| a.is_generic() || matches!(a, TyType::StructApp { .. }))
                {
                    Ok(TyType::StructApp {
                        base: base.clone(),
                        args,
                    })
                } else {
                    Ok(TyType::Struct {
                        name: ty::mono_struct_name(base, &args),
                        fields: Vec::new(),
                    })
                }
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

/// Convert an IR type to the sema `Type` used for generic inference and
/// mangling.  Struct-typed values are passed by pointer at the ABI level, so
/// `Ptr(Struct)` is unwrapped to the struct type (matching how sema types
/// call arguments).  Struct fields are filled in from the struct registry so
/// generic inference can unify field patterns against concrete instances.
///
/// Convert a substituted (concrete) sema type to an IR type.
fn sema_to_ir(ty: &TyType) -> Result<ir::Type> {
    Ok(match ty {
        TyType::Void => ir::Type::Void,
        TyType::Bool => ir::Type::Bool,
        TyType::Char => ir::Type::Char,
        TyType::Int { width, signed } => match (width, signed) {
            (8, true) => ir::Type::I8,
            (16, true) => ir::Type::I16,
            (32, true) => ir::Type::I32,
            (64, true) => ir::Type::I64,
            (8, false) => ir::Type::U8,
            (16, false) => ir::Type::U16,
            (32, false) => ir::Type::U32,
            (64, false) => ir::Type::U64,
            _ => bail!("128-bit integers are not supported by any backend yet"),
        },
        TyType::Float { width } => match width {
            32 => ir::Type::F32,
            _ => ir::Type::F64,
        },
        TyType::USize => ir::Type::U64,
        TyType::ISize => ir::Type::I64,
        TyType::Pointer { pointee }
        | TyType::Own { pointee }
        | TyType::Ref { pointee }
        | TyType::RefMut { pointee } => ir::Type::Ptr(Box::new(sema_to_ir(pointee)?)),
        TyType::Slice { elem } => ir::Type::Slice(Box::new(sema_to_ir(elem)?)),
        TyType::Array { elem, .. } => ir::Type::Ptr(Box::new(sema_to_ir(elem)?)),
        TyType::Struct { name, .. } => ir::Type::Struct(name.clone()),
        TyType::Union { .. } | TyType::Enum { .. } => {
            bail!("union/enum types are not supported in generic signatures yet")
        }
        TyType::Tuple { fields } => {
            let elems: Vec<ir::Type> = fields.iter().map(sema_to_ir).collect::<Result<Vec<_>>>()?;
            // Tuple names are synthesized by the lowerer's struct registry.
            return Ok(ir::Type::Struct(tuple_struct_name(&elems)));
        }
        TyType::Generic { .. } | TyType::StructApp { .. } => {
            bail!("unresolved generic type in lowered signature")
        }
        TyType::Function { .. } => bail!("function types are not supported in the first milestone"),
        TyType::Unknown => bail!("unknown type in lowered signature"),
    })
}

/// Deterministic tuple struct name (mirrors `ensure_tuple_struct`).
fn tuple_struct_name(elem_types: &[ir::Type]) -> String {
    format!(
        "__tuple_{}",
        elem_types
            .iter()
            .map(|t| format!("{:?}", t))
            .collect::<Vec<_>>()
            .join("_")
    )
}

impl LowerCtx<'_> {
    /// Convert an IR type to the sema `Type` used for generic inference and
    /// mangling.  Struct-typed values are passed by pointer at the ABI level,
    /// so `Ptr(Struct)` is unwrapped to the struct type (matching how sema
    /// types call arguments).  Struct fields are filled in from the struct
    /// registry so generic inference can unify field patterns against
    /// concrete instances.
    fn ir_to_sema(&self, ty: &ir::Type) -> TyType {
        match ty {
            ir::Type::I8 => TyType::int(8, true),
            ir::Type::I16 => TyType::int(16, true),
            ir::Type::I32 => TyType::int(32, true),
            ir::Type::I64 => TyType::int(64, true),
            ir::Type::U8 => TyType::int(8, false),
            ir::Type::U16 => TyType::int(16, false),
            ir::Type::U32 => TyType::int(32, false),
            ir::Type::U64 => TyType::int(64, false),
            ir::Type::F32 => TyType::Float { width: 32 },
            ir::Type::F64 => TyType::Float { width: 64 },
            ir::Type::Bool => TyType::Bool,
            ir::Type::Char => TyType::Char,
            ir::Type::Void => TyType::Void,
            ir::Type::Ptr(inner) => match inner.as_ref() {
                ir::Type::Struct(n) if !n.starts_with("__enum_") => self.struct_sema(n),
                other => TyType::pointer(self.ir_to_sema(other)),
            },
            ir::Type::Struct(n) => self.struct_sema(n),
            ir::Type::Slice(elem) => TyType::slice(self.ir_to_sema(elem)),
        }
    }

    /// The sema type of a struct name with its registered field types.
    fn struct_sema(&self, name: &str) -> TyType {
        let fields: Vec<crate::ty::Field> = self
            .structs
            .get(name)
            .map(|d| {
                d.fields
                    .iter()
                    .map(|(fname, fty)| crate::ty::Field {
                        name: fname.clone(),
                        ty: self.ir_to_sema(fty),
                    })
                    .collect()
            })
            .unwrap_or_default();
        TyType::Struct {
            name: name.to_string(),
            fields,
        }
    }

    /// Infer concrete generic arguments by unifying type patterns with
    /// argument types.  Structs compare by name only (fields are resolved
    /// separately by the struct registry).  A `Pair[T]` pattern against a
    /// concrete `Pair$i64` struct recovers the missing arguments by unifying
    /// the base struct's (substituted) field types against the concrete
    /// struct's fields.
    fn lower_collect(
        &mut self,
        pattern: &TyType,
        concrete: &TyType,
        map: &mut HashMap<String, TyType>,
    ) -> Option<()> {
        match (pattern, concrete) {
            (TyType::Generic { name }, _) => {
                map.insert(name.clone(), concrete.clone());
                Some(())
            }
            (TyType::Struct { name: pn, .. }, TyType::Struct { name: cn, .. }) if pn == cn => {
                Some(())
            }
            (TyType::StructApp { base, args }, TyType::Struct { name, .. }) => {
                let sdef = self.module.items.iter().find_map(|it| match it {
                    ast::Item::Struct(s) if s.name == *base => Some(s),
                    _ => None,
                })?;
                let sg: HashSet<String> = sdef.generics.iter().cloned().collect();
                let subst: HashMap<String, TyType> = sdef
                    .generics
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                let field_patterns: Vec<TyType> = sdef
                    .fields
                    .iter()
                    .map(|f| self.pattern_type(&f.ty, &sg).ok())
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .map(|fp| substitute(&fp, &subst))
                    .collect();
                let fields = self.structs.get(name)?.fields.clone();
                if field_patterns.len() != fields.len() {
                    return None;
                }
                for (fp, fc) in field_patterns.iter().zip(fields.iter()) {
                    self.lower_collect(fp, &self.ir_to_sema(&fc.1), map)?;
                }
                Some(())
            }
            (TyType::Pointer { pointee: p }, TyType::Pointer { pointee: c }) => {
                self.lower_collect(p, c, map)
            }
            (TyType::Slice { elem: p }, TyType::Slice { elem: c }) => self.lower_collect(p, c, map),
            (TyType::Array { elem: p, size: ps }, TyType::Array { elem: c, size: cs })
                if ps == cs =>
            {
                self.lower_collect(p, c, map)
            }
            (TyType::Tuple { fields: pf }, TyType::Tuple { fields: cf })
                if pf.len() == cf.len() =>
            {
                for (p, c) in pf.iter().zip(cf.iter()) {
                    self.lower_collect(p, c, map)?;
                }
                Some(())
            }
            (
                TyType::StructApp { base: pb, args: pa },
                TyType::StructApp { base: cb, args: ca },
            ) if pb == cb && pa.len() == ca.len() => {
                for (x, y) in pa.iter().zip(ca.iter()) {
                    self.lower_collect(x, y, map)?;
                }
                Some(())
            }
            (a, b) if a == b || a.is_unknown() || b.is_unknown() => Some(()),
            _ => None,
        }
    }
}
