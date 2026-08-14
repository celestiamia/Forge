//! Semantic analysis implementation.
//!
//! The analyzer performs name resolution and type checking over an
//! `ast::Module`, producing a `TypedModule` where every expression is annotated
//! with its resolved Forge type.

use crate::sema::ast::{self, BinOp, Block, Expr, Import, Item, Literal, Pattern, Span, Stmt, TypeExpr, UnOp};
use crate::sema::error::{Error, Loc};
use crate::sema::typed::*;
use crate::ty::{Type, Field as TyField, Variant as TyVariant};
use std::collections::{HashMap, HashSet};

/// Analyze an AST module and produce a typed module.
pub fn check(module: ast::Module) -> TypedModule {
    check_with_file(module, None)
}

/// Analyze a module with an optional source file path for diagnostics.
pub fn check_with_file(module: ast::Module, file: Option<String>) -> TypedModule {
    let mut ctx = Context::new(file);
    ctx.register_module(&module);
    let items: Vec<TypedItem> = module
        .items
        .iter()
        .map(|item| ctx.check_item(item))
        .collect();
    TypedModule {
        package: module.package.clone(),
        imports: module.imports.clone(),
        items,
        mono_instances: ctx.mono_instances.into_iter().collect(),
        errors: ctx.errors,
    }
}

