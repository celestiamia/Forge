//! Lexer for the Forge language (.dev files).
//!
//! The public API is [`Token`], [`Lexer`], [`LexError`], and [`tokenize`].

#[allow(clippy::module_inception)]
pub mod lexer;

#[allow(unused_imports)]
pub use lexer::{Lexer, TokenPos, tokenize, tokenize_with_pos};

/// A lexical token produced by the Forge lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// End of input.
    Eof,
    /// Logical line break.
    Newline,
    /// Increase in indentation level (Python-style block start).
    Indent,
    /// Decrease in indentation level (Python-style block end).
    Dedent,

    /// Identifier, e.g. `foo` or `bar_baz`.
    Ident(String),

    // Keywords
    /// `package`
    Package,
    /// `import`
    Import,
    /// `from`
    From,
    /// `def`
    Def,
    /// `let`
    Let,
    /// `var`
    Var,
    /// `return`
    Return,
    /// `if`
    If,
    /// `elif`
    Elif,
    /// `else`
    Else,
    /// `for`
    For,
    /// `in`
    In,
    /// `while`
    While,
    /// `match`
    Match,
    /// `case`
    Case,
    /// `struct`
    Struct,
    /// `union`
    Union,
    /// `enum`
    Enum,
    /// `interface`
    Interface,
    /// `impl`
    Impl,
    /// `unsafe`
    Unsafe,
    /// `extern`
    Extern,
    /// `pub`
    Pub,
    /// `const`
    Const,
    /// `static`
    Static,
    /// `embed`
    Embed,
    /// `as`
    As,
    /// `true`
    True,
    /// `false`
    False,
    /// `null`
    Null,
    /// `never`
    Never,
    /// `sizeof`
    Sizeof,
    /// `offsetof`
    Offsetof,
    /// `loop`
    Loop,
    /// `break`
    Break,
    /// `continue`
    Continue,

    // Literals
    /// Integer literal, including optional suffix (e.g. `42`, `0xff_u8`).
    IntLit(String),
    /// Floating-point literal, including optional suffix (e.g. `3.14`, `10.5f64`).
    FloatLit(String),
    /// Double-quoted string literal, with escape sequences interpreted.
    StringLit(String),
    /// Single-quoted character literal, with escape sequences interpreted.
    CharLit(String),

    // Arithmetic operators
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `**`
    Power,
    /// `//`
    FloorDiv,

    // Bitwise / logical operators
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `&`
    And,
    /// `|`
    Or,
    /// `^`
    Xor,
    /// `~`
    Tilde,
    /// `!`
    Bang,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,

    // Comparison operators
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,

    // Assignment operators
    /// `=`
    Assign,
    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,
    /// `%=`
    PercentEq,
    /// `&=`
    AndEq,
    /// `|=`
    OrEq,
    /// `^=`
    XorEq,
    /// `<<=`
    ShlEq,
    /// `>>=`
    ShrEq,

    // Other operators
    /// `->`
    Arrow,
    /// `@`
    At,

    // Delimiters
    /// `.`
    Dot,
    /// `..`
    DotDot,
    /// `..=`
    DotDotEq,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `;`
    Semicolon,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
}

