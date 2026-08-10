use super::*;
use super::typing::*;

impl Context {
    pub(super) fn check_expr(&mut self, expr: &Expr, expected: Option<&Type>) -> TypedExpr {
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
                // When the expected type is a pointer, treat the array literal
                // as a pointer to the element type (matching the IR representation).
                let is_ptr_expected = matches!(expected, Some(t) if t.is_pointer());
                if let Some(Type::Array { elem: expected_elem, .. }) = expected {
                    if !is_ptr_expected {
                        elem_ty = *expected_elem.clone();
                        for e in a {
                            let te = self.check_expr(e, Some(&elem_ty));
                            elems.push(te);
                        }
                    } else {
                        elem_ty = *expected_elem.clone();
                        for e in a {
                            let te = self.check_expr(e, Some(&elem_ty));
                            elems.push(te);
                        }
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
                let ty = if is_ptr_expected {
                    // Find the pointee type from the expected pointer
                    match expected {
                        Some(t) if t.is_pointer() => {
                            // Extract the inner type from the pointer
                            if let Type::Pointer { pointee } = t {
                                Type::pointer(*pointee.clone())
                            } else if let Type::Ref { pointee } = t {
                                Type::pointer(*pointee.clone())
                            } else {
                                Type::pointer(elem_ty.clone())
                            }
                        }
                        _ => Type::pointer(elem_ty.clone()),
                    }
                } else {
                    Type::array(elem_ty.clone(), elems.len() as u64)
                };
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
                let struct_info = self.adts.get(name).cloned();
                if let Some(info) = struct_info {
                    if info.kind != AdtKind::Struct {
                        self.error(format!("`{}` is not a struct type", name));
                    }
                    let mut field_values = Vec::new();
                    for (fname, expr) in fields {
                        if let Some(finfo) = info.fields.iter().find(|f| &f.name == fname) {
                            let te = self.check_expr(expr, Some(&finfo.ty));
                            if !compatible(&finfo.ty, &te.ty) && !te.ty.is_unknown() {
                                self.error(format!(
                                    "field `{}` expected `{}`, found `{}`",
                                    fname, finfo.ty, te.ty
                                ));
                            }
                            field_values.push((fname.clone(), te));
                        } else {
                            self.error(format!("struct `{}` has no field `{}`", name, fname));
                            field_values.push((fname.clone(), self.check_expr(expr, None)));
                        }
                    }
                    let ty = Type::Struct {
                        name: name.clone(),
                        fields: info.fields.clone(),
                    };
                    TypedExpr::new(
                        TypedExprKind::StructLiteral { name: name.clone(), fields: field_values },
                        ty,
                    )
                } else {
                    self.error(format!("unknown struct type `{}`", name));
                    TypedExpr::new(TypedExprKind::Literal(Literal::Null), Type::Unknown)
                }
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

    pub(super) fn check_ident(&mut self, name: &str) -> TypedExpr {
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
        if let Some(static_info) = self.statics.get(name) {
            return TypedExpr::new(
                TypedExprKind::Ident(name.to_string()),
                static_info.ty.clone(),
            );
        }
        if self.adts.contains_key(name) || self.imports.contains_key(name) {
            return TypedExpr::new(TypedExprKind::Ident(name.to_string()), Type::Unknown);
        }
        self.error(format!("unknown identifier `{}`", name));
        TypedExpr::new(TypedExprKind::Ident(name.to_string()), Type::Unknown)
    }

    pub(super) fn check_binary(&mut self, b: &ast::BinaryExpr, expected: Option<&Type>) -> TypedExpr {
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

    pub(super) fn check_unary(&mut self, u: &ast::UnaryExpr, expected: Option<&Type>) -> TypedExpr {
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

    pub(super) fn check_call(&mut self, c: &ast::CallExpr, expected: Option<&Type>) -> TypedExpr {
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

    pub(super) fn resolve_call(
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

    pub(super) fn check_field(&mut self, f: &ast::FieldExpr) -> TypedExpr {
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

    pub(super) fn check_index(&mut self, i: &ast::IndexExpr) -> TypedExpr {
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

    pub(super) fn check_cast(&mut self, c: &ast::CastExpr) -> TypedExpr {
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

    pub(super) fn check_asm(&mut self, a: &ast::AsmExpr) -> TypedExpr {
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

    pub(super) fn check_if_expr(&mut self, i: &ast::IfExpr) -> TypedExpr {
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

    pub(super) fn check_match_expr(&mut self, m: &ast::MatchExpr) -> TypedExpr {
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

    pub(super) fn check_match_exhaustive(&mut self, scrutinee_ty: &Type, cases: &[TypedMatchCase]) {
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

    pub(super) fn is_mutable_lvalue(&self, expr: &TypedExpr) -> bool {
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

    pub(super) fn expect_type(&mut self, expected: &Type, got: &Type, context: &str) {
        if !got.is_unknown() && !expected.is_unknown() && got != expected {
            self.error(format!("{} expected `{}`, found `{}`", context, expected, got));
        }
    }

}
