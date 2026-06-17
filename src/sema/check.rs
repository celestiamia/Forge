//! Semantic analysis implementation.
//!
//! The analyzer performs name resolution and type checking over an
//! `ast::Module`, producing a `TypedModule` where every expression is annotated
//! with its resolved Forge type.

use crate::sema::ast::{self, BinOp, Block, Expr, Import, Item, Literal, Pattern, Stmt, TypeExpr, UnOp};
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

impl Context {
    fn new(file: Option<String>) -> Self {
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

    fn error(&mut self, message: impl Into<String>) {
        self.errors.push(Error::new(
            self.file.as_ref().map(|f| Loc::with_file(f.clone())).unwrap_or_else(Loc::unknown),
            message,
        ));
    }

    // -----------------------------------------------------------------------
    // Type expression resolution
    // -----------------------------------------------------------------------

    fn is_generic(&self, name: &str) -> bool {
        self.generic_stack.iter().rev().any(|s| s.contains(name))
    }

    fn resolve_type_expr(&mut self, tx: &TypeExpr) -> Type {
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

    fn eval_const_usize(&mut self, expr: &Expr) -> Option<u64> {
        match expr {
            Expr::Literal(Literal::Int(n)) if *n >= 0 => Some(*n as u64),
            _ => {
                self.error("array size must be a constant non-negative integer literal".to_string());
                None
            }
        }
    }

    // -----------------------------------------------------------------------
    // Name resolution: first pass
    // -----------------------------------------------------------------------

    fn register_module(&mut self, module: &ast::Module) {
        for imp in &module.imports {
            self.register_import(imp);
        }
        for item in &module.items {
            self.register_item(item);
        }
    }

    fn register_import(&mut self, imp: &Import) {
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

    fn register_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => {
                let sig = self.register_fn_sig(f, true);
                self.functions.insert(sig.name.clone(), sig);
            }
            Item::Struct(s) => self.register_adt(s),
            Item::Union(u) => self.register_adt(u),
            Item::Enum(e) => self.register_enum(e),
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

    fn register_fn_sig(&mut self, f: &ast::Function, has_body: bool) -> FnSig {
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

    fn register_adt(&mut self, s: &dyn AdtDefinition) {
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
        let kind = if s.is_union() { AdtKind::Union } else { AdtKind::Struct };
        let info = AdtInfo {
            name: s.name().to_string(),
            kind,
            generics: s.generics().to_vec(),
            fields,
            variants: Vec::new(),
        };
        self.adts.insert(info.name.clone(), info);
    }

    fn register_enum(&mut self, e: &ast::Enum) {
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
        let info = AdtInfo {
            name: e.name.clone(),
            kind: AdtKind::Enum,
            generics: e.generics.clone(),
            fields: Vec::new(),
            variants,
        };
        self.adts.insert(info.name.clone(), info);
    }

    // -----------------------------------------------------------------------
    // Type checking: second pass
    // -----------------------------------------------------------------------

    fn check_item(&mut self, item: &Item) -> TypedItem {
        match item {
            Item::Function(f) => TypedItem::Function(self.check_function(f)),
            Item::Struct(s) => TypedItem::Struct {
                name: s.name.clone(),
                generics: s.generics.clone(),
                fields: self.adts.get(&s.name).map(|i| i.fields.clone()).unwrap_or_default(),
            },
            Item::Union(u) => TypedItem::Union {
                name: u.name.clone(),
                generics: u.generics.clone(),
                fields: self.adts.get(&u.name).map(|i| i.fields.clone()).unwrap_or_default(),
            },
            Item::Enum(e) => TypedItem::Enum {
                name: e.name.clone(),
                generics: e.generics.clone(),
                variants: self.adts.get(&e.name).map(|i| i.variants.clone()).unwrap_or_default(),
            },
            Item::ExternFn(e) => {
                let sig = self.extern_fns.get(&e.name).cloned().unwrap();
                TypedItem::ExternFn {
                    name: sig.name,
                    generics: sig.generics,
                    params: sig.params,
                    ret: sig.ret,
                }
            }
            Item::Const(c) => {
                let ty = c
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_type_expr(t))
                    .unwrap_or(Type::Unknown);
                let value = self.check_expr(&c.value, Some(&ty));
                self.expect_type(&ty, &value.ty, "constant initializer");
                TypedItem::Const {
                    name: c.name.clone(),
                    ty,
                    value,
                }
            }
            Item::Use(u) => TypedItem::Use {
                path: u.path.clone(),
                alias: u.alias.clone(),
            },
            Item::Impl(i) => {
                let target = self.resolve_type_expr(&i.target);
                let methods: Vec<TypedFunction> = i
                    .methods
                    .iter()
                    .map(|m| self.check_function(m))
                    .collect();
                TypedItem::Impl { target, methods }
            }
        }
    }

    fn check_function(&mut self, f: &ast::Function) -> TypedFunction {
        let sig = self
            .functions
            .get(&f.name)
            .or_else(|| self.extern_fns.get(&f.name))
            .cloned()
            .unwrap();
        let prev_unsafe = self.in_unsafe;
        self.in_unsafe = self.in_unsafe || sig.is_unsafe;
        self.current_function = Some(sig.name.clone());
        self.return_type = Some(sig.ret.clone());

        let body = f.body.as_ref().map(|b| {
            self.push_scope();
            for (name, ty) in &sig.params {
                self.bind_var(name, ty.clone(), true);
            }
            let block = self.check_block(b);
            self.pop_scope();
            // If the function declares a non-void return type, make sure the
            // body's trailing expression matches or that every execution path
            // reaches a `return`.  Void functions may end with any statement.
            if !sig.ret.is_void()
                && !sig.ret.is_unknown()
                && !block.ty.is_unknown()
                && block.ty != sig.ret
                && !Self::block_definitely_returns(&block)
            {
                self.error(format!(
                    "function `{}` returns `{}`, but body may not return a value",
                    sig.name, sig.ret
                ));
            }
            block
        });

        self.return_type = None;
        self.current_function = None;
        self.in_unsafe = prev_unsafe;

        TypedFunction {
            name: sig.name,
            generics: sig.generics,
            params: sig.params,
            ret: sig.ret,
            body,
            is_unsafe: sig.is_unsafe,
        }
    }

    /// Return true if every execution path through the block reaches a `return`.
    /// This is intentionally conservative: it recognizes top-level returns,
    /// returns inside `unsafe` blocks, and infinite loops (`while true` / `loop`)
    /// that contain a return anywhere in their body.
    fn block_definitely_returns(block: &TypedBlock) -> bool {
        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i == block.stmts.len() - 1;
            match stmt {
                TypedStmt::Return(_) => return true,
                TypedStmt::UnsafeBlock(b) if is_last => return Self::block_definitely_returns(b),
                TypedStmt::While { cond, body } if is_last => {
                    if Self::is_infinite_cond(cond) && Self::block_contains_return(body) {
                        return true;
                    }
                }
                TypedStmt::Loop(body) if is_last => {
                    if Self::block_contains_return(body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn is_infinite_cond(cond: &TypedExpr) -> bool {
        matches!(
            cond.kind,
            TypedExprKind::Literal(Literal::Bool(true)) | TypedExprKind::Literal(Literal::Int(1))
        )
    }

    /// Return true if the block contains a `return` statement at any nesting level.
    fn block_contains_return(block: &TypedBlock) -> bool {
        for stmt in &block.stmts {
            match stmt {
                TypedStmt::Return(_) => return true,
                TypedStmt::UnsafeBlock(b) |
                TypedStmt::If { then_block: b, else_block: None, .. } => {
                    if Self::block_contains_return(b) {
                        return true;
                    }
                }
                TypedStmt::If {
                    then_block: t,
                    else_block: Some(e),
                    ..
                } => {
                    if Self::block_contains_return(t) || Self::block_contains_return(e) {
                        return true;
                    }
                }
                TypedStmt::While { body, .. } | TypedStmt::For { body, .. } | TypedStmt::Loop(body) => {
                    if Self::block_contains_return(body) {
                        return true;
                    }
                }
                TypedStmt::Match { cases, .. } => {
                    if cases.iter().any(|c| Self::block_contains_return(&c.body)) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Scopes
    // -----------------------------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind_var(&mut self, name: &str, ty: Type, mutable: bool) {
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), VarInfo { ty, mutable });
    }

    fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    // -----------------------------------------------------------------------
    // Blocks and statements
    // -----------------------------------------------------------------------

    fn check_block(&mut self, block: &Block) -> TypedBlock {
        let mut stmts = Vec::with_capacity(block.stmts.len());
        let mut last_ty = Type::Void;
        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i == block.stmts.len() - 1;
            let typed = self.check_stmt(stmt);
            if is_last {
                match &typed {
                    TypedStmt::Expr(e) => last_ty = e.ty.clone(),
                    TypedStmt::Return(Some(e)) => last_ty = e.ty.clone(),
                    TypedStmt::UnsafeBlock(b) => last_ty = b.ty.clone(),
                    _ => {}
                }
            }
            stmts.push(typed);
        }
        TypedBlock { stmts, ty: last_ty }
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> TypedStmt {
        match stmt {
            Stmt::Let(l) => {
                let annotated = l.ty.as_ref().map(|t| self.resolve_type_expr(t));
                let (init, ty) = if let Some(value) = l.value.as_ref() {
                    let init = self.check_expr(value, annotated.as_ref());
                    let ty = annotated.unwrap_or_else(|| init.ty.clone());
                    if !init.ty.is_unknown() && !ty.is_unknown() && init.ty != ty {
                        self.error(format!(
                            "`let {}` expected `{}`, found `{}`",
                            l.name, ty, init.ty
                        ));
                    }
                    (init, ty)
                } else {
                    let ty = annotated.unwrap_or_else(|| {
                        self.error(format!("`let {}` needs a type annotation or initializer", l.name));
                        Type::Unknown
                    });
                    (zero_expr(&ty), ty)
                };
                self.bind_var(&l.name, ty.clone(), false);
                TypedStmt::Let {
                    name: l.name.clone(),
                    ty,
                    init,
                    mutable: false,
                }
            }
            Stmt::Var(v) => {
                let annotated = v.ty.as_ref().map(|t| self.resolve_type_expr(t));
                let (init, ty) = if let Some(value) = v.value.as_ref() {
                    let init = self.check_expr(value, annotated.as_ref());
                    let ty = annotated.unwrap_or_else(|| init.ty.clone());
                    if !init.ty.is_unknown() && !ty.is_unknown() && init.ty != ty {
                        self.error(format!(
                            "`var {}` expected `{}`, found `{}`",
                            v.name, ty, init.ty
                        ));
                    }
                    (init, ty)
                } else {
                    let ty = annotated.unwrap_or_else(|| {
                        self.error(format!("`var {}` needs a type annotation or initializer", v.name));
                        Type::Unknown
                    });
                    (zero_expr(&ty), ty)
                };
                self.bind_var(&v.name, ty.clone(), true);
                TypedStmt::Var {
                    name: v.name.clone(),
                    ty,
                    init,
                }
            }
            Stmt::Assign(a) => {
                let target = self.check_expr(&a.target, None);
                let value = self.check_expr(&a.value, Some(&target.ty));
                if !self.is_mutable_lvalue(&target) {
                    self.error(format!(
                        "cannot assign to immutable or non-lvalue expression"
                    ));
                }
                if !value.ty.is_unknown() && !target.ty.is_unknown() && value.ty != target.ty {
                    self.error(format!(
                        "assignment expected `{}`, found `{}`",
                        target.ty, value.ty
                    ));
                }
                TypedStmt::Assign { target, value }
            }
            Stmt::Expr(e) => TypedStmt::Expr(self.check_expr(e, None)),
            Stmt::Return(e) => {
                let ret = self.return_type.clone().unwrap_or(Type::Unknown);
                let value = e.as_ref().map(|v| self.check_expr(v, Some(&ret)));
                if let Some(v) = &value {
                    if !v.ty.is_unknown() && !ret.is_unknown() && v.ty != ret {
                        self.error(format!(
                            "return expected `{}`, found `{}`",
                            ret, v.ty
                        ));
                    }
                } else if !ret.is_void() && !ret.is_unknown() {
                    self.error("missing return value".to_string());
                }
                TypedStmt::Return(value)
            }
            Stmt::If(i) => {
                let cond = self.check_expr(&i.condition, Some(&Type::Bool));
                if !cond.ty.is_unknown() && cond.ty != Type::Bool {
                    self.error(format!("if condition must be bool, found `{}`", cond.ty));
                }
                let then_block = self.check_block(&i.then_block);
                let elifs: Vec<(TypedExpr, TypedBlock)> = i
                    .elifs
                    .iter()
                    .map(|(c, b)| {
                        let tc = self.check_expr(c, Some(&Type::Bool));
                        if !tc.ty.is_unknown() && tc.ty != Type::Bool {
                            self.error(format!("elif condition must be bool, found `{}`", tc.ty));
                        }
                        (tc, self.check_block(b))
                    })
                    .collect();
                let else_block = i.else_block.as_ref().map(|b| self.check_block(b));
                TypedStmt::If {
                    cond,
                    then_block,
                    elifs,
                    else_block,
                }
            }
            Stmt::For(f) => {
                let iter = self.check_expr(&f.iter, None);
                let elem_ty = self.iter_element_type(&iter.ty);
                self.push_scope();
                self.bind_var(&f.var, elem_ty, true);
                let body = self.check_block(&f.body);
                self.pop_scope();
                TypedStmt::For {
                    var: f.var.clone(),
                    iter,
                    body,
                }
            }
            Stmt::While(w) => {
                let cond = self.check_expr(&w.condition, Some(&Type::Bool));
                if !cond.ty.is_unknown() && cond.ty != Type::Bool {
                    self.error(format!("while condition must be bool, found `{}`", cond.ty));
                }
                let body = self.check_block(&w.body);
                TypedStmt::While { cond, body }
            }
            Stmt::Match(m) => {
                let scrutinee = self.check_expr(&m.scrutinee, None);
                let mut cases = Vec::new();
                for case in &m.cases {
                    self.push_scope();
                    self.check_pattern(&case.pattern, &scrutinee.ty);
                    let body = self.check_block(&case.body);
                    self.pop_scope();
                    cases.push(TypedMatchCase {
                        pattern: self.lower_pattern(&case.pattern),
                        body,
                    });
                }
                self.check_match_exhaustive(&scrutinee.ty, &cases);
                TypedStmt::Match { scrutinee, cases }
            }
            Stmt::UnsafeBlock(b) => {
                let prev = self.in_unsafe;
                self.in_unsafe = true;
                let block = self.check_block(b);
                self.in_unsafe = prev;
                TypedStmt::UnsafeBlock(block)
            }
            Stmt::Loop(b) => {
                let body = self.check_block(b);
                TypedStmt::Loop(body)
            }
            Stmt::Break => TypedStmt::Break,
            Stmt::Continue => TypedStmt::Continue,
        }
    }

    fn iter_element_type(&mut self, ty: &Type) -> Type {
        match ty {
            Type::Slice { elem } => *elem.clone(),
            Type::Array { elem, .. } => *elem.clone(),
            Type::Pointer { pointee } => *pointee.clone(), // pointer iteration (unsafe elsewhere)
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(format!("cannot iterate over type `{}`", ty));
                Type::Unknown
            }
        }
    }

    fn check_pattern(&mut self, pat: &Pattern, ty: &Type) {
        match pat {
            Pattern::Wildcard => {}
            Pattern::Literal(l) => {
                let lit_ty = literal_type(l, Some(ty));
                if !lit_ty.is_unknown() && !ty.is_unknown() && lit_ty != *ty {
                    self.error(format!("pattern literal type `{}` does not match `{}`", lit_ty, ty));
                }
            }
            Pattern::Ident(name) => {
                self.bind_var(name, ty.clone(), false);
            }
            Pattern::Tuple(pats) => {
                if let Type::Tuple { fields } = ty {
                    if pats.len() != fields.len() {
                        self.error(format!(
                            "tuple pattern has {} elements, but value has {}",
                            pats.len(),
                            fields.len()
                        ));
                    } else {
                        for (p, f) in pats.iter().zip(fields.iter()) {
                            self.check_pattern(p, f);
                        }
                    }
                } else if !ty.is_unknown() {
                    self.error(format!("cannot match tuple pattern against non-tuple type `{}`", ty));
                }
            }
        }
    }

    fn lower_pattern(&self, pat: &Pattern) -> TypedPattern {
        match pat {
            Pattern::Wildcard => TypedPattern::Wildcard,
            Pattern::Literal(l) => TypedPattern::Literal(l.clone()),
            Pattern::Ident(name) => TypedPattern::Ident(name.clone()),
            Pattern::Tuple(pats) => TypedPattern::Tuple(pats.iter().map(|p| self.lower_pattern(p)).collect()),
        }
    }

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    fn check_expr(&mut self, expr: &Expr, expected: Option<&Type>) -> TypedExpr {
        match expr {
            Expr::Binary(b) => self.check_binary(b, expected),
            Expr::Unary(u) => self.check_unary(u, expected),
            Expr::Literal(l) => {
                let ty = literal_type(l, expected);
                TypedExpr::new(TypedExprKind::Literal(l.clone()), ty)
            }
            Expr::Ident(name) => self.check_ident(name),
            Expr::Call(c) => self.check_call(c, expected),
            Expr::Field(f) => self.check_field(f),
            Expr::Index(i) => self.check_index(i),
            Expr::Cast(c) => self.check_cast(c),
            Expr::Asm(a) => self.check_asm(a),
            Expr::SizeOf(s) => {
                let ty = self.resolve_type_expr(&s.ty);
                TypedExpr::new(TypedExprKind::SizeOf(ty), Type::USize)
            }
            Expr::OffsetOf(o) => {
                let ty = self.resolve_type_expr(&o.ty);
                let idx = field_index(&ty, &o.field, self);
                TypedExpr::new(
                    TypedExprKind::OffsetOf {
                        ty: ty.clone(),
                        field: o.field.clone(),
                        field_index: idx,
                    },
                    Type::USize,
                )
            }
            Expr::Deref(d) => {
                let operand = self.check_expr(&d.expr, None);
                let result_ty = match &operand.ty {
                    Type::Pointer { pointee } => {
                        if !self.in_unsafe {
                            self.error("raw pointer dereference requires `unsafe`".to_string());
                        }
                        *pointee.clone()
                    }
                    Type::Ref { pointee } | Type::RefMut { pointee } => *pointee.clone(),
                    _ => {
                        self.error(format!("cannot dereference non-pointer type `{}`", operand.ty));
                        Type::Unknown
                    }
                };
                TypedExpr::new(TypedExprKind::Deref(Box::new(operand)), result_ty)
            }
            Expr::Ref(r) => {
                let operand = self.check_expr(&r.expr, None);
                let ty = Type::refr(operand.ty.clone());
                TypedExpr::new(TypedExprKind::Ref(Box::new(operand)), ty)
            }
            Expr::RefMut(r) => {
                let operand = self.check_expr(&r.expr, None);
                if !self.is_mutable_lvalue(&operand) {
                    self.error("cannot take a mutable reference to an immutable place".to_string());
                }
                let ty = Type::ref_mut(operand.ty.clone());
                TypedExpr::new(TypedExprKind::RefMut(Box::new(operand)), ty)
            }
            Expr::Tuple(t) => {
                let mut fields = Vec::new();
                let mut field_tys = Vec::new();
                if let Some(Type::Tuple { fields: expected_fields }) = expected {
                    for (i, e) in t.iter().enumerate() {
                        let exp = expected_fields.get(i);
                        let te = self.check_expr(e, exp);
                        field_tys.push(te.ty.clone());
                        fields.push(te);
                    }
                } else {
                    for e in t {
                        let te = self.check_expr(e, None);
                        field_tys.push(te.ty.clone());
                        fields.push(te);
                    }
                }
                let ty = Type::tuple(field_tys);
                TypedExpr::new(TypedExprKind::Tuple(fields), ty)
            }
            Expr::Array(a) => {
                let mut elems = Vec::new();
                let mut elem_ty = Type::Unknown;
                if let Some(Type::Array { elem: expected_elem, .. }) = expected {
                    elem_ty = *expected_elem.clone();
                    for e in a {
                        let te = self.check_expr(e, Some(&elem_ty));
                        elems.push(te);
                    }
                } else {
                    for (i, e) in a.iter().enumerate() {
                        let te = self.check_expr(e, None);
                        if i == 0 {
                            elem_ty = te.ty.clone();
                        }
                        elems.push(te);
                    }
                }
                let ty = Type::array(elem_ty.clone(), elems.len() as u64);
                TypedExpr::new(TypedExprKind::Array(elems), ty)
            }
            Expr::Range(r) => {
                let start = r
                    .start
                    .as_ref()
                    .map(|e| Box::new(self.check_expr(e, None)));
                let end = r.end.as_ref().map(|e| Box::new(self.check_expr(e, None)));
                TypedExpr::new(
                    TypedExprKind::Range {
                        start,
                        end,
                        inclusive: r.inclusive,
                    },
                    Type::Unknown,
                )
            }
            Expr::If(i) => self.check_if_expr(i),
            Expr::Match(m) => self.check_match_expr(m),
            Expr::Block(b) => {
                let block = self.check_block(b);
                TypedExpr::new(TypedExprKind::Block(block.clone()), block.ty.clone())
            }
            Expr::Loop(b) => {
                let block = self.check_block(b);
                TypedExpr::new(TypedExprKind::Loop(block), Type::Unknown)
            }
            Expr::Break => TypedExpr::new(TypedExprKind::Break, Type::Unknown),
            Expr::Continue => TypedExpr::new(TypedExprKind::Continue, Type::Unknown),
            Expr::StructLiteral { name, fields } => {
                let _ = (name, fields);
                self.error("struct literals are not supported in the first milestone".to_string());
                TypedExpr::new(TypedExprKind::Literal(Literal::Null), Type::Unknown)
            }
            Expr::UnsafeBlock(b) => {
                let old = self.in_unsafe;
                self.in_unsafe = true;
                let block = self.check_block(b);
                self.in_unsafe = old;
                TypedExpr::new(TypedExprKind::Block(block.clone()), block.ty.clone())
            }
        }
    }

    fn check_ident(&mut self, name: &str) -> TypedExpr {
        if let Some(var) = self.lookup_var(name) {
            return TypedExpr::new(
                TypedExprKind::Ident(name.to_string()),
                var.ty.clone(),
            );
        }
        if let Some(sig) = self.functions.get(name).or_else(|| self.extern_fns.get(name)) {
            let ty = Type::function(
                sig.params.iter().map(|(_, t)| t.clone()).collect(),
                sig.ret.clone(),
            );
            return TypedExpr::new(TypedExprKind::Ident(name.to_string()), ty);
        }
        if self.adts.contains_key(name) || self.imports.contains_key(name) {
            return TypedExpr::new(TypedExprKind::Ident(name.to_string()), Type::Unknown);
        }
        self.error(format!("unknown identifier `{}`", name));
        TypedExpr::new(TypedExprKind::Ident(name.to_string()), Type::Unknown)
    }

    fn check_binary(&mut self, b: &ast::BinaryExpr, expected: Option<&Type>) -> TypedExpr {
        use BinOp::*;

        if b.op == Assign {
            let target = self.check_expr(&b.left, None);
            let value = self.check_expr(&b.right, Some(&target.ty));
            if !self.is_mutable_lvalue(&target) {
                self.error("cannot assign to immutable or non-lvalue expression".to_string());
            }
            if !value.ty.is_unknown() && !target.ty.is_unknown() && value.ty != target.ty {
                self.error(format!(
                    "assignment expected `{}`, found `{}`",
                    target.ty, value.ty
                ));
            }
            return TypedExpr::new(
                TypedExprKind::Binary {
                    op: b.op,
                    left: Box::new(target),
                    right: Box::new(value),
                },
                Type::Void,
            );
        }

        let (left, right, result_ty) = match b.op {
            And | Or => {
                let left = self.check_expr(&b.left, Some(&Type::Bool));
                let right = self.check_expr(&b.right, Some(&Type::Bool));
                if !left.ty.is_unknown() && left.ty != Type::Bool {
                    self.error(format!("`{}` expected bool, found `{}`", op_name(b.op), left.ty));
                }
                if !right.ty.is_unknown() && right.ty != Type::Bool {
                    self.error(format!("`{}` expected bool, found `{}`", op_name(b.op), right.ty));
                }
                (left, right, Type::Bool)
            }
            Eq | Ne | Lt | Le | Gt | Ge => {
                let left = self.check_expr(&b.left, expected.filter(|t| t.is_numeric() || t.is_pointer()));
                // Type the right operand against the left's type so that a
                // `null` literal is coerced to the pointer type it is compared
                // against (e.g. `buf == null` makes `null` take `buf`'s type).
                let right_expected = if left.ty.is_numeric() || left.ty.is_pointer() {
                    Some(&left.ty)
                } else {
                    None
                };
                let right = self.check_expr(&b.right, right_expected);
                // A pointer compared against an untyped `null` literal (`*?`,
                // i.e. `Pointer { pointee: Unknown }`) is always allowed: the
                // null side is taken to have the other operand's pointer type.
                let left_nullish =
                    matches!(&left.ty, Type::Pointer { pointee } if pointee.is_unknown());
                let right_nullish =
                    matches!(&right.ty, Type::Pointer { pointee } if pointee.is_unknown());
                let compatible = left.ty == right.ty
                    || left.ty.is_unknown()
                    || right.ty.is_unknown()
                    || (left.ty.is_pointer()
                        && right.ty.is_pointer()
                        && (left_nullish || right_nullish));
                if !compatible {
                    self.error(format!(
                        "comparison `{}` between incompatible types `{}` and `{}`",
                        op_name(b.op), left.ty, right.ty
                    ));
                }
                (left, right, Type::Bool)
            }
            BitAnd | BitOr | BitXor | Shl | Shr => {
                let left = self.check_expr(&b.left, expected.filter(|t| t.is_integer()));
                let right = self.check_expr(&b.right, Some(&left.ty).filter(|t| t.is_integer()));
                if !left.ty.is_unknown() && !left.ty.is_integer() {
                    self.error(format!("bitwise op `{}` requires integer, found `{}`", op_name(b.op), left.ty));
                }
                if !right.ty.is_unknown() && !right.ty.is_integer() {
                    self.error(format!("bitwise op `{}` requires integer, found `{}`", op_name(b.op), right.ty));
                }
                let result_ty = left.ty.clone();
                (left, right, result_ty)
            }
            Add | Sub | Mul | Div | Mod => {
                let left = self.check_expr(&b.left, expected.filter(|t| t.is_numeric()));
                let right = self.check_expr(&b.right, Some(&left.ty).filter(|t| t.is_numeric()));

                // Pointer arithmetic is allowed only inside unsafe blocks.
                let result_ty = if left.ty.is_pointer() && right.ty.is_integer() {
                    if !self.in_unsafe {
                        self.error("pointer arithmetic requires `unsafe`".to_string());
                    }
                    left.ty.clone()
                } else if right.ty.is_pointer() && left.ty.is_integer() && b.op == Add {
                    if !self.in_unsafe {
                        self.error("pointer arithmetic requires `unsafe`".to_string());
                    }
                    right.ty.clone()
                } else {
                    if !left.ty.is_unknown() && !left.ty.is_numeric() {
                        self.error(format!("arithmetic op `{}` requires numeric type, found `{}`", op_name(b.op), left.ty));
                    }
                    if !right.ty.is_unknown() && !right.ty.is_numeric() {
                        self.error(format!("arithmetic op `{}` requires numeric type, found `{}`", op_name(b.op), right.ty));
                    }
                    if !left.ty.is_unknown() && !right.ty.is_unknown() && left.ty != right.ty {
                        self.error(format!(
                            "arithmetic `{}` between incompatible types `{}` and `{}`",
                            op_name(b.op), left.ty, right.ty
                        ));
                    }
                    left.ty.clone()
                };
                (left, right, result_ty)
            }
            Assign => unreachable!("assignment handled above"),
            FloorDiv | Power => {
                self.error(format!("`{}` is not supported in the first milestone", op_name(b.op)));
                let left = self.check_expr(&b.left, None);
                let right = self.check_expr(&b.right, None);
                (left, right, Type::Unknown)
            }
        };

        TypedExpr::new(
            TypedExprKind::Binary {
                op: b.op,
                left: Box::new(left),
                right: Box::new(right),
            },
            result_ty,
        )
    }

    fn check_unary(&mut self, u: &ast::UnaryExpr, expected: Option<&Type>) -> TypedExpr {
        use UnOp::*;
        let operand = self.check_expr(&u.operand, None);
        match u.op {
            Neg => {
                if !operand.ty.is_unknown() && !operand.ty.is_numeric() {
                    self.error(format!("negation requires numeric type, found `{}`", operand.ty));
                }
                let ty = expected.cloned().unwrap_or_else(|| operand.ty.clone());
                TypedExpr::new(TypedExprKind::Unary { op: u.op, operand: Box::new(operand) }, ty)
            }
            Not => {
                if !operand.ty.is_unknown() && operand.ty != Type::Bool {
                    self.error(format!("logical not requires bool, found `{}`", operand.ty));
                }
                TypedExpr::new(TypedExprKind::Unary { op: u.op, operand: Box::new(operand) }, Type::Bool)
            }
            BitNot => {
                if !operand.ty.is_unknown() && !operand.ty.is_integer() {
                    self.error(format!("bitwise not requires integer, found `{}`", operand.ty));
                }
                let result_ty = operand.ty.clone();
                TypedExpr::new(TypedExprKind::Unary { op: u.op, operand: Box::new(operand) }, result_ty)
            }
            Deref => {
                let result_ty = match &operand.ty {
                    Type::Pointer { pointee } => {
                        if !self.in_unsafe {
                            self.error("raw pointer dereference requires `unsafe`".to_string());
                        }
                        *pointee.clone()
                    }
                    Type::Ref { pointee } | Type::RefMut { pointee } => *pointee.clone(),
                    _ => {
                        self.error(format!("cannot dereference non-pointer type `{}`", operand.ty));
                        Type::Unknown
                    }
                };
                TypedExpr::new(TypedExprKind::Unary { op: u.op, operand: Box::new(operand) }, result_ty)
            }
            Ref => {
                let pointee = operand.ty.clone();
                TypedExpr::new(TypedExprKind::Ref(Box::new(operand)), Type::refr(pointee))
            }
        }
    }

    fn check_call(&mut self, c: &ast::CallExpr, expected: Option<&Type>) -> TypedExpr {
        let callee = self.check_expr(&c.callee, None);
        let args_in: Vec<&Expr> = c.args.iter().collect();

        // Method call: `obj.method(args)`
        if let Expr::Field(field) = &c.callee.as_ref() {
            if let Some(target_name) = adt_name_from_type(&callee.ty) {
                if let Some(methods) = self.methods.get(&target_name).cloned() {
                    if let Some(sig) = methods.iter().find(|m| m.name == field.field).cloned() {
                        return self.resolve_call(callee, &args_in, &sig, expected, Some(target_name));
                    }
                }
            }
        }

        // Direct function call
        if let Expr::Ident(name) = &c.callee.as_ref() {
            if let Some(sig) = self.functions.get(name).or_else(|| self.extern_fns.get(name)).cloned() {
                return self.resolve_call(callee, &args_in, &sig, expected, None);
            }
            self.error(format!("call to unknown function `{}`", name));
        }

        // Function pointer / other callable
        if let Type::Function { params, ret } = &callee.ty {
            let typed_args: Vec<TypedExpr> = args_in
                .iter()
                .zip(params.iter())
                .map(|(arg, p)| self.check_expr(arg, Some(p)))
                .collect();
            for (i, (arg, p)) in typed_args.iter().zip(params.iter()).enumerate() {
                if !arg.ty.is_unknown() && !p.is_unknown() && arg.ty != *p {
                    self.error(format!(
                        "argument {} expected `{}`, found `{}`",
                        i + 1, p, arg.ty
                    ));
                }
            }
            let ret = *ret.clone();
            return TypedExpr::new(
                TypedExprKind::Call {
                    callee: Box::new(callee),
                    args: typed_args,
                    generic_args: None,
                    mangled_name: None,
                },
                ret,
            );
        }

        self.error(format!("cannot call non-function type `{}`", callee.ty));
        TypedExpr::new(
            TypedExprKind::Call {
                callee: Box::new(callee),
                args: Vec::new(),
                generic_args: None,
                mangled_name: None,
            },
            Type::Unknown,
        )
    }

    fn resolve_call(
        &mut self,
        callee: TypedExpr,
        args_in: &[&Expr],
        sig: &FnSig,
        _expected: Option<&Type>,
        method_target: Option<String>,
    ) -> TypedExpr {
        let param_tys: Vec<Type> = sig.params.iter().map(|(_, t)| t.clone()).collect();
        let n = args_in.len();
        if n != param_tys.len() {
            self.error(format!(
                "function `{}` expects {} arguments, got {}",
                sig.name, param_tys.len(), n
            ));
        }

        // Type arguments with their expected parameter types so literals coerce.
        let typed_args: Vec<TypedExpr> = args_in
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                let exp = param_tys.get(i);
                self.check_expr(arg, exp)
            })
            .collect();
        let arg_tys: Vec<Type> = typed_args.iter().map(|a| a.ty.clone()).collect();

        // Infer and substitute generic parameters.
        let (generic_args, ret_ty) = if sig.generics.is_empty() {
            (None, sig.ret.clone())
        } else {
            match infer_generic_params(&param_tys, &arg_tys) {
                Some(mapping) => {
                    let generic_args: Vec<Type> = sig
                        .generics
                        .iter()
                        .map(|g| mapping.get(g).cloned().unwrap_or(Type::Unknown))
                        .collect();
                    let ret = substitute(&sig.ret, &mapping);
                    let mono = MonoInstance::new(
                        method_target
                            .as_ref()
                            .map(|t| format!("{}::{}", t, sig.name))
                            .unwrap_or_else(|| sig.name.clone()),
                        generic_args.clone(),
                    );
                    if !self.mono_instances.iter().any(|m| *m == mono) {
                        self.mono_instances.push(mono);
                    }
                    (Some(generic_args), ret)
                }
                None => {
                    self.error(format!(
                        "could not infer generic arguments for `{}`",
                        sig.name
                    ));
                    (None, sig.ret.clone())
                }
            }
        };

        // Final compatibility check.
        for (i, (arg, p)) in typed_args.iter().zip(param_tys.iter()).enumerate() {
            if !compatible(p, &arg.ty) {
                self.error(format!(
                    "argument {} expected `{}`, found `{}`",
                    i + 1, p, arg.ty
                ));
            }
        }

        let mangled_name = generic_args.as_ref().map(|args| {
            MonoInstance::new(
                method_target
                    .as_ref()
                    .map(|t| format!("{}::{}", t, sig.name))
                    .unwrap_or_else(|| sig.name.clone()),
                args.clone(),
            )
            .mangled_name
        });

        TypedExpr::new(
            TypedExprKind::Call {
                callee: Box::new(callee),
                args: typed_args,
                generic_args,
                mangled_name,
            },
            ret_ty,
        )
    }

    fn check_field(&mut self, f: &ast::FieldExpr) -> TypedExpr {
        let object = self.check_expr(&f.object, None);
        let object_ty = object.ty.clone();
        let (base_ty, _deref_once) = match &object_ty {
            Type::Pointer { pointee } => (&**pointee, true),
            _ => (&object_ty, false),
        };

        let base_name = base_type_name(base_ty);
        let is_union = self
            .adts
            .get(&base_name)
            .map(|info| info.kind == AdtKind::Union)
            .unwrap_or(false);
        if is_union && !self.in_unsafe {
            self.error("union field access requires `unsafe`".to_string());
        }

        if let Some(info) = self.adts.get(&base_name) {
            if let Some((idx, field)) = info.fields.iter().enumerate().find(|(_, fld)| fld.name == f.field) {
                let field_ty = field.ty.clone();
                return TypedExpr::new(
                    TypedExprKind::Field {
                        object: Box::new(object),
                        field: f.field.clone(),
                        field_index: idx,
                    },
                    field_ty,
                );
            }
        }

        if let Type::Tuple { fields } = base_ty {
            if let Ok(idx) = f.field.parse::<usize>() {
                if idx < fields.len() {
                    return TypedExpr::new(
                        TypedExprKind::Field {
                            object: Box::new(object),
                            field: f.field.clone(),
                            field_index: idx,
                        },
                        fields[idx].clone(),
                    );
                }
            }
        }

        self.error(format!(
            "type `{}` has no field `{}`",
            object.ty, f.field
        ));
        TypedExpr::new(
            TypedExprKind::Field {
                object: Box::new(object),
                field: f.field.clone(),
                field_index: 0,
            },
            Type::Unknown,
        )
    }

    fn check_index(&mut self, i: &ast::IndexExpr) -> TypedExpr {
        let object = self.check_expr(&i.object, None);
        let index = self.check_expr(&i.index, Some(&Type::int(32, true)));
        if !index.ty.is_unknown() && !index.ty.is_integer() {
            self.error(format!("index must be integer, found `{}`", index.ty));
        }
        match &object.ty {
            Type::Slice { elem } | Type::Array { elem, .. } => {
                let elem = *elem.clone();
                TypedExpr::new(
                    TypedExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    elem,
                )
            }
            Type::Pointer { pointee } => {
                if !self.in_unsafe {
                    self.error("pointer indexing requires `unsafe`".to_string());
                }
                let elem = *pointee.clone();
                TypedExpr::new(
                    TypedExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    elem,
                )
            }
            _ => {
                self.error(format!("cannot index type `{}`", object.ty));
                TypedExpr::new(
                    TypedExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    Type::Unknown,
                )
            }
        }
    }

    fn check_cast(&mut self, c: &ast::CastExpr) -> TypedExpr {
        let expr = self.check_expr(&c.expr, None);
        let to = self.resolve_type_expr(c.ty.as_ref());
        if !cast_allowed(&expr.ty, &to) {
            self.error(format!(
                "cannot cast from `{}` to `{}`",
                expr.ty, to
            ));
        }
        TypedExpr::new(TypedExprKind::Cast { expr: Box::new(expr), ty: to.clone() }, to)
    }

    fn check_asm(&mut self, a: &ast::AsmExpr) -> TypedExpr {
        if !self.in_unsafe {
            self.error("inline assembly requires `unsafe`".to_string());
        }
        let inputs: Vec<TypedAsmOperand> = a
            .inputs
            .iter()
            .map(|op| TypedAsmOperand {
                constraint: op.constraint.clone(),
                expr: self.check_expr(&op.expr, None),
            })
            .collect();
        let outputs: Vec<TypedAsmOperand> = a
            .outputs
            .iter()
            .map(|op| TypedAsmOperand {
                constraint: op.constraint.clone(),
                expr: self.check_expr(&op.expr, None),
            })
            .collect();
        TypedExpr::new(
            TypedExprKind::Asm(TypedAsmExpr {
                template: a.template.clone(),
                inputs,
                outputs,
                clobbers: a.clobbers.clone(),
            }),
            Type::Void,
        )
    }

    fn check_if_expr(&mut self, i: &ast::IfExpr) -> TypedExpr {
        let cond = self.check_expr(&i.condition, Some(&Type::Bool));
        if !cond.ty.is_unknown() && cond.ty != Type::Bool {
            self.error(format!("if condition must be bool, found `{}`", cond.ty));
        }
        let then_block = self.check_block(&i.then_block);
        let else_block = i.else_block.as_ref().map(|b| self.check_block(b));
        let ty = if let Some(else_b) = &else_block {
            if then_block.ty != else_b.ty && !then_block.ty.is_unknown() && !else_b.ty.is_unknown() {
                self.error(format!(
                    "if expression branches have incompatible types `{}` and `{}`",
                    then_block.ty, else_b.ty
                ));
            }
            if !then_block.ty.is_unknown() {
                then_block.ty.clone()
            } else {
                else_b.ty.clone()
            }
        } else {
            then_block.ty.clone()
        };
        TypedExpr::new(
            TypedExprKind::If {
                cond: Box::new(cond),
                then_block,
                else_block,
            },
            ty,
        )
    }

    fn check_match_expr(&mut self, m: &ast::MatchExpr) -> TypedExpr {
        let scrutinee = self.check_expr(&m.scrutinee, None);
        let mut cases = Vec::new();
        let mut result_ty = Type::Unknown;
        for case in &m.cases {
            self.push_scope();
            self.check_pattern(&case.pattern, &scrutinee.ty);
            let body = self.check_block(&case.body);
            self.pop_scope();
            if result_ty.is_unknown() && !body.ty.is_unknown() {
                result_ty = body.ty.clone();
            } else if !result_ty.is_unknown() && !body.ty.is_unknown() && result_ty != body.ty {
                self.error(format!(
                    "match arm has type `{}`, expected `{}`",
                    body.ty, result_ty
                ));
            }
            cases.push(TypedMatchCase {
                pattern: self.lower_pattern(&case.pattern),
                body,
            });
        }
        self.check_match_exhaustive(&scrutinee.ty, &cases);
        TypedExpr::new(
            TypedExprKind::Match {
                scrutinee: Box::new(scrutinee),
                cases,
            },
            result_ty,
        )
    }

    fn check_match_exhaustive(&mut self, scrutinee_ty: &Type, cases: &[TypedMatchCase]) {
        let enum_name = match base_type_name(scrutinee_ty).as_str() {
            "" => return,
            name => name.to_string(),
        };
        let info = match self.adts.get(&enum_name) {
            Some(i) if i.kind == AdtKind::Enum => i.clone(),
            _ => return,
        };

        let mut covered: HashSet<String> = HashSet::new();
        let mut has_wildcard = false;
        for case in cases {
            match &case.pattern {
                TypedPattern::Wildcard => has_wildcard = true,
                TypedPattern::Ident(name) => {
                    if info.variants.iter().any(|v| &v.name == name) {
                        covered.insert(name.clone());
                    } else {
                        // A bare identifier that is not a variant acts like a binding wildcard.
                        has_wildcard = true;
                    }
                }
                _ => {}
            }
        }

        if !has_wildcard {
            for v in &info.variants {
                if !covered.contains(&v.name) {
                    self.error(format!(
                        "non-exhaustive match: missing variant `{}` of enum `{}`",
                        v.name, info.name
                    ));
                    return;
                }
            }
        }
    }

    fn is_mutable_lvalue(&self, expr: &TypedExpr) -> bool {
        match &expr.kind {
            TypedExprKind::Ident(name) => self
                .lookup_var(name)
                .map(|v| v.mutable)
                .unwrap_or(false),
            TypedExprKind::Field { object, .. } => self.is_mutable_lvalue(object),
            TypedExprKind::Index { object, .. } => self.is_mutable_lvalue(object),
            TypedExprKind::Deref(operand) | TypedExprKind::Unary { op: UnOp::Deref, operand } => {
                if self.in_unsafe {
                    operand.ty.is_pointer() || operand.ty.is_mutable_reference()
                } else {
                    operand.ty.is_mutable_reference()
                }
            }
            _ => false,
        }
    }

    fn expect_type(&mut self, expected: &Type, got: &Type, context: &str) {
        if !got.is_unknown() && !expected.is_unknown() && got != expected {
            self.error(format!("{} expected `{}`, found `{}`", context, expected, got));
        }
    }
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn zero_expr(ty: &Type) -> TypedExpr {
    let kind = match ty {
        Type::Bool => TypedExprKind::Literal(Literal::Bool(false)),
        Type::Char => TypedExprKind::Literal(Literal::Char('\0')),
        Type::Pointer { .. } | Type::USize | Type::ISize => TypedExprKind::Literal(Literal::Null),
        _ => TypedExprKind::Literal(Literal::Int(0)),
    };
    TypedExpr::new(kind, ty.clone())
}

fn primitive_type(name: &str) -> Option<Type> {
    match name {
        "void" => Some(Type::Void),
        "bool" => Some(Type::Bool),
        "char" => Some(Type::Char),
        "usize" => Some(Type::USize),
        "isize" => Some(Type::ISize),
        "i8" | "int8" => Some(Type::Int { width: 8, signed: true }),
        "i16" | "int16" => Some(Type::Int { width: 16, signed: true }),
        "i32" | "int" | "int32" => Some(Type::Int { width: 32, signed: true }),
        "i64" | "int64" => Some(Type::Int { width: 64, signed: true }),
        "i128" | "int128" => Some(Type::Int { width: 128, signed: true }),
        "u8" | "uint8" => Some(Type::Int { width: 8, signed: false }),
        "u16" | "uint16" => Some(Type::Int { width: 16, signed: false }),
        "u32" | "uint32" => Some(Type::Int { width: 32, signed: false }),
        "u64" | "uint64" => Some(Type::Int { width: 64, signed: false }),
        "u128" | "uint128" => Some(Type::Int { width: 128, signed: false }),
        "f32" | "float32" => Some(Type::Float { width: 32 }),
        "f64" | "float" | "float64" => Some(Type::Float { width: 64 }),
        _ => None,
    }
}

fn literal_type(lit: &Literal, expected: Option<&Type>) -> Type {
    match lit {
        Literal::Int(_) => expected
            .filter(|t| t.is_integer())
            .cloned()
            .unwrap_or(Type::Int { width: 32, signed: true }),
        Literal::Float(_) => expected
            .filter(|t| t.is_float())
            .cloned()
            .unwrap_or(Type::Float { width: 64 }),
        Literal::Bool(_) => Type::Bool,
        Literal::Char(_) => Type::Char,
        Literal::String(_) => Type::pointer(Type::Char),
        Literal::Null => expected
            .filter(|t| t.is_pointer())
            .cloned()
            .unwrap_or(Type::pointer(Type::Unknown)),
    }
}

fn adt_type(info: &AdtInfo) -> Type {
    match info.kind {
        AdtKind::Struct => Type::Struct {
            name: info.name.clone(),
            fields: info.fields.clone(),
        },
        AdtKind::Union => Type::Union {
            name: info.name.clone(),
            fields: info.fields.clone(),
        },
        AdtKind::Enum => Type::Enum {
            name: info.name.clone(),
            variants: info.variants.clone(),
        },
    }
}

fn base_type_name(ty: &Type) -> String {
    match ty {
        Type::Struct { name, .. }
        | Type::Union { name, .. }
        | Type::Enum { name, .. } => name.clone(),
        Type::Pointer { pointee } => base_type_name(pointee),
        Type::Ref { pointee } | Type::RefMut { pointee } | Type::Own { pointee } => base_type_name(pointee),
        _ => String::new(),
    }
}

fn base_type_name_from_type_expr(tx: &TypeExpr) -> String {
    match tx {
        TypeExpr::Name(name) => name.clone(),
        TypeExpr::Pointer(inner) | TypeExpr::Slice(inner) => base_type_name_from_type_expr(inner),
        _ => String::new(),
    }
}

fn adt_name_from_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Struct { name, .. }
        | Type::Union { name, .. }
        | Type::Enum { name, .. } => Some(name.clone()),
        Type::Pointer { pointee } => adt_name_from_type(pointee),
        Type::Ref { pointee } | Type::RefMut { pointee } => adt_name_from_type(pointee),
        _ => None,
    }
}

fn field_index(ty: &Type, field: &str, ctx: &Context) -> usize {
    let name = base_type_name(ty);
    if let Some(info) = ctx.adts.get(&name) {
        return info.fields.iter().position(|f| f.name == field).unwrap_or(0);
    }
    0
}

fn op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Assign => "=",
        BinOp::FloorDiv => "//",
        BinOp::Power => "**",
    }
}

fn cast_allowed(from: &Type, to: &Type) -> bool {
    if from == to || from.is_unknown() || to.is_unknown() {
        return true;
    }
    match (from, to) {
        (a, b) if a.is_numeric() && b.is_numeric() => true,
        (Type::Pointer { .. }, Type::Pointer { .. }) => true,
        (Type::Pointer { .. }, Type::Int { .. }) | (Type::Int { .. }, Type::Pointer { .. }) => true,
        (Type::Pointer { .. }, Type::USize) | (Type::USize, Type::Pointer { .. }) => true,
        (Type::Pointer { .. }, Type::ISize) | (Type::ISize, Type::Pointer { .. }) => true,
        (Type::Bool, Type::Int { .. }) | (Type::Int { .. }, Type::Bool) => true,
        _ => false,
    }
}

fn compatible(expected: &Type, got: &Type) -> bool {
    if expected == got || expected.is_unknown() || got.is_unknown() {
        return true;
    }
    if matches!(expected, Type::Generic { .. }) {
        return true;
    }
    // Allow reference/owned values to coerce to raw pointers, and permit
    // layout-compatible 8-bit pointees (char/uint8/int8) to intermix.
    if let Type::Pointer { pointee: ep } = expected {
        let got_pointee = match got {
            Type::Pointer { pointee } => Some(pointee.as_ref()),
            Type::Ref { pointee } | Type::RefMut { pointee } | Type::Own { pointee } => {
                Some(pointee.as_ref())
            }
            _ => None,
        };
        if let Some(gp) = got_pointee {
            if **ep == *gp {
                return true;
            }
            if is_layout_compatible_8bit(ep, gp) {
                return true;
            }
        }
    }
    false
}

fn is_layout_compatible_8bit(a: &Type, b: &Type) -> bool {
    let is_8bit_integer = |t: &Type| match t {
        Type::Int { width: 8, .. } | Type::Char | Type::Bool => true,
        _ => false,
    };
    is_8bit_integer(a) && is_8bit_integer(b)
}

fn infer_generic_params(
    param_tys: &[Type],
    arg_tys: &[Type],
) -> Option<HashMap<String, Type>> {
    let mut mapping = HashMap::new();
    for (p, a) in param_tys.iter().zip(arg_tys.iter()) {
        collect_substitutions(p, a, &mut mapping)?;
    }
    Some(mapping)
}

fn collect_substitutions(pattern: &Type, concrete: &Type, map: &mut HashMap<String, Type>) -> Option<()> {
    match (pattern, concrete) {
        (Type::Generic { name }, _) => {
            map.insert(name.clone(), concrete.clone());
            Some(())
        }
        (Type::Pointer { pointee: p }, Type::Pointer { pointee: c })
        | (Type::Own { pointee: p }, Type::Own { pointee: c })
        | (Type::Ref { pointee: p }, Type::Ref { pointee: c })
        | (Type::RefMut { pointee: p }, Type::RefMut { pointee: c }) => collect_substitutions(p, c, map),
        (Type::Slice { elem: p }, Type::Slice { elem: c }) => collect_substitutions(p, c, map),
        (Type::Array { elem: p, size: ps }, Type::Array { elem: c, size: cs }) if ps == cs => {
            collect_substitutions(p, c, map)
        }
        (Type::Tuple { fields: pf }, Type::Tuple { fields: cf }) if pf.len() == cf.len() => {
            for (p, c) in pf.iter().zip(cf.iter()) {
                collect_substitutions(p, c, map)?;
            }
            Some(())
        }
        (Type::Function { params: pp, ret: pr }, Type::Function { params: cp, ret: cr })
            if pp.len() == cp.len() =>
        {
            for (p, c) in pp.iter().zip(cp.iter()) {
                collect_substitutions(p, c, map)?;
            }
            collect_substitutions(pr, cr, map)
        }
        (a, b) if a == b || a.is_unknown() || b.is_unknown() => Some(()),
        _ => None,
    }
}

fn substitute(ty: &Type, mapping: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Generic { name } => mapping.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Pointer { pointee } => Type::pointer(substitute(pointee, mapping)),
        Type::Own { pointee } => Type::own(substitute(pointee, mapping)),
        Type::Ref { pointee } => Type::refr(substitute(pointee, mapping)),
        Type::RefMut { pointee } => Type::ref_mut(substitute(pointee, mapping)),
        Type::Slice { elem } => Type::slice(substitute(elem, mapping)),
        Type::Array { elem, size } => Type::array(substitute(elem, mapping), *size),
        Type::Tuple { fields } => Type::tuple(fields.iter().map(|f| substitute(f, mapping)).collect()),
        Type::Function { params, ret } => Type::function(
            params.iter().map(|p| substitute(p, mapping)).collect(),
            substitute(ret, mapping),
        ),
        _ => ty.clone(),
    }
}

// -----------------------------------------------------------------------------
// AdtDefinition trait to share struct/union registration code
// -----------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::check;
    use crate::sema::ast::{self, *};
    use crate::sema::typed::{TypedItem, TypedModule};
    use crate::ty::Type;

