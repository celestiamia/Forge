use super::typing::*;
use super::*;

impl Context {
    pub(super) fn check_item(&mut self, item: &Item) -> TypedItem {
        match item {
            Item::Function(f) => TypedItem::Function(self.check_function(f)),
            Item::Struct(s) => TypedItem::Struct {
                name: s.name.clone(),
                generics: s.generics.clone(),
                fields: self
                    .adts
                    .get(&s.name)
                    .map(|i| i.fields.clone())
                    .unwrap_or_default(),
            },
            Item::Union(u) => TypedItem::Union {
                name: u.name.clone(),
                generics: u.generics.clone(),
                fields: self
                    .adts
                    .get(&u.name)
                    .map(|i| i.fields.clone())
                    .unwrap_or_default(),
            },
            Item::Enum(e) => TypedItem::Enum {
                name: e.name.clone(),
                generics: e.generics.clone(),
                variants: self
                    .adts
                    .get(&e.name)
                    .map(|i| i.variants.clone())
                    .unwrap_or_default(),
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
                let ty =
                    c.ty.as_ref()
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
            Item::Embed(e) => TypedItem::Embed {
                name: e.name.clone(),
                len: e.data.len(),
            },
            Item::Use(u) => TypedItem::Use {
                path: u.path.clone(),
                alias: u.alias.clone(),
            },
            Item::Impl(i) => {
                let target = self.resolve_type_expr(&i.target);
                let target_name = base_type_name_from_type_expr(&i.target);
                let sigs = self.methods.get(&target_name).cloned().unwrap_or_default();
                let methods: Vec<TypedFunction> = i
                    .methods
                    .iter()
                    .map(|m| {
                        let sig = sigs
                            .iter()
                            .find(|s| s.name == m.name)
                            .cloned()
                            .unwrap_or_else(|| FnSig {
                                name: m.name.clone(),
                                generics: m.generics.clone(),
                                params: Vec::new(),
                                ret: Type::Unknown,
                                is_unsafe: m.unsafe_kw,
                                has_body: true,
                            });
                        self.check_function_with_sig(m, sig)
                    })
                    .collect();
                TypedItem::Impl { target, methods }
            }
        }
    }

    pub(super) fn check_function(&mut self, f: &ast::Function) -> TypedFunction {
        let sig = match self
            .functions
            .get(&f.name)
            .or_else(|| self.extern_fns.get(&f.name))
        {
            Some(s) => s.clone(),
            None => {
                self.error(format!(
                    "internal error: no registered signature for function `{}`",
                    f.name
                ));
                FnSig {
                    name: f.name.clone(),
                    generics: f.generics.clone(),
                    params: Vec::new(),
                    ret: Type::Unknown,
                    is_unsafe: f.unsafe_kw,
                    has_body: true,
                }
            }
        };
        self.check_function_with_sig(f, sig)
    }

    fn check_function_with_sig(&mut self, f: &ast::Function, sig: FnSig) -> TypedFunction {
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
    pub(super) fn block_definitely_returns(block: &TypedBlock) -> bool {
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
                TypedStmt::Loop(body) if is_last && Self::block_contains_return(body) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub(super) fn is_infinite_cond(cond: &TypedExpr) -> bool {
        matches!(
            cond.kind,
            TypedExprKind::Literal(Literal::Bool(true)) | TypedExprKind::Literal(Literal::Int(1))
        )
    }

    /// Return true if the block contains a `return` statement at any nesting level.
    pub(super) fn block_contains_return(block: &TypedBlock) -> bool {
        for stmt in &block.stmts {
            match stmt {
                TypedStmt::Return(_) => return true,
                TypedStmt::UnsafeBlock(b)
                | TypedStmt::If {
                    then_block: b,
                    else_block: None,
                    ..
                } => {
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
                TypedStmt::While { body, .. }
                | TypedStmt::For { body, .. }
                | TypedStmt::Loop(body) => {
                    if Self::block_contains_return(body) {
                        return true;
                    }
                }
                TypedStmt::Match { cases, .. }
                    if cases.iter().any(|c| Self::block_contains_return(&c.body)) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    // Scopes

    pub(super) fn check_block(&mut self, block: &Block) -> TypedBlock {
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

    pub(super) fn check_stmt(&mut self, stmt: &Stmt) -> TypedStmt {
        match stmt {
            Stmt::Let(l) => self.check_let_var(&l.pattern, &l.ty, &l.value, false),
            Stmt::Var(v) => self.check_let_var(&v.pattern, &v.ty, &v.value, true),
            Stmt::Assign(a) => self.check_assign(a),
            Stmt::CompoundAssign(c) => self.check_compound_assign(c),
            Stmt::Expr(e) => TypedStmt::Expr(self.check_expr(e, None)),
            Stmt::Return(e) => self.check_return(e),
            Stmt::If(i) => self.check_if(i),
            Stmt::For(f) => self.check_for(f),
            Stmt::While(w) => self.check_while(w),
            Stmt::Match(m) => self.check_match(m),
            Stmt::UnsafeBlock(b) => self.check_unsafe_block(b),
            Stmt::Loop(b) => TypedStmt::Loop(self.check_block(b)),
            Stmt::Break => TypedStmt::Break,
            Stmt::Continue => TypedStmt::Continue,
        }
    }

    fn check_let_var(
        &mut self,
        pattern: &ast::Pattern,
        ty_opt: &Option<ast::TypeExpr>,
        value_opt: &Option<ast::Expr>,
        mutable: bool,
    ) -> TypedStmt {
        let annotated = ty_opt.as_ref().map(|t| self.resolve_type_expr(t));
        let (init, ty) = if let Some(value) = value_opt {
            let init = self.check_expr(value, annotated.as_ref());
            let ty = annotated.unwrap_or_else(|| init.ty.clone());
            if !init.ty.is_unknown() && !ty.is_unknown() && init.ty != ty {
                let pat_str = pattern_binding_name(pattern);
                self.error(format!(
                    "`{} {}` expected `{}`, found `{}`",
                    if mutable { "var" } else { "let" },
                    pat_str,
                    ty,
                    init.ty
                ));
            }
            (init, ty)
        } else {
            let pat_str = pattern_binding_name(pattern);
            let ty = annotated.unwrap_or_else(|| {
                self.error(format!(
                    "`{} {}` needs a type annotation or initializer",
                    if mutable { "var" } else { "let" },
                    pat_str
                ));
                Type::Unknown
            });
            (zero_expr(&ty), ty)
        };
        self.check_pattern(pattern, &ty, mutable);
        let pat = self.lower_pattern(pattern);
        if mutable {
            TypedStmt::Var {
                pattern: pat,
                ty,
                init,
            }
        } else {
            TypedStmt::Let {
                pattern: pat,
                ty,
                init,
                mutable: false,
            }
        }
    }

    fn check_bool_cond(&mut self, cond_expr: &ast::Expr, context: &str) -> TypedExpr {
        let cond = self.check_expr(cond_expr, Some(&Type::Bool));
        if !cond.ty.is_unknown() && cond.ty != Type::Bool {
            self.error(format!(
                "{} condition must be bool, found `{}`",
                context, cond.ty
            ));
        }
        cond
    }

    fn check_assign(&mut self, a: &ast::AssignStmt) -> TypedStmt {
        let target = self.check_expr(&a.target, None);
        let value = self.check_expr(&a.value, Some(&target.ty));
        if !self.is_mutable_lvalue(&target) {
            self.error("cannot assign to immutable or non-lvalue expression".to_string());
        }
        if !value.ty.is_unknown() && !target.ty.is_unknown() && value.ty != target.ty {
            self.error(format!(
                "assignment expected `{}`, found `{}`",
                target.ty, value.ty
            ));
        }
        TypedStmt::Assign { target, value }
    }

    fn check_compound_assign(&mut self, c: &ast::CompoundAssignStmt) -> TypedStmt {
        let target = self.check_expr(&c.target, None);
        // The RHS must be type-compatible with the target (e.g. `x += 1`
        // where x is int32 requires an int32 RHS, modulo usual numeric
        // coercion).
        let value = self.check_expr(&c.value, Some(&target.ty));
        if !self.is_mutable_lvalue(&target) {
            self.error(format!(
                "cannot use `{}` on non-mutable or non-lvalue expression",
                ast_binop_symbol(c.op)
            ));
        }
        if !value.ty.is_unknown() && !target.ty.is_unknown() && value.ty != target.ty {
            self.error(format!(
                "`{}` expected `{}`, found `{}`",
                ast_binop_symbol(c.op),
                target.ty,
                value.ty
            ));
        }
        TypedStmt::CompoundAssign {
            target,
            op: c.op,
            value,
        }
    }

    fn check_return(&mut self, e: &Option<ast::Expr>) -> TypedStmt {
        let ret = self.return_type.clone().unwrap_or(Type::Unknown);
        let value = e.as_ref().map(|v| self.check_expr(v, Some(&ret)));
        if let Some(v) = &value {
            if !v.ty.is_unknown() && !ret.is_unknown() && v.ty != ret {
                self.error(format!("return expected `{}`, found `{}`", ret, v.ty));
            }
        } else if !ret.is_void() && !ret.is_unknown() {
            self.error("missing return value".to_string());
        }
        TypedStmt::Return(value)
    }

    fn check_if(&mut self, i: &ast::IfStmt) -> TypedStmt {
        let cond = self.check_bool_cond(&i.condition, "if");
        let then_block = self.check_block(&i.then_block);
        let elifs: Vec<(TypedExpr, TypedBlock)> = i
            .elifs
            .iter()
            .map(|(c, b)| {
                let tc = self.check_bool_cond(c, "elif");
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

    fn check_while(&mut self, w: &ast::WhileStmt) -> TypedStmt {
        let cond = self.check_bool_cond(&w.condition, "while");
        let body = self.check_block(&w.body);
        TypedStmt::While { cond, body }
    }

    fn check_for(&mut self, f: &ast::ForStmt) -> TypedStmt {
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

    fn check_match(&mut self, m: &ast::MatchStmt) -> TypedStmt {
        let scrutinee = self.check_expr(&m.scrutinee, None);
        let mut cases = Vec::new();
        for case in &m.cases {
            self.push_scope();
            self.check_pattern(&case.pattern, &scrutinee.ty, false);
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

    fn check_unsafe_block(&mut self, b: &ast::Block) -> TypedStmt {
        let prev = self.in_unsafe;
        self.in_unsafe = true;
        let block = self.check_block(b);
        self.in_unsafe = prev;
        TypedStmt::UnsafeBlock(block)
    }

    pub(super) fn iter_element_type(&mut self, ty: &Type) -> Type {
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

    pub(super) fn check_pattern(&mut self, pat: &Pattern, ty: &Type, mutable: bool) {
        match pat {
            Pattern::Wildcard => {}
            Pattern::Literal(l) => {
                let lit_ty = literal_type(l, Some(ty));
                if !lit_ty.is_unknown() && !ty.is_unknown() && lit_ty != *ty {
                    self.error(format!(
                        "pattern literal type `{}` does not match `{}`",
                        lit_ty, ty
                    ));
                }
            }
            Pattern::Ident(name) => {
                self.bind_var(name, ty.clone(), mutable);
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
                            self.check_pattern(p, f, mutable);
                        }
                    }
                } else if !ty.is_unknown() {
                    self.error(format!(
                        "cannot match tuple pattern against non-tuple type `{}`",
                        ty
                    ));
                }
            }
        }
    }

    pub(super) fn lower_pattern(&self, pat: &Pattern) -> TypedPattern {
        match pat {
            Pattern::Wildcard => TypedPattern::Wildcard,
            Pattern::Literal(l) => TypedPattern::Literal(l.clone()),
            Pattern::Ident(name) => TypedPattern::Ident(name.clone()),
            Pattern::Tuple(pats) => {
                TypedPattern::Tuple(pats.iter().map(|p| self.lower_pattern(p)).collect())
            }
        }
    }

    // Expressions
}

fn pattern_binding_name(pat: &Pattern) -> String {
    match pat {
        Pattern::Ident(name) => name.clone(),
        Pattern::Tuple(pats) => {
            let names: Vec<String> = pats.iter().map(pattern_binding_name).collect();
            format!("({})", names.join(", "))
        }
        Pattern::Wildcard => "_".to_string(),
        Pattern::Literal(_) => "<literal>".to_string(),
    }
}