// ---------------------------------------------------------------------------
// Internal name tables
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FnSig {
    name: String,
    generics: Vec<String>,
    params: Vec<(String, Type)>,
    ret: Type,
    is_unsafe: bool,
    has_body: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdtKind {
    Struct,
    Union,
    Enum,
}

#[derive(Debug, Clone)]
struct AdtInfo {
    name: String,
    kind: AdtKind,
    generics: Vec<String>,
    fields: Vec<TyField>,
    variants: Vec<TyVariant>,
}

#[derive(Debug, Clone)]
struct StaticInfo {
    ty: Type,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
}

struct Context {
    file: Option<String>,
    adts: HashMap<String, AdtInfo>,
    functions: HashMap<String, FnSig>,
    extern_fns: HashMap<String, FnSig>,
    methods: HashMap<String, Vec<FnSig>>,
    statics: HashMap<String, StaticInfo>,
    imports: HashMap<String, Vec<String>>,
    generic_stack: Vec<HashSet<String>>,
    scopes: Vec<HashMap<String, VarInfo>>,
    in_unsafe: bool,
    current_function: Option<String>,
    return_type: Option<Type>,
    mono_instances: Vec<MonoInstance>,
    errors: Vec<Error>,
}

#[cfg(test)]
mod tests;
mod expr;
mod items;
mod typing;

use typing::*;

impl Context {
    pub(super) fn new(file: Option<String>) -> Self {
        Self {
            file,
            adts: HashMap::new(),
            functions: HashMap::new(),
            extern_fns: HashMap::new(),
            methods: HashMap::new(),
            statics: HashMap::new(),
            imports: HashMap::new(),
            generic_stack: vec![HashSet::new()],
            scopes: vec![HashMap::new()],
            in_unsafe: false,
            current_function: None,
            return_type: None,
            mono_instances: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(super) fn error(&mut self, message: impl Into<String>) {
        self.errors.push(Error::new(
            self.file.as_ref().map(|f| Loc::with_file(f.clone())).unwrap_or_else(Loc::unknown),
            message,
        ));
    }

    pub(super) fn error_at(&mut self, span: ast::Span, message: impl Into<String>) {
        let loc = if span.is_unknown() {
            self.file.as_ref().map(|f| Loc::with_file(f.clone())).unwrap_or_else(Loc::unknown)
        } else {
            Loc {
                file: self.file.clone(),
                line: if span.line > 0 { Some(span.line) } else { None },
                col: if span.col > 0 { Some(span.col) } else { None },
            }
        };
        self.errors.push(Error::new(loc, message));
    }

    // Type expression resolution

    pub(super) fn is_generic(&self, name: &str) -> bool {
        self.generic_stack.iter().rev().any(|s| s.contains(name))
    }

    pub(super) fn resolve_type_expr(&mut self, tx: &TypeExpr) -> Type {
        match tx {
            TypeExpr::Name(name) => {
                if self.is_generic(name) {
                    return Type::Generic { name: name.clone() };
                }
                if let Some(t) = primitive_type(name) {
                    return t;
                }
                if let Some(info) = self.adts.get(name) {
                    return adt_type(info);
                }
                self.error(format!("unknown type name `{}`", name));
                Type::Unknown
            }
            TypeExpr::Pointer(inner) => Type::pointer(self.resolve_type_expr(inner)),
            TypeExpr::Own(inner) => Type::own(self.resolve_type_expr(inner)),
            TypeExpr::Ref(inner) => Type::refr(self.resolve_type_expr(inner)),
            TypeExpr::RefMut(inner) => Type::ref_mut(self.resolve_type_expr(inner)),
            TypeExpr::Slice(inner) => Type::slice(self.resolve_type_expr(inner)),
            TypeExpr::Array(inner, size_expr) => {
                let size = self.eval_const_usize(size_expr.as_ref()).unwrap_or(0);
                Type::array(self.resolve_type_expr(inner), size)
            }
            TypeExpr::Tuple(fields) => {
                Type::tuple(fields.iter().map(|t| self.resolve_type_expr(t)).collect())
            }
            TypeExpr::Function { params, ret } => {
                let p: Vec<Type> = params.iter().map(|t| self.resolve_type_expr(t)).collect();
                let r = ret
                    .as_ref()
                    .map(|t| self.resolve_type_expr(t))
                    .unwrap_or(Type::Void);
                Type::function(p, r)
            }
        }
    }

    pub(super) fn eval_const_usize(&mut self, expr: &Expr) -> Option<u64> {
        match expr {
            Expr::Literal(Literal::Int(n)) if *n >= 0 => Some(*n as u64),
            _ => {
                self.error("array size must be a constant non-negative integer literal".to_string());
                None
            }
        }
    }

    // Name resolution: first pass

    pub(super) fn register_module(&mut self, module: &ast::Module) {
        for imp in &module.imports {
            self.register_import(imp);
        }
        // Phase 1: register ADT names so any item can reference any type,
        // regardless of item order in the merged module.
        for item in &module.items {
            match item {
                Item::Struct(s) => self.register_adt_skeleton(s),
                Item::Union(u) => self.register_adt_skeleton(u),
                Item::Enum(e) => self.register_enum_skeleton(e),
                _ => {}
            }
        }
        // Phase 2: resolve ADT fields and variants.
        for item in &module.items {
            match item {
                Item::Struct(s) => self.resolve_adt_fields(s),
                Item::Union(u) => self.resolve_adt_fields(u),
                Item::Enum(e) => self.resolve_enum_variants(e),
                _ => {}
            }
        }
        // Phase 3: function signatures, externs, consts, impls, uses.
        for item in &module.items {
            self.register_item(item);
        }
    }

    pub(super) fn register_import(&mut self, imp: &Import) {
        match imp {
            Import::Path { path, alias } => {
                let alias = alias.clone().unwrap_or_else(|| {
                    path.last().cloned().unwrap_or_default()
                });
                self.imports.insert(alias, path.clone());
            }
            Import::From { path, items } => {
                if let Some(names) = items {
                    for name in names {
                        let mut full = path.clone();
                        full.push(name.clone());
                        self.imports.insert(name.clone(), full);
                    }
                }
            }
        }
    }

    pub(super) fn register_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => {
                let sig = self.register_fn_sig(f, true);
                self.functions.insert(sig.name.clone(), sig);
            }
            Item::Struct(_) | Item::Union(_) | Item::Enum(_) => {}
            Item::ExternFn(e) => {
                let generics: HashSet<String> = e.generics.iter().cloned().collect();
                self.generic_stack.push(generics);
                let params: Vec<(String, Type)> = e
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                    .collect();
                let ret = e
                    .ret
                    .as_ref()
                    .map(|t| self.resolve_type_expr(t))
                    .unwrap_or(Type::Void);
                self.generic_stack.pop();
                let sig = FnSig {
                    name: e.name.clone(),
                    generics: e.generics.clone(),
                    params,
                    ret,
                    is_unsafe: false,
                    has_body: false,
                };
                self.extern_fns.insert(sig.name.clone(), sig);
            }
            Item::Const(c) => {
                let ty = c
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_type_expr(t))
                    .unwrap_or(Type::Unknown);
                self.statics.insert(c.name.clone(), StaticInfo { ty, mutable: false });
            }
            Item::Embed(e) => {
                // `embed NAME = "file"` binds `NAME` as a read-only byte
                // pointer into .rodata plus an implicit `NAME_LEN: int64`
                // length constant.
                self.statics.insert(
                    e.name.clone(),
                    StaticInfo { ty: Type::pointer(Type::int(8, false)), mutable: false },
                );
                self.statics.insert(
                    format!("{}_LEN", e.name),
                    StaticInfo { ty: Type::int(64, true), mutable: false },
                );
            }
            Item::Impl(i) => {
                let target_name = base_type_name_from_type_expr(&i.target);
                for method in &i.methods {
                    let sig = self.register_fn_sig(method, true);
                    self.methods
                        .entry(target_name.clone())
                        .or_default()
                        .push(sig);
                }
            }
            Item::Use(u) => {
                let alias = u.alias.clone().unwrap_or_else(|| {
                    u.path.last().cloned().unwrap_or_default()
                });
                self.imports.insert(alias, u.path.clone());
            }
        }
    }

    pub(super) fn register_fn_sig(&mut self, f: &ast::Function, has_body: bool) -> FnSig {
        let generics: HashSet<String> = f.generics.iter().cloned().collect();
        self.generic_stack.push(generics);
        let params: Vec<(String, Type)> = f
            .params
            .iter()
            .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
            .collect();
        let ret = f
            .ret
            .as_ref()
            .map(|t| self.resolve_type_expr(t))
            .unwrap_or(Type::Void);
        self.generic_stack.pop();
        FnSig {
            name: f.name.clone(),
            generics: f.generics.clone(),
            params,
            ret,
            is_unsafe: f.unsafe_kw,
            has_body,
        }
    }

    pub(super) fn register_adt_skeleton(&mut self, s: &dyn AdtDefinition) {
        let kind = if s.is_union() { AdtKind::Union } else { AdtKind::Struct };
        let info = AdtInfo {
            name: s.name().to_string(),
            kind,
            generics: s.generics().to_vec(),
            fields: Vec::new(),
            variants: Vec::new(),
        };
        self.adts.insert(info.name.clone(), info);
    }

    pub(super) fn resolve_adt_fields(&mut self, s: &dyn AdtDefinition) {
        let generics: HashSet<String> = s.generics().iter().cloned().collect();
        self.generic_stack.push(generics);
        let fields: Vec<TyField> = s
            .fields()
            .iter()
            .map(|f| TyField {
                name: f.name.clone(),
                ty: self.resolve_type_expr(&f.ty),
            })
            .collect();
        self.generic_stack.pop();
        if let Some(info) = self.adts.get_mut(s.name()) {
            info.fields = fields;
        }
    }

    pub(super) fn register_enum_skeleton(&mut self, e: &ast::Enum) {
        let info = AdtInfo {
            name: e.name.clone(),
            kind: AdtKind::Enum,
            generics: e.generics.clone(),
            fields: Vec::new(),
            variants: Vec::new(),
        };
        self.adts.insert(info.name.clone(), info);
    }

    pub(super) fn resolve_enum_variants(&mut self, e: &ast::Enum) {
        let generics: HashSet<String> = e.generics.iter().cloned().collect();
        self.generic_stack.push(generics);
        let variants: Vec<TyVariant> = e
            .variants
            .iter()
            .map(|v| TyVariant {
                name: v.name.clone(),
                payload: v.payload.as_ref().map(|t| self.resolve_type_expr(t)),
            })
            .collect();
        self.generic_stack.pop();
        if let Some(info) = self.adts.get_mut(&e.name) {
            info.variants = variants;
        }
    }

    // Type checking: second pass

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn bind_var(&mut self, name: &str, ty: Type, mutable: bool) {
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), VarInfo { ty, mutable });
    }

    pub(super) fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    // Blocks and statements

}

trait AdtDefinition {
    fn name(&self) -> &str;
    fn generics(&self) -> &[String];
    fn fields(&self) -> &[ast::Field];
    fn is_union(&self) -> bool;
}

impl AdtDefinition for ast::Struct {
    fn name(&self) -> &str { &self.name }
    fn generics(&self) -> &[String] { &self.generics }
    fn fields(&self) -> &[ast::Field] { &self.fields }
    fn is_union(&self) -> bool { false }
}

impl AdtDefinition for ast::Union {
    fn name(&self) -> &str { &self.name }
    fn generics(&self) -> &[String] { &self.generics }
    fn fields(&self) -> &[ast::Field] { &self.fields }
    fn is_union(&self) -> bool { true }
}
