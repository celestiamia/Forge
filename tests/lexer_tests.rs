#[cfg(test)]
mod lexer_tests {
    use forgec::lexer::{Token, tokenize};

    #[test]
    fn recognizes_package_keyword() {
        let tokens = tokenize("package hello").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Package,
                Token::Ident("hello".to_string()),
                Token::Newline,
                Token::Eof
            ]
        );
    }

    #[test]
    fn recognizes_string_literal() {
        let tokens = tokenize("\"Hello, Forge!\"").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::StringLit("Hello, Forge!".to_string()),
                Token::Newline,
                Token::Eof
            ]
        );
    }

    #[test]
    fn recognizes_number_literals() {
        let tokens = tokenize("42 3.14").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::IntLit("42".to_string()),
                Token::FloatLit("3.14".to_string()),
                Token::Newline,
                Token::Eof
            ]
        );
    }

    #[test]
    fn recognizes_indentation_blocks() {
        let tokens = tokenize("def f():\n    return 1\n").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Def,
                Token::Ident("f".to_string()),
                Token::LParen,
                Token::RParen,
                Token::Colon,
                Token::Newline,
                Token::Indent,
                Token::Return,
                Token::IntLit("1".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Eof
            ]
        );
    }

    #[test]
    fn operators_and_punctuation() {
        let tokens = tokenize("a + b * (c - d)").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Ident("a".to_string()),
                Token::Plus,
                Token::Ident("b".to_string()),
                Token::Star,
                Token::LParen,
                Token::Ident("c".to_string()),
                Token::Minus,
                Token::Ident("d".to_string()),
                Token::RParen,
                Token::Newline,
                Token::Eof
            ]
        );
    }

    #[test]
    fn rejects_tabs_in_indentation() {
        let err = tokenize("def f():\n\treturn 1\n").unwrap_err();
        assert!(err.to_string().contains("tabs"));
    }
}
