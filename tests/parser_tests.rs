#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod parser_tests {
    // Once the parser is implemented, these tests should verify that the
    // example programs parse into the expected AST structures.

    #[test]
    fn parses_hello_program() {
        // TODO: parse examples/hello.dev and assert a function named "main" exists.
        assert!(true, "placeholder until parser is available")
    }

    #[test]
    fn parses_bump_struct() {
        // TODO: parse examples/bump.dev and assert a "BumpArena" struct with methods.
        assert!(true, "placeholder until parser is available")
    }
}