/// An error reported by the lexer, with 1-based line/column information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for LexError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(src: &str) -> Vec<Token> {
        tokenize(src).unwrap()
    }

    #[test]
    fn empty_source() {
        assert_eq!(tok(""), vec![Token::Eof]);
    }

    #[test]
    fn simple_identifier() {
        assert_eq!(
            tok("foo\n"),
            vec![Token::Ident("foo".to_string()), Token::Newline, Token::Eof]
        );
    }

    #[test]
    fn all_keywords() {
        let src = "package import from def let var return if elif else for in while match case struct union enum interface impl unsafe extern pub const static embed as true false null\n";
        let expected = vec![
            Token::Package,
            Token::Import,
            Token::From,
            Token::Def,
            Token::Let,
            Token::Var,
            Token::Return,
            Token::If,
            Token::Elif,
            Token::Else,
            Token::For,
            Token::In,
            Token::While,
            Token::Match,
            Token::Case,
            Token::Struct,
            Token::Union,
            Token::Enum,
            Token::Interface,
            Token::Impl,
            Token::Unsafe,
            Token::Extern,
            Token::Pub,
            Token::Const,
            Token::Static,
            Token::Embed,
            Token::As,
            Token::True,
            Token::False,
            Token::Null,
            Token::Newline,
            Token::Eof,
        ];
        assert_eq!(tok(src), expected);
    }

    #[test]
    fn numeric_literals() {
        assert_eq!(
            tok("42 0 1_000_000 3.14 0.5 2.0f64 255u8 -7i32\n"),
            vec![
                Token::IntLit("42".to_string()),
                Token::IntLit("0".to_string()),
                Token::IntLit("1_000_000".to_string()),
                Token::FloatLit("3.14".to_string()),
                Token::FloatLit("0.5".to_string()),
                Token::FloatLit("2.0f64".to_string()),
                Token::IntLit("255u8".to_string()),
                Token::Minus,
                Token::IntLit("7i32".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn string_and_char_literals() {
        assert_eq!(
            tok(r#""hello" 'a' "with\nescape" '"'"#),
            vec![
                Token::StringLit("hello".to_string()),
                Token::CharLit("a".to_string()),
                Token::StringLit("with\nescape".to_string()),
                Token::CharLit("\"".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn operators_and_delimiters() {
        let src = "+ - * / % ** // << >> & | ^ ~ ! < > <= >= == != = += -= *= /= &= |= ^= <<= >>= -> . , : ; ( ) [ ] { }\n";
        let expected = vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Percent,
            Token::Power,
            Token::FloorDiv,
            Token::Shl,
            Token::Shr,
            Token::And,
            Token::Or,
            Token::Xor,
            Token::Tilde,
            Token::Bang,
            Token::Lt,
            Token::Gt,
            Token::Le,
            Token::Ge,
            Token::Eq,
            Token::Ne,
            Token::Assign,
            Token::PlusEq,
            Token::MinusEq,
            Token::StarEq,
            Token::SlashEq,
            Token::AndEq,
            Token::OrEq,
            Token::XorEq,
            Token::ShlEq,
            Token::ShrEq,
            Token::Arrow,
            Token::Dot,
            Token::Comma,
            Token::Colon,
            Token::Semicolon,
            Token::LParen,
            Token::RParen,
            Token::LBracket,
            Token::RBracket,
            Token::LBrace,
            Token::RBrace,
            Token::Newline,
            Token::Eof,
        ];
        assert_eq!(tok(src), expected);
    }

    #[test]
    fn line_comments_skipped() {
        assert_eq!(
            tok("foo # a comment\nbar\n"),
            vec![
                Token::Ident("foo".to_string()),
                Token::Newline,
                Token::Ident("bar".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn doc_comments_skipped() {
        assert_eq!(
            tok("## doc comment\n42\n"),
            vec![Token::IntLit("42".to_string()), Token::Newline, Token::Eof]
        );
    }

    #[test]
    fn indentation_basic() {
        let src = "if x:\n    y\n    z\nw\n";
        assert_eq!(
            tok(src),
            vec![
                Token::If,
                Token::Ident("x".to_string()),
                Token::Colon,
                Token::Newline,
                Token::Indent,
                Token::Ident("y".to_string()),
                Token::Newline,
                Token::Ident("z".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Ident("w".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn indentation_nested() {
        let src = "a\n    b\n        c\n    d\ne\n";
        assert_eq!(
            tok(src),
            vec![
                Token::Ident("a".to_string()),
                Token::Newline,
                Token::Indent,
                Token::Ident("b".to_string()),
                Token::Newline,
                Token::Indent,
                Token::Ident("c".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Ident("d".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Ident("e".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn indentation_blank_lines_ignored() {
        let src = "a\n\n    b\n\nc\n";
        assert_eq!(
            tok(src),
            vec![
                Token::Ident("a".to_string()),
                Token::Newline,
                Token::Indent,
                Token::Ident("b".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Ident("c".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn indentation_eof_dedents() {
        let src = "a\n    b";
        assert_eq!(
            tok(src),
            vec![
                Token::Ident("a".to_string()),
                Token::Newline,
                Token::Indent,
                Token::Ident("b".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn tab_in_indentation_is_error() {
        let err = tokenize("a\n\tb").unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(err.col, 1);
        assert!(err.message.contains("tabs"));
    }

    #[test]
    fn bad_dedent_is_error() {
        let err = tokenize("a\n    b\n            c\n        d").unwrap_err();
        assert!(err.message.contains("inconsistent dedent"));
    }

    #[test]
    fn non_multiple_indent_is_error() {
        let err = tokenize("a\n   b").unwrap_err();
        assert!(err.message.contains("multiple of 4"));
    }

    #[test]
    fn unterminated_string_is_error() {
        let err = tokenize(r#""hello"#).unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn newline_in_string_is_error() {
        let err = tokenize("\"he\nllo\"").unwrap_err();
        assert!(err.message.contains("newline in string literal"));
    }

    #[test]
    fn lexer_next_token_incrementally() {
        let mut lexer = Lexer::new("let x = 1\n");
        assert_eq!(lexer.next_token().unwrap(), Token::Let);
        assert_eq!(lexer.next_token().unwrap(), Token::Ident("x".to_string()));
        assert_eq!(lexer.next_token().unwrap(), Token::Assign);
        assert_eq!(lexer.next_token().unwrap(), Token::IntLit("1".to_string()));
        assert_eq!(lexer.next_token().unwrap(), Token::Newline);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn full_program_like() {
        let src = r#"package hello

def main() -> int32:
    return 0
"#;
        assert_eq!(
            tokenize(src).unwrap(),
            vec![
                Token::Package,
                Token::Ident("hello".to_string()),
                Token::Newline,
                Token::Def,
                Token::Ident("main".to_string()),
                Token::LParen,
                Token::RParen,
                Token::Arrow,
                Token::Ident("int32".to_string()),
                Token::Colon,
                Token::Newline,
                Token::Indent,
                Token::Return,
                Token::IntLit("0".to_string()),
                Token::Newline,
                Token::Dedent,
                Token::Eof,
            ]
        );
    }
}
