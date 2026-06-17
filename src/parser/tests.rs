//! Unit tests for the Forge parser.

use super::*;

#[test]
fn empty_module() {
    let m = parse_module("").unwrap();
    assert_eq!(m.package, "");
    assert!(m.imports.is_empty());
    assert!(m.items.is_empty());
}

#[test]
fn package_and_imports() {
    let src = r#"package hello
import std.io
from std.math import sin, cos
from std.all import *
"#;
    let m = parse_module(src).unwrap();
    assert_eq!(m.package, "hello");
    assert_eq!(m.imports.len(), 3);
    assert!(matches!(
        m.imports[0],
        Import::Path {
            path: ref p,
            alias: None
        } if p == &["std".to_string(), "io".to_string()]
    ));
    assert!(matches!(
        m.imports[1],
        Import::From {
            path: ref p,
            items: Some(ref items)
        } if p == &["std".to_string(), "math".to_string()] && items == &["sin".to_string(), "cos".to_string()]
    ));
    assert!(matches!(
        m.imports[2],
        Import::From {
            path: ref p,
            items: None
        } if p == &["std".to_string(), "all".to_string()]
    ));
}

#[test]
fn simple_function() {
    let src = r#"def main() -> int32 {
    return 0
}"#;
    let m = parse_module(src).unwrap();
    assert_eq!(m.items.len(), 1);
    let f = match &m.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert_eq!(f.name, "main");
    assert!(f.params.is_empty());
    assert!(matches!(f.ret, Some(TypeExpr::Name(n)) if n == "int32"));
    assert!(f.body.is_some());
    assert!(matches!(
        f.body.as_ref().unwrap().stmts[0],
        Stmt::Return(Some(Expr::Literal(Literal::Int(0))))
    ));
}

#[test]
fn function_with_generics_and_params() {
    let src = r#"def identity[T](x: T) -> T {
    return x
}"#;
    let m = parse_module(src).unwrap();
    let f = match &m.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert_eq!(f.generics, vec!["T"]);
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].name, "x");
    assert!(matches!(f.params[0].ty, TypeExpr::Name(ref n) if n == "T"));
    assert!(matches!(f.ret, Some(TypeExpr::Name(ref n)) if n == "T"));
}

#[test]
fn public_unsafe_function() {
    let src = r#"pub unsafe def _start() -> never {
    loop {}
}"#;
    let m = parse_module(src).unwrap();
    let f = match &m.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert!(matches!(f.vis, Visibility::Public));
    assert!(f.unsafe_kw);
    assert_eq!(f.name, "_start");
    assert!(matches!(f.ret, Some(TypeExpr::Name(ref n) if n == "never")));
}

#[test]
fn attributes() {
    let src = r#"@packed
@align(8)
struct S {
    x: int32
}

@freestanding
pub def entry() {}

@extern("c")
def ffi()

@c_enum
enum Color {
    Red,
    Green(int32),
    Blue
}
"#;
    let m = parse_module(src).unwrap();
    let s = match &m.items[0] {
        Item::Struct(s) => s,
        _ => panic!("expected struct"),
    };
    assert!(s.attrs.contains(&Attribute::Packed));
    assert!(s.attrs.contains(&Attribute::Align(8)));

    let f = match &m.items[1] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert!(f.attrs.contains(&Attribute::Freestanding));

    let e = match &m.items[2] {
        Item::ExternFn(e) => e,
        _ => panic!("expected extern function"),
    };
    assert!(e.attrs.contains(&Attribute::Extern("c".to_string())));

    let en = match &m.items[3] {
        Item::Enum(en) => en,
        _ => panic!("expected enum"),
    };
    assert!(en.attrs.contains(&Attribute::CEnum));
    assert_eq!(en.variants.len(), 3);
    assert!(en.variants[1].payload.is_some());
}

#[test]
fn struct_union_enum_impl() {
    let src = r#"struct Point {
    x: int32,
    y: int32
}

union Value {
    i: int32,
    b: bool
}

enum Option[T] {
    None,
    Some(T)
}

impl Point {
    def length(self: Point) -> int32 {
        return 0
    }
}

use std.io as io
"#;
    let m = parse_module(src).unwrap();
    assert!(matches!(m.items[0], Item::Struct(_)));
    assert!(matches!(m.items[1], Item::Union(_)));
    assert!(matches!(m.items[2], Item::Enum(_)));
    assert!(matches!(m.items[3], Item::Impl(_)));
    assert!(matches!(m.items[4], Item::Use(_)));
}

