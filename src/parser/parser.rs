use crate::ast::*;
use crate::lexer::{tokenize_with_pos, LexError, Token, TokenPos};

/// A parse error reported by the Forge parser, including 1-based line/column
/// information from the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(err: LexError) -> Self {
        Self {
            line: err.line,
            col: err.col,
            message: err.message,
        }
    }
}

/// Tokenize `src` and parse it into a [`Module`].
///
/// Embedded-file paths (`embed NAME = "path"`) resolve relative to the current
/// working directory.
pub fn parse_module(src: &str) -> Result<Module, ParseError> {
    parse_module_in_dir(src, std::path::Path::new("."))
}

/// Tokenize `src` and parse it into a [`Module]`, resolving `embed` file paths
/// relative to `base_dir` (the directory containing the source file).
pub fn parse_module_in_dir(src: &str, base_dir: &std::path::Path) -> Result<Module, ParseError> {
    let tokens = tokenize_with_pos(src)?;
    let mut parser = Parser::new(tokens);
    parser.base_dir = base_dir.to_path_buf();
    parser.parse_module()
}

/// A recursive-descent parser for Forge `.dev` source.
pub struct Parser {
    tokens: Vec<TokenPos>,
    pos: usize,
    pub(super) base_dir: std::path::PathBuf,
}

impl Parser {
    pub fn new(tokens: Vec<TokenPos>) -> Self {
        Self {
            tokens,
            pos: 0,
            base_dir: std::path::PathBuf::from("."),
        }
    }

    pub(super) fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|tp| &tp.token)
            .unwrap_or(&Token::Eof)
    }

    pub(super) fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if !self.at_end() {
            self.pos += 1;
        }
        tok
    }

    pub(super) fn at_end(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    pub(super) fn line_col(&self) -> (usize, usize) {
        self.tokens
            .get(self.pos)
            .map(|tp| (tp.line, tp.col))
            .unwrap_or((0, 0))
    }

    pub(super) fn current_span(&self) -> Span {
        let (line, col) = self.line_col();
        Span::new(line, col)
    }

    pub(super) fn error(&self, message: impl Into<String>) -> ParseError {
        let (line, col) = self.line_col();
        ParseError {
            line,
            col,
            message: message.into(),
        }
    }

    pub(super) fn expect(&mut self, kind: Token) -> Result<Token, ParseError> {
        if self.peek() == &kind {
            Ok(self.advance())
        } else {
            Err(self.error(format!(
                "expected {:?}, found {:?}",
                kind,
                self.peek()
            )))
        }
    }

    pub(super) fn eat(&mut self, kind: &Token) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(super) fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    /// Peek at the first token ahead of the cursor, skipping any `Newline`
    /// tokens but without consuming them. Used to decide whether a postfix
    /// chain legitimately continues onto the next non-blank line.

    pub(super) fn peek_past_newlines(&self) -> Token {
        let mut i = self.pos;
        while i < self.tokens.len() && matches!(self.tokens[i].token, Token::Newline) {
            i += 1;
        }
        self.tokens
            .get(i)
            .map(|tp| tp.token.clone())
            .unwrap_or(Token::Eof)
    }

    pub(super) fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            other => Err(self.error(format!("expected identifier, found {:?}", other))),
        }
    }

    /// Consume the next token if it is an identifier or a keyword that can be
    /// used as an attribute name (e.g. `@extern("c")`).

    pub(super) fn expect_ident_or_keyword(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            Token::Extern => Ok("extern".to_string()),
            other => Err(self.error(format!("expected identifier, found {:?}", other))),
        }
    }

    pub(super) fn parse_module(&mut self) -> Result<Module, ParseError> {
        self.skip_newlines();
        let package = if self.eat(&Token::Package) {
            let path = self.parse_path()?;
            self.skip_newlines();
            path.join(".")
        } else {
            String::new()
        };

        let mut imports = Vec::new();
        let mut items = Vec::new();

        while !self.at_end() {
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            match self.peek() {
                Token::Import | Token::From => imports.push(self.parse_import()?),
                _ => items.push(self.parse_item()?),
            }
        }

        Ok(Module {
            package,
            imports,
            items,
        })
    }

}

pub(super) fn parse_int(s: &str) -> Literal {
    let s_clean = s.replace('_', "");
    if s_clean.starts_with("0x") || s_clean.starts_with("0X") {
        Literal::Int(i64::from_str_radix(&s_clean[2..], 16).unwrap_or(0))
    } else if s_clean.starts_with("0b") || s_clean.starts_with("0B") {
        Literal::Int(i64::from_str_radix(&s_clean[2..], 2).unwrap_or(0))
    } else if s_clean.starts_with("0o") || s_clean.starts_with("0O") {
        Literal::Int(i64::from_str_radix(&s_clean[2..], 8).unwrap_or(0))
    } else {
        let s = s_clean.trim_end_matches(|c: char| c.is_alphabetic());
        Literal::Int(s.parse::<i64>().unwrap_or(0))
    }
}