    fn typed_mod(items: Vec<Item>) -> TypedModule {
        check(ast::Module {
            package: "test".to_string(),
            imports: Vec::new(),
            items,
        })
    }

    fn func(name: &str, params: Vec<(&str, TypeExpr)>, ret: Option<TypeExpr>, body: Block) -> Item {
        Item::Function(Function {
            attrs: Vec::new(),
            vis: Visibility::Private,
            unsafe_kw: false,
            name: name.to_string(),
            generics: Vec::new(),
            params: params
                .into_iter()
                .map(|(n, t)| Param { name: n.to_string(), ty: t })
                .collect(),
            ret,
            body: Some(body),
        })
    }

    fn body(expr: Expr) -> Block {
        Block { stmts: vec![Stmt::Expr(expr)] }
    }

    fn ret(expr: Expr) -> Stmt {
        Stmt::Return(Some(expr))
    }

    fn var_init(name: &str, value: Expr) -> Stmt {
        Stmt::Var(VarStmt {
            name: name.to_string(),
            ty: None,
            value: Some(value),
        })
    }

    fn let_init(name: &str, value: Expr) -> Stmt {
        Stmt::Let(LetStmt {
            name: name.to_string(),
            ty: None,
            value: Some(value),
        })
    }

    fn assign(target: Expr, value: Expr) -> Stmt {
        Stmt::Assign(AssignStmt { target, value })
    }

