#[cfg(test)]
mod parser_tests {
    use forgec::ast::{Item, Stmt, TypeExpr};
    use forgec::parser::parse_module;

    #[test]
    fn parses_hello_program() {
        let src = std::fs::read_to_string("examples/hello.dev").unwrap();
        let m = parse_module(&src).unwrap();
        assert_eq!(m.package, "hello");
        let main = m.items.iter().find_map(|i| match i {
            Item::Function(f) if f.name == "main" => Some(f),
            _ => None,
        });
        assert!(main.is_some(), "expected a function named `main`");
    }

    #[test]
    fn parses_bump_program() {
        let src = std::fs::read_to_string("examples/bump.dev").unwrap();
        let m = parse_module(&src).unwrap();
        assert_eq!(m.package, "bump");
        let value_at = m.items.iter().find_map(|i| match i {
            Item::Function(f) if f.name == "value_at" => Some(f),
            _ => None,
        });
        let f = value_at.expect("expected a function named `value_at`");
        assert_eq!(f.params.len(), 1);
        assert!(matches!(&f.params[0].ty, TypeExpr::Pointer(_)));
        assert!(
            m.items
                .iter()
                .any(|i| matches!(i, Item::Function(f) if f.name == "main"))
        );
    }

    #[test]
    fn casts_do_not_absorb_next_statement() {
        // Regression: `var x: T = e as T` followed by a statement starting
        // with `(` used to merge the two lines into one bogus postfix chain.
        let src = r#"
package test

pub def main() -> int32:
    var b: Buf
    b.slot = 0
    var i: int32 = 0
    while i < 3:
        var x: int64 = i as int64
        (b.slot) = i
        i = i + 1
    return b.slot
"#;
        let m = parse_module(src).unwrap();
        let Item::Function(f) = m
            .items
            .iter()
            .find(|i| matches!(i, Item::Function(f) if f.name == "main"))
            .expect("expected function `main`")
        else {
            unreachable!()
        };
        let body = f.body.as_ref().expect("function body");
        assert!(
            body.stmts.iter().any(|s| matches!(s, Stmt::Assign(_))),
            "parenthesized assignment must be its own statement"
        );
    }
}