#[test]
fn extern_fn_and_const() {
    let src = r#"extern def exit(code: int32)
const UART0_ADDR: usize = 0x10000000
"#;
    let m = parse_module(src).unwrap();
    assert!(matches!(m.items[0], Item::ExternFn(_)));
    let c = match &m.items[1] {
        Item::Const(c) => c,
        _ => panic!("expected const"),
    };
    assert_eq!(c.name, "UART0_ADDR");
    assert!(matches!(c.ty, Some(TypeExpr::Name(ref n) if n == "usize"));
}

#[test]
fn statements() {
    let src = r#"def test() {
    let x = 1
    var y: int32 = 2
    y = x + y
    if y > 0 {
        return y
    } elif y == 0 {
        return 0
    } else {
        return -1
    }
    for i in 0..10 {
        x = x + i
    }
    while x < 100 {
        x = x + 1
    }
    match x {
        case 0: return 0
        case 1: return 1
        case _: return -1
    }
    unsafe {
        asm("nop")
    }
    loop {
        break
        continue
    }
}"#;
    let m = parse_module(src).unwrap();
    let f = match &m.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    let stmts = &f.body.as_ref().unwrap().stmts;
    assert!(matches!(stmts[0], Stmt::Let(_)));
    assert!(matches!(stmts[1], Stmt::Var(_)));
    assert!(matches!(stmts[2], Stmt::Assign(_)));
    assert!(matches!(stmts[3], Stmt::If(_)));
    assert!(matches!(stmts[4], Stmt::For(_)));
    assert!(matches!(stmts[5], Stmt::While(_)));
    assert!(matches!(stmts[6], Stmt::Match(_)));
    assert!(matches!(stmts[7], Stmt::UnsafeBlock(_)));
    assert!(matches!(stmts[8], Stmt::Loop(_)));
}

#[test]
fn expressions() {
    let src = r#"def test() -> int32 {
    let a = 1 + 2 * 3
    let b = -a
    let c = a == b
    let d = a as int64
    let e = (1, 2, 3)
    let f = [1, 2, 3]
    let g = sizeof(int32)
    let h = offsetof(Point, x)
    let i = asm("nop", in("r") a, out("=r") b, clobber("rax"))
    let j = *a
    let k = &a
    let l = a.b
    let m = a[0]
    let n = if a > 0 { 1 } else { 0 }
    let o = match a {
        case 0: 1
        case _: 0
    }
    return 0
}"#;
    assert!(parse_module(src).is_ok());
}

#[test]
fn types() {
    let src = r#"def test(
    a: int32,
    b: ptr[byte],
    c: [byte; 1024],
    d: [int32],
    e: (int32, bool),
    f: (int32) -> bool
) {}
"#;
    let m = parse_module(src).unwrap();
    let f = match &m.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert!(matches!(f.params[0].ty, TypeExpr::Name(_)));
    assert!(matches!(f.params[1].ty, TypeExpr::Pointer(_)));
    assert!(matches!(f.params[2].ty, TypeExpr::Array(_, _)));
    assert!(matches!(f.params[3].ty, TypeExpr::Slice(_)));
    assert!(matches!(f.params[4].ty, TypeExpr::Tuple(_)));
    assert!(matches!(f.params[5].ty, TypeExpr::Function { .. }));
}

#[test]
fn indentation_block() {
    let src = "def main() -> int32:\n    return 0\n";
    let m = parse_module(src).unwrap();
    let f = match &m.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert!(matches!(
        f.body.as_ref().unwrap().stmts[0],
        Stmt::Return(Some(Expr::Literal(Literal::Int(0))))
    ));
}

#[test]
fn parse_error_reports_location() {
    let src = "def main() {\n    let x = \n}";
    let err = parse_module(src).unwrap_err();
    assert!(err.line > 0);
    assert!(err.col > 0);
}

#[test]
fn method_call_and_field_index() {
    let src = r#"def test() {
    let arena = BumpArena { base: 0, end: 0, cursor: 0 }
    let p = arena.alloc(16)
    if p == null {
        return 1
    }
    arena.reset()
    return 0
}"#;
    assert!(parse_module(src).is_ok());
}
