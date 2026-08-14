#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;

    fn parse_ok(src: &str) -> Module {
        parse_module(src).unwrap()
    }

    #[test]
    fn package_only() {
        let m = parse_ok("package demo\n");
        assert_eq!(m.package, "demo");
        assert!(m.imports.is_empty());
        assert!(m.items.is_empty());
    }

    #[test]
    fn simple_function() {
        let m = parse_ok("package test\n\ndef main() -> int32:\n    return 0\n");
        assert_eq!(m.items.len(), 1);
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function");
        };
        assert_eq!(f.name, "main");
        assert!(f.params.is_empty());
        assert!(matches!(&f.ret, Some(TypeExpr::Name(n)) if n == "int32"));
        assert!(f.body.is_some());
    }

    #[test]
    fn brace_block_function() {
        let m = parse_ok("package test\n\ndef main() -> int32 {\n    return 0\n}\n");
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function");
        };
        assert_eq!(f.name, "main");
        assert!(f.body.is_some());
    }

    #[test]
    fn function_params() {
        let m = parse_ok("package test\n\ndef add(a: i32, b: i32) -> i32:\n    return a + b\n");
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function");
        };
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "a");
        assert!(matches!(&f.params[0].ty, TypeExpr::Name(n) if n == "i32"));
    }

    #[test]
    fn imports() {
        let m = parse_ok(
            "package test\nimport std.io\nfrom std.io import println, putchar\nfrom std import *\n",
        );
        assert_eq!(m.imports.len(), 3);
        assert!(matches!(
            &m.imports[0],
            Import::Path { path, alias: None } if path == &["std", "io"]
        ));
        assert!(matches!(
            &m.imports[1],
            Import::From { path, items: Some(items) }
            if path == &["std", "io"] && items == &["println", "putchar"]
        ));
        assert!(matches!(
            &m.imports[2],
            Import::From { path, items: None } if path == &["std"]
        ));
    }

    #[test]
    fn let_var_return() {
        let m = parse_ok(
            "package test\n\ndef f():\n    let x: i32 = 1\n    var y = 2\n    return x + y\n",
        );
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        let body = f.body.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 3);
        assert!(matches!(body.stmts[0], Stmt::Let(_)));
        assert!(matches!(body.stmts[1], Stmt::Var(_)));
        assert!(matches!(body.stmts[2], Stmt::Return(_)));
    }

    #[test]
    fn if_elif_else() {
        let m = parse_ok(
            "package test\n\ndef f(x: i32) -> i32:\n    if x < 0:\n        return -1\n    elif x == 0:\n        return 0\n    else:\n        return 1\n",
        );
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        let Stmt::If(ref stmt) = f.body.as_ref().unwrap().stmts[0] else {
            panic!("expected if")
        };
        assert!(!stmt.elifs.is_empty());
        assert!(stmt.else_block.is_some());
    }

    #[test]
    fn for_range() {
        let m = parse_ok("package test\n\ndef f():\n    for i in 0..10:\n        continue\n");
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        let Stmt::For(ref stmt) = f.body.as_ref().unwrap().stmts[0] else {
            panic!("expected for")
        };
        assert_eq!(stmt.var, "i");
        assert!(matches!(stmt.iter, Expr::Range(_)));
    }

    #[test]
    fn while_loop() {
        let m = parse_ok("package test\n\ndef f():\n    while true:\n        break\n");
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        let Stmt::While(ref stmt) = f.body.as_ref().unwrap().stmts[0] else {
            panic!("expected while")
        };
        assert!(matches!(stmt.condition, Expr::Literal(Literal::Bool(true))));
    }

    #[test]
    fn match_case() {
        let m = parse_ok(
            "package test\n\ndef f(x: i32) -> i32:\n    match x:\n        case 0:\n            return 0\n        case _:\n            return 1\n",
        );
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        let Stmt::Match(ref stmt) = f.body.as_ref().unwrap().stmts[0] else {
            panic!("expected match")
        };
        assert_eq!(stmt.cases.len(), 2);
    }

    #[test]
    fn expressions_operators() {
        let m = parse_ok(
            "package test\n\ndef f(a: i32, b: i32) -> i32:\n    return a + b * 2 - 3 // 2 ** 4 == 0 && a | b ^ c & d < e\n",
        );
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        assert!(matches!(f.body.as_ref().unwrap().stmts[0], Stmt::Return(_)));
    }

    #[test]
    fn call_field_index_cast() {
        let m = parse_ok(
            "package test\n\ndef f(p: ptr[uint8], arr: ptr[uint8]) -> uint8:\n    return p[0]\n",
        );
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        assert!(f.body.is_some());
    }

    #[test]
    fn sizeof_offsetof() {
        let m = parse_ok(
            "package test\n\ndef f() -> usize:\n    let a = sizeof(u32)\n    let b = offsetof(MyStruct, field)\n    return a + b\n",
        );
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        assert!(f.body.is_some());
    }

    #[test]
    fn unsafe_block() {
        let m = parse_ok("package test\n\ndef f():\n    unsafe:\n        return 1\n");
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        assert!(matches!(
            f.body.as_ref().unwrap().stmts[0],
            Stmt::UnsafeBlock(_)
        ));
    }

    #[test]
    fn attributes() {
        let m = parse_ok(
            r#"package test

@packed
@align(8)
struct Point:
    x: i32
    y: i32

@extern("c")
def puts(s: *char) -> i32:
    return 0
"#,
        );
        let Item::Struct(ref s) = m.items[0] else {
            panic!("expected struct")
        };
        assert_eq!(s.attrs.len(), 2);
        assert!(s.attrs.contains(&Attribute::Packed));
        assert!(s.attrs.contains(&Attribute::Align(8)));

        let Item::Function(ref f) = m.items[1] else {
            panic!("expected function")
        };
        assert!(f.attrs.contains(&Attribute::Extern("c".to_string())));
    }

    #[test]
    fn struct_union_enum() {
        let m = parse_ok(
            "package test\n\nstruct Point:\n    x: i32\n    y: i32\n\nunion U:\n    a: i32\n    b: f32\n\nenum Option[T]:\n    None\n    Some(T)\n",
        );
        assert!(matches!(m.items[0], Item::Struct(_)));
        assert!(matches!(m.items[1], Item::Union(_)));
        assert!(matches!(m.items[2], Item::Enum(_)));
    }

    #[test]
    fn extern_fn_abi() {
        let m = parse_ok("package test\n\nextern(\"c\") def puts(s: *char) -> i32\n");
        assert!(matches!(m.items[0], Item::ExternFn(_)));
    }

    #[test]
    fn hello_dev_example() {
        let src = r#"package hello

import std.io

pub def main() -> int32 {
    println("Hello, Forge!")
    return 0
}
"#;
        let m = parse_ok(src);
        assert_eq!(m.package, "hello");
        assert_eq!(m.imports.len(), 1);
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function")
        };
        assert_eq!(f.name, "main");
        assert!(matches!(f.vis, Visibility::Public));
        assert!(f.body.is_some());
    }

    #[test]
    fn bump_dev_example() {
        let src = r#"package bump

def add(a: int32, b: int32) -> int32:
    return a + b

extern def puts(s: ptr[char]) -> int32

def main() -> int32:
    let x: int32 = add(3, 4)
    if x == 7:
        puts("bump ok")
        return 0
    return 1
"#;
        let m = parse_ok(src);
        assert_eq!(m.items.len(), 3);
        assert!(matches!(m.items[0], Item::Function(_)));
        assert!(matches!(m.items[1], Item::ExternFn(_)));
        assert!(matches!(m.items[2], Item::Function(_)));
    }

    #[test]
    fn range_literals() {
        for src in ["0..5", "0..=5", "..5"] {
            parse_ok(&format!("package t\n\ndef f():\n    let r = {}\n", src));
        }
    }

    // Regression: a leading-`*` deref assignment on the line immediately
    // after another statement must be parsed as its own statement, not
    // swallowed as multiplication by the previous expression.  The postfix
    // chain used to skip the separating newline and then `parse_mul` would
    // grab the `*` as `BinOp::Mul`, leaving the `=` dangling.
    #[test]
    fn deref_assign_after_statement() {
        let src = "package test\n\ndef f():\n    var idx = 0\n    *(buf + idx) = 48 as char\n";
        let m = parse_ok(src);
        let Item::Function(ref f) = m.items[0] else {
            panic!("expected function");
        };
        let body = f.body.as_ref().expect("function body");
        assert_eq!(
            body.stmts.len(),
            2,
            "expected two statements, got {}",
            body.stmts.len()
        );
        assert!(
            matches!(body.stmts[0], Stmt::Var(_)),
            "first stmt should be a var"
        );
        let Stmt::Assign(ref a) = body.stmts[1] else {
            panic!("expected an assignment, got {:?}", body.stmts[1]);
        };
        assert!(
            matches!(&a.target, Expr::Deref(_)),
            "assignment target must be a deref, got {:?}",
            a.target
        );
    }

    // Same class of bug: a leading binary/prefix operator on the next line
    // (`&x`, `-x`, `*x`) must not be absorbed into the previous statement.
    #[test]
    fn leading_operator_not_absorbed() {
        for stmt in ["&x", "-x", "*x", "+x"] {
            let src = format!("package test\n\ndef f():\n    var y = 0\n    {}\n", stmt);
            parse_ok(&src);
        }
    }
}
