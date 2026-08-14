#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::check;
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
                .map(|(n, t)| Param {
                    name: n.to_string(),
                    ty: t,
                })
                .collect(),
            ret,
            body: Some(body),
        })
    }

    fn body(expr: Expr) -> Block {
        Block {
            stmts: vec![Stmt::Expr(expr)],
        }
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
        Expr::Binary(BinaryExpr {
            span: Span::unknown(),
            op,
            left: Box::new(l),
            right: Box::new(r),
        })
    }

    fn call(callee: Expr, args: Vec<Expr>) -> Expr {
        Expr::Call(CallExpr {
            span: Span::unknown(),
            callee: Box::new(callee),
            args,
        })
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
            Block {
                stmts: vec![ret(int(0))],
            },
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
        assert!(
            msgs.iter()
                .any(|s| s.contains("cannot assign to immutable"))
        );
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
        assert!(
            m.errors
                .iter()
                .any(|e| e.message.contains("expected `bool`"))
        );
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
            body: Some(Block {
                stmts: vec![ret(ident("x"))],
            }),
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
        assert!(
            m.mono_instances
                .iter()
                .any(|mi| mi.function_name == "identity")
        );
    }

    #[test]
    fn unsafe_raw_deref_requires_unsafe() {
        let items = vec![func(
            "main",
            vec![("p", TypeExpr::Pointer(Box::new(ty_name("i32"))))],
            None,
            Block {
                stmts: vec![Stmt::Expr(Expr::Deref(DerefExpr {
                    span: Span::unknown(),
                    expr: Box::new(ident("p")),
                }))],
            },
        )];
        let m = typed_mod(items);
        let msgs: Vec<String> = m.errors.iter().map(|e| e.message.clone()).collect();
        assert!(
            msgs.iter()
                .any(|s| s.contains("raw pointer dereference requires `unsafe`"))
        );
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
            span: Span::unknown(),
            object: Box::new(ident("u")),
            field: "i".to_string(),
        });
        let main = func("main", vec![("u", ty_name("U"))], None, body(access));
        let m = typed_mod(vec![union_item, main]);
        let msgs: Vec<String> = m.errors.iter().map(|e| e.message.clone()).collect();
        assert!(
            msgs.iter()
                .any(|s| s.contains("union field access requires `unsafe`"))
        );
    }

    #[test]
    fn cast_numeric_ok() {
        let items = vec![func(
            "main",
            Vec::new(),
            Some(ty_name("f64")),
            body(Expr::Cast(CastExpr {
                span: Span::unknown(),
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
        assert!(
            m.errors
                .iter()
                .any(|e| e.message.contains("if condition must be bool"))
        );
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
                        span: Span::unknown(),
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
                        span: Span::unknown(),
                        expr: Box::new(ident("x")),
                    })),
                ],
            },
        )];
        let m = typed_mod(items);
        assert!(
            m.errors
                .iter()
                .any(|e| e.message.contains("mutable reference"))
        );
    }

    #[test]
    fn non_exhaustive_enum_match() {
        let color = Item::Enum(Enum {
            attrs: Vec::new(),
            vis: Visibility::Private,
            name: "Color".to_string(),
            generics: Vec::new(),
            variants: vec![
                Variant {
                    name: "Red".to_string(),
                    payload: None,
                },
                Variant {
                    name: "Green".to_string(),
                    payload: None,
                },
                Variant {
                    name: "Blue".to_string(),
                    payload: None,
                },
            ],
        });
        let body = Block {
            stmts: vec![Stmt::Expr(Expr::Match(MatchExpr {
                span: Span::unknown(),
                scrutinee: Box::new(ident("c")),
                cases: vec![
                    MatchCase {
                        pattern: Pattern::Ident("Red".to_string()),
                        body: Block {
                            stmts: vec![Stmt::Expr(int(0))],
                        },
                    },
                    MatchCase {
                        pattern: Pattern::Ident("Green".to_string()),
                        body: Block {
                            stmts: vec![Stmt::Expr(int(1))],
                        },
                    },
                ],
            }))],
        };
        let main = func("main", vec![("c", ty_name("Color"))], None, body);
        let m = typed_mod(vec![color, main]);
        assert!(
            m.errors
                .iter()
                .any(|e| e.message.contains("non-exhaustive"))
        );
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
        assert!(
            m.errors
                .iter()
                .any(|e| e.message.contains("pointer arithmetic requires `unsafe`"))
        );
    }

    fn format_errors(errors: &[super::super::Error]) -> String {
        errors
            .iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
