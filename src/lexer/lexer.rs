use super::{LexError, Token};

/// Convenience function: tokenize an entire source string.
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let tok = lexer.next_token()?;
        let is_eof = tok == Token::Eof;
        tokens.push(tok);
        if is_eof {
            break;
        }
    }
    Ok(tokens)
}

/// A token paired with its 1-based source location.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenPos {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

/// Tokenize a source string and record each token's source position.
pub fn tokenize_with_pos(src: &str) -> Result<Vec<TokenPos>, LexError> {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let (line, col) = lexer.position();
        let tok = lexer.next_token()?;
        let is_eof = tok == Token::Eof;
        tokens.push(TokenPos { token: tok, line, col });
        if is_eof {
            break;
        }
    }
    Ok(tokens)
}

/// A lexer for Forge `.dev` source files.
///
/// Produces a stream of `Token`s, including `Newline`, `Indent`, and `Dedent`
/// tokens that reflect the indentation-based block structure.
pub struct Lexer<'src> {
    src: &'src str,
    /// Byte offset into `src`.
    pos: usize,
    /// 1-based line number of the current character.
    line: usize,
    /// 1-based column number of the current character.
    col: usize,
    /// Stack of indentation widths (in spaces). The bottom element is always 0.
    indent_stack: Vec<usize>,
    /// Number of Dedent tokens queued from a single dedent action.
    pending_dedents: usize,
    /// True when the next token begins a logical line and indentation may need processing.
    at_line_start: bool,
    /// Becomes true once EOF has been emitted.
    eof_emitted: bool,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source string.
    pub fn new(src: &'src str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            col: 1,
            indent_stack: vec![0],
            pending_dedents: 0,
            at_line_start: true,
            eof_emitted: false,
        }
    }

    /// Return the current 1-based line and column.
    pub fn position(&self) -> (usize, usize) {
        (self.line, self.col)
    }

    /// Return the next token, or an error.
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        if self.eof_emitted {
            return Ok(Token::Eof);
        }

        // Emit any dedents queued by a previous indentation change.
        if self.pending_dedents > 0 {
            self.pending_dedents -= 1;
            return Ok(Token::Dedent);
        }

        loop {
            // EOF handling: if the final logical line was not already terminated,
            // emit a final Newline, then drain any remaining indentation levels.
            if self.is_at_end() {
                if !self.at_line_start {
                    self.at_line_start = true;
                    return Ok(Token::Newline);
                }
                if self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    return Ok(Token::Dedent);
                }
                self.eof_emitted = true;
                return Ok(Token::Eof);
            }

            if self.at_line_start {
                match self.process_line_start()? {
                    LineStartAction::Token(tok) => return Ok(tok),
                    LineStartAction::Continue => continue,
                }
            }

            self.skip_inline_whitespace();

            // Re-check EOF after skipping whitespace.
            if self.is_at_end() {
                continue;
            }

            let c = self.peek();
            match c {
                '\n' => {
                    self.advance();
                    self.at_line_start = true;
                    return Ok(Token::Newline);
                }
                '#' => {
                    self.skip_comment();
                    if !self.is_at_end() && self.peek() == '\n' {
                        self.advance();
                        self.at_line_start = true;
                        return Ok(Token::Newline);
                    }
                    self.at_line_start = true;
                    continue;
                }
                '"' => return self.lex_string(),
                '\'' => return self.lex_char(),
                '0'..='9' => return self.lex_number(),
                'a'..='z' | 'A'..='Z' | '_' => return self.lex_ident_or_keyword(),
                '.' => {
                    let c2 = self.peek2();
                    if c2.is_ascii_digit() {
                        return self.lex_float_starting_dot();
                    }
                    return self.lex_operator_or_delim();
                }
                '+' | '-' | '*' | '/' | '%' | '<' | '>' | '&' | '|' | '^' | '~' | '!'
                | '=' | '@' | ',' | ':' | ';' | '(' | ')' | '[' | ']' | '{' | '}' => {
                    return self.lex_operator_or_delim()
                }
                _ => return Err(self.error(format!("unexpected character: {}", c))),
            }
        }
    }

    // ------------------------------------------------------------------
    // Indentation handling
    // ------------------------------------------------------------------

    fn process_line_start(&mut self) -> Result<LineStartAction, LexError> {
        let mut spaces = 0usize;

        // Consume leading spaces; tabs are forbidden in indentation.
        while !self.is_at_end() {
            match self.peek() {
                ' ' => {
                    spaces += 1;
                    self.advance();
                }
                '\t' => {
                    return Err(self.error("tabs are not allowed for indentation".to_string()));
                }
                _ => break,
            }
        }

        // Blank or comment-only lines do not participate in block structure.
        if self.is_at_end() {
            self.at_line_start = false;
            return Ok(LineStartAction::Continue);
        }
        if self.peek() == '\n' {
            self.advance();
            return Ok(LineStartAction::Continue);
        }
        if self.peek() == '#' {
            self.skip_comment();
            if !self.is_at_end() && self.peek() == '\n' {
                self.advance();
            }
            return Ok(LineStartAction::Continue);
        }

        // Validate that indentation is a multiple of 4 spaces.
        if spaces % 4 != 0 {
            return Err(self.error(format!(
                "indentation must be a multiple of 4 spaces, got {} spaces",
                spaces
            )));
        }
        let indent = spaces / 4;
        let current = *self.indent_stack.last().unwrap_or(&0);

        if indent > current {
            self.indent_stack.push(indent);
            self.at_line_start = false;
            return Ok(LineStartAction::Token(Token::Indent));
        }

        if indent < current {
            while *self.indent_stack.last().unwrap_or(&0) > indent {
                self.indent_stack.pop();
                self.pending_dedents += 1;
            }
            if *self.indent_stack.last().unwrap_or(&0) != indent {
                return Err(self.error(format!(
                    "inconsistent dedent to indentation level {}",
                    indent
                )));
            }
            self.at_line_start = false;
            if self.pending_dedents > 0 {
                self.pending_dedents -= 1;
                return Ok(LineStartAction::Token(Token::Dedent));
            }
        }

        // Same indentation: no token produced, just continue scanning the line.
        self.at_line_start = false;
        Ok(LineStartAction::Continue)
    }

    // ------------------------------------------------------------------
    // Whitespace / comments
    // ------------------------------------------------------------------

    fn skip_inline_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self) {
        // Both `#` line comments and `##` doc comments are skipped.
        debug_assert_eq!(self.peek(), '#');
        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }
    }

    // ------------------------------------------------------------------
    // Identifiers and keywords
    // ------------------------------------------------------------------

    fn lex_ident_or_keyword(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }
        let text = &self.src[start..self.pos];
        let tok = match text {
            "package" => Token::Package,
            "import" => Token::Import,
            "from" => Token::From,
            "def" => Token::Def,
            "let" => Token::Let,
            "var" => Token::Var,
            "return" => Token::Return,
            "if" => Token::If,
            "elif" => Token::Elif,
            "else" => Token::Else,
            "for" => Token::For,
            "in" => Token::In,
            "while" => Token::While,
            "match" => Token::Match,
            "case" => Token::Case,
            "struct" => Token::Struct,
            "union" => Token::Union,
            "enum" => Token::Enum,
            "interface" => Token::Interface,
            "impl" => Token::Impl,
            "unsafe" => Token::Unsafe,
            "extern" => Token::Extern,
            "pub" => Token::Pub,
            "const" => Token::Const,
            "static" => Token::Static,
            "embed" => Token::Embed,
            "as" => Token::As,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            "never" => Token::Never,
            "sizeof" => Token::Sizeof,
            "offsetof" => Token::Offsetof,
            "asm" => Token::Asm,
            "loop" => Token::Loop,
            "break" => Token::Break,
            "continue" => Token::Continue,
            _ => Token::Ident(text.to_string()),
        };
        Ok(tok)
    }

    // ------------------------------------------------------------------
    // Numeric literals
    // ------------------------------------------------------------------

    fn lex_number(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        let mut is_float = false;

        while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '_') {
            self.advance();
        }

        if !self.is_at_end() && self.peek() == '.' {
            // Lookahead: `3..5` must stay as int 3 followed by range operator `..`,
            // while `3.14` is a float.
            let c2 = self.peek2();
            if c2.is_ascii_digit() {
                self.advance();
                is_float = true;
                while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '_') {
                    self.advance();
                }
            }
        }

        // Optional type suffix (e.g. `u8`, `i32`, `f64`).
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }

        let text = self.src[start..self.pos].to_string();
        if is_float {
            Ok(Token::FloatLit(text))
        } else {
            Ok(Token::IntLit(text))
        }
    }

    fn lex_float_starting_dot(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        debug_assert_eq!(self.peek(), '.');
        debug_assert!(self.peek2().is_ascii_digit());
        self.advance(); // '.'
        while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '_') {
            self.advance();
        }
        // Optional type suffix.
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }
        Ok(Token::FloatLit(self.src[start..self.pos].to_string()))
    }

    // ------------------------------------------------------------------
    // String and char literals
    // ------------------------------------------------------------------

    fn lex_string(&mut self) -> Result<Token, LexError> {
        self.lex_quoted('"', Token::StringLit)
    }

    fn lex_char(&mut self) -> Result<Token, LexError> {
        self.lex_quoted('\'', Token::CharLit)
    }

    fn lex_quoted<F>(&mut self, quote: char, make: F) -> Result<Token, LexError>
    where
        F: FnOnce(String) -> Token,
    {
        debug_assert_eq!(self.peek(), quote);
        self.advance(); // opening quote

        let mut value = String::new();
        while !self.is_at_end() && self.peek() != quote {
            let c = self.peek();
            if c == '\\' {
                self.advance();
                if self.is_at_end() {
                    return Err(self.error("unterminated escape sequence".to_string()));
                }
                match self.peek() {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    '0' => value.push('\0'),
                    other => value.push(other),
                }
                self.advance();
            } else if c == '\n' {
                return Err(self.error("newline in string literal".to_string()));
            } else {
                value.push(c);
                self.advance();
            }
        }

        if self.is_at_end() {
            return Err(self.error("unterminated string literal".to_string()));
        }
        self.advance(); // closing quote
        Ok(make(value))
    }

    // ------------------------------------------------------------------
    // Operators and delimiters
    // ------------------------------------------------------------------

    fn lex_operator_or_delim(&mut self) -> Result<Token, LexError> {
        let c = self.peek();
        self.advance();
        let c2 = if !self.is_at_end() { self.peek() } else { '\0' };

        match (c, c2) {
            ('+', '=') => {
                self.advance();
                Ok(Token::PlusEq)
            }
            ('-', '=') => {
                self.advance();
                Ok(Token::MinusEq)
            }
            ('-', '>') => {
                self.advance();
                Ok(Token::Arrow)
            }
            ('*', '*') => {
                self.advance();
                Ok(Token::Power)
            }
            ('*', '=') => {
                self.advance();
                Ok(Token::StarEq)
            }
            ('/', '/') => {
                self.advance();
                Ok(Token::FloorDiv)
            }
            ('/', '=') => {
                self.advance();
                Ok(Token::SlashEq)
            }
            ('%', '=') => {
                self.advance();
                Ok(Token::PercentEq)
            }
            ('<', '<') => {
                self.advance();
                if !self.is_at_end() && self.peek() == '=' {
                    self.advance();
                    Ok(Token::ShlEq)
                } else {
                    Ok(Token::Shl)
                }
            }
            ('>', '>') => {
                self.advance();
                if !self.is_at_end() && self.peek() == '=' {
                    self.advance();
                    Ok(Token::ShrEq)
                } else {
                    Ok(Token::Shr)
                }
            }
            ('&', '&') => {
                self.advance();
                Ok(Token::AndAnd)
            }
            ('&', '=') => {
                self.advance();
                Ok(Token::AndEq)
            }
            ('|', '|') => {
                self.advance();
                Ok(Token::OrOr)
            }
            ('|', '=') => {
                self.advance();
                Ok(Token::OrEq)
            }
            ('^', '=') => {
                self.advance();
                Ok(Token::XorEq)
            }
            ('=', '=') => {
                self.advance();
                Ok(Token::Eq)
            }
            ('!', '=') => {
                self.advance();
                Ok(Token::Ne)
            }
            ('<', '=') => {
                self.advance();
                Ok(Token::Le)
            }
            ('>', '=') => {
                self.advance();
                Ok(Token::Ge)
            }
            ('@', _) => Ok(Token::At),
            ('.', '.') => {
                self.advance();
                if !self.is_at_end() && self.peek() == '=' {
                    self.advance();
                    Ok(Token::DotDotEq)
                } else {
                    Ok(Token::DotDot)
                }
            }
            ('.', _) => Ok(Token::Dot),
            (',', _) => Ok(Token::Comma),
            (':', _) => Ok(Token::Colon),
            (';', _) => Ok(Token::Semicolon),
            ('(', _) => Ok(Token::LParen),
            (')', _) => Ok(Token::RParen),
            ('[', _) => Ok(Token::LBracket),
            (']', _) => Ok(Token::RBracket),
            ('{', _) => Ok(Token::LBrace),
            ('}', _) => Ok(Token::RBrace),
            ('+', _) => Ok(Token::Plus),
            ('-', _) => Ok(Token::Minus),
            ('*', _) => Ok(Token::Star),
            ('/', _) => Ok(Token::Slash),
            ('%', _) => Ok(Token::Percent),
            ('<', _) => Ok(Token::Lt),
            ('>', _) => Ok(Token::Gt),
            ('&', _) => Ok(Token::And),
            ('|', _) => Ok(Token::Or),
            ('^', _) => Ok(Token::Xor),
            ('~', _) => Ok(Token::Tilde),
            ('!', _) => Ok(Token::Bang),
            ('=', _) => Ok(Token::Assign),
            _ => Err(self.error(format!("unexpected character: {}", c))),
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn is_at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> char {
        self.src[self.pos..].chars().next().unwrap_or('\0')
    }

    fn peek2(&self) -> char {
        self.src[self.pos..].chars().nth(1).unwrap_or('\0')
    }

    fn advance(&mut self) -> char {
        let c = self.peek();
        if c == '\0' {
            return '\0';
        }
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn error(&self, message: String) -> LexError {
        LexError {
            line: self.line,
            col: self.col,
            message,
        }
    }
}

enum LineStartAction {
    Token(Token),
    Continue,
}