    fn ident(s: &str) -> Expr {
        Expr::Ident(s.to_string())
    }

    fn int(n: i64) -> Expr {
        Expr::Literal(Literal::Int(n))
    }

    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::Binary(BinaryExpr { op, left: Box::new(l), right: Box::new(r) })
    }

    fn call(callee: Expr, args: Vec<Expr>) -> Expr {
        Expr::Call(CallExpr { callee: Box::new(callee), args })
    }

    fn ty_name(s: &str) -> TypeExpr {
        TypeExpr::Name(s.to_string())
    }

    #[test]
    fn empty_module_has_no_errors() {
        let m = typed_mod(Vec::new());
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        assert!(m.items.is_empty());
    }

    #[test]
    fn simple_function_return_type() {
        let items = vec![func(
            "main",
            Vec::new(),
            Some(ty_name("i32")),
            Block { stmts: vec![ret(int(0))] },
        )];
        let m = typed_mod(items);
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        if let TypedItem::Function(f) = &m.items[0] {
            assert_eq!(f.ret, Type::int(32, true));
        } else {
            panic!("expected function");
        }
    }

    #[test]
    fn mutability_let_vs_var() {
        let items = vec![func(
            "main",
            Vec::new(),
            None,
            Block {
                stmts: vec![
                    let_init("x", int(1)),
                    var_init("y", int(2)),
                    assign(ident("x"), int(3)), // error: immutable
                    assign(ident("y"), int(4)), // ok
                ],
            },
        )];
        let m = typed_mod(items);
        let msgs: Vec<String> = m.errors.iter().map(|e| e.message.clone()).collect();
        assert!(msgs.iter().any(|s| s.contains("cannot assign to immutable")));
    }

    #[test]
    fn type_mismatch_reported() {
        let items = vec![func(
            "main",
            Vec::new(),
            Some(ty_name("i32")),
            Block {
                stmts: vec![Stmt::Let(LetStmt {
                    name: "b".to_string(),
                    ty: Some(ty_name("bool")),
                    value: Some(int(1)),
                })],
            },
        )];
        let m = typed_mod(items);
        assert!(m.errors.iter().any(|e| e.message.contains("expected `bool`")));
    }

    #[test]
    fn generic_function_monomorphized() {
        let generic_fn = Function {
            attrs: Vec::new(),
            vis: Visibility::Private,
            unsafe_kw: false,
            name: "identity".to_string(),
            generics: vec!["T".to_string()],
            params: vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Name("T".to_string()),
            }],
            ret: Some(TypeExpr::Name("T".to_string())),
            body: Some(Block { stmts: vec![ret(ident("x"))] }),
        };
        let call_site = call(ident("identity"), vec![int(42)]);
        let items = vec![
            Item::Function(generic_fn),
            func("use_it", Vec::new(), Some(ty_name("i32")), body(call_site)),
        ];
        let m = typed_mod(items);
        for e in &m.errors {
            println!("err: {}", e.message);
        }
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        assert!(!m.mono_instances.is_empty());
        assert!(m.mono_instances.iter().any(|mi| mi.function_name == "identity"));
    }

    #[test]
    fn unsafe_raw_deref_requires_unsafe() {
        let items = vec![func(
            "main",
            vec![("p", TypeExpr::Pointer(Box::new(ty_name("i32"))))],
            None,
            Block {
                stmts: vec![Stmt::Expr(Expr::Deref(DerefExpr {
                    expr: Box::new(ident("p")),
                }))],
            },
        )];
        let m = typed_mod(items);
        let msgs: Vec<String> = m.errors.iter().map(|e| e.message.clone()).collect();
        assert!(msgs.iter().any(|s| s.contains("raw pointer dereference requires `unsafe`")));
    }

    #[test]
    fn unsafe_union_field_requires_unsafe() {
        let union_item = Item::Union(ast::Union {
            attrs: Vec::new(),
            vis: Visibility::Private,
            name: "U".to_string(),
            generics: Vec::new(),
            fields: vec![ast::Field {
                name: "i".to_string(),
                ty: ty_name("i32"),
            }],
        });
        let access = Expr::Field(FieldExpr {
            object: Box::new(ident("u")),
            field: "i".to_string(),
        });
        let main = func("main", vec![("u", ty_name("U"))], None, body(access));
        let m = typed_mod(vec![union_item, main]);
        let msgs: Vec<String> = m.errors.iter().map(|e| e.message.clone()).collect();
        assert!(msgs.iter().any(|s| s.contains("union field access requires `unsafe`")));
    }

    #[test]
    fn cast_numeric_ok() {
        let items = vec![func(
            "main",
            Vec::new(),
            Some(ty_name("f64")),
            body(Expr::Cast(CastExpr {
                expr: Box::new(int(1)),
                ty: Box::new(ty_name("f64")),
            })),
        )];
        let m = typed_mod(items);
        assert!(m.errors.is_empty(), "{:?}", m.errors);
    }

    #[test]
    fn if_condition_must_be_bool() {
        let items = vec![func(
            "main",
            Vec::new(),
            None,
            Block {
                stmts: vec![Stmt::If(IfStmt {
                    condition: Expr::Literal(Literal::String("not bool".to_string())),
                    then_block: Block { stmts: vec![] },
                    elifs: Vec::new(),
                    else_block: None,
                })],
            },
        )];
        let m = typed_mod(items);
        assert!(m.errors.iter().any(|e| e.message.contains("if condition must be bool")));
    }

    #[test]
    fn unsafe_raw_deref_allowed_in_unsafe_block() {
        let items = vec![func(
            "main",
            vec![("p", TypeExpr::Pointer(Box::new(ty_name("i32"))))],
            None,
            Block {
                stmts: vec![Stmt::UnsafeBlock(Block {
                    stmts: vec![Stmt::Expr(Expr::Deref(DerefExpr {
                        expr: Box::new(ident("p")),
                    }))],
                })],
            },
        )];
        let m = typed_mod(items);
        assert!(m.errors.is_empty(), "{}", format_errors(&m.errors));
    }

    #[test]
    fn refmut_requires_mutable_place() {
        let items = vec![func(
            "main",
            Vec::new(),
            None,
            Block {
                stmts: vec![
                    let_init("x", int(0)),
                    Stmt::Expr(Expr::RefMut(RefMutExpr {
                        expr: Box::new(ident("x")),
                    })),
                ],
            },
        )];
        let m = typed_mod(items);
        assert!(m.errors.iter().any(|e| e.message.contains("mutable reference")));
    }

    #[test]
    fn non_exhaustive_enum_match() {
        let color = Item::Enum(Enum {
            attrs: Vec::new(),
            vis: Visibility::Private,
            name: "Color".to_string(),
            generics: Vec::new(),
            variants: vec![
                Variant { name: "Red".to_string(), payload: None },
                Variant { name: "Green".to_string(), payload: None },
                Variant { name: "Blue".to_string(), payload: None },
            ],
        });
        let body = Block {
            stmts: vec![Stmt::Expr(Expr::Match(MatchExpr {
                scrutinee: Box::new(ident("c")),
                cases: vec![
                    MatchCase {
                        pattern: Pattern::Ident("Red".to_string()),
                        body: Block { stmts: vec![Stmt::Expr(int(0))] },
                    },
                    MatchCase {
                        pattern: Pattern::Ident("Green".to_string()),
                        body: Block { stmts: vec![Stmt::Expr(int(1))] },
                    },
                ],
            }))],
        };
        let main = func("main", vec![("c", ty_name("Color"))], None, body);
        let m = typed_mod(vec![color, main]);
        assert!(m.errors.iter().any(|e| e.message.contains("non-exhaustive")));
    }

    #[test]
    fn pointer_arithmetic_requires_unsafe() {
        let items = vec![func(
            "main",
            vec![("p", TypeExpr::Pointer(Box::new(ty_name("i32"))))],
            None,
            Block {
                stmts: vec![Stmt::Expr(bin(BinOp::Add, ident("p"), int(1)))],
            },
        )];
        let m = typed_mod(items);
        assert!(m.errors.iter().any(|e| e.message.contains("pointer arithmetic requires `unsafe`")));
    }

    fn format_errors(errors: &[super::Error]) -> String {
        errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", ")
    }
}
