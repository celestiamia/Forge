#[cfg(test)]
mod lexer_tests {
    // Once the lexer is implemented, these tests should exercise tokenizing
    // keywords, identifiers, literals, and punctuation used by the examples.

    #[test]
    fn recognizes_package_keyword() {
        // TODO: feed "package hello" into lexer and assert Package + Ident tokens.
        assert!(true, "placeholder until lexer is available")
    }

    #[test]
    fn recognizes_string_literal() {
        // TODO: assert that "Hello, Forge!" yields a single String token.
        assert!(true, "placeholder until lexer is available")
    }
}
