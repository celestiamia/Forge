//! Lexer for the Forge Linker Descriptor (`.fld`) format.

#[derive(Debug, Clone, PartialEq)]
pub enum Tk {
    Ident(String),
    Number(u64),
    Str(String),
    Colon,
    Comma,
    Equals,
    Gt,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Tk,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'s> {
    src: &'s str,
    bytes: &'s [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'s> Lexer<'s> {
    pub fn new(src: &'s str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut toks = Vec::new();
        loop {
            let t = self.next_token()?;
            let done = t.kind == Tk::Eof;
            toks.push(t);
            if done {
                break;
            }
        }
        Ok(toks)
    }

    fn next_token(&mut self) -> Result<Token, String> {
        self.skip_ws_and_comments();
        let line = self.line;
        let col = self.col;
        let c = if let Some(&b) = self.bytes.get(self.pos) {
            b
        } else {
            return Ok(Token { kind: Tk::Eof, line, col });
        };
        let kind = match c {
            b'{' => { self.advance(); Tk::LBrace }
            b'}' => { self.advance(); Tk::RBrace }
            b'(' => { self.advance(); Tk::LParen }
            b')' => { self.advance(); Tk::RParen }
            b':' => { self.advance(); Tk::Colon }
            b',' => { self.advance(); Tk::Comma }
            b'=' => { self.advance(); Tk::Equals }
            b'>' => { self.advance(); Tk::Gt }
            b'"' => self.read_string()?,
            b'0'..=b'9' => self.read_number()?,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'.' => self.read_ident()?,
            _ => Err(format!("unexpected byte {:?} at line {} col {}", c as char, line, col))?,
        };
        Ok(Token { kind, line, col })
    }

    fn advance(&mut self) {
        if self.bytes[self.pos] == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        self.pos += 1;
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            match self.bytes.get(self.pos) {
                Some(b) if b.is_ascii_whitespace() => self.advance(),
                Some(b'#') => {
                    while let Some(&b) = self.bytes.get(self.pos) {
                        if b == b'\n' { break; }
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn read_ident(&mut self) -> Result<Tk, String> {
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-' {
                self.advance();
            } else {
                break;
            }
        }
        let s = &self.src[start..self.pos];
        Ok(Tk::Ident(s.to_string()))
    }

    fn read_number(&mut self) -> Result<Tk, String> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'0')
            && self.bytes.get(self.pos + 1) == Some(&b'x')
        {
            self.advance();
            self.advance();
            let hex_start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b.is_ascii_hexdigit() || b == b'_' {
                    self.advance();
                } else {
                    break;
                }
            }
            let hex_str: String = self.src[hex_start..self.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect();
            let n = u64::from_str_radix(&hex_str, 16)
                .map_err(|e| format!("invalid hex literal: {}", e))?;
            return self.apply_size_suffix(n);
        }
        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_digit() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        let digits: String = self.src[start..self.pos]
            .chars()
            .filter(|&c| c != '_')
            .collect();
        let n: u64 = digits.parse().map_err(|e| format!("invalid integer literal: {}", e))?;
        self.apply_size_suffix(n)
    }

    fn apply_size_suffix(&mut self, n: u64) -> Result<Tk, String> {
        let save = self.pos;
        let save_line = self.line;
        let save_col = self.col;
        if let Some(&b) = self.bytes.get(self.pos) {
            let (mult, len) = match b {
                b'K' => (1024u64, 1),
                b'M' => (1024 * 1024, 1),
                b'G' => (1024 * 1024 * 1024, 1),
                b'k' => (1024u64, 1),
                b'm' => (1024 * 1024, 1),
                b'g' => (1024 * 1024 * 1024, 1),
                _ => return Ok(Tk::Number(n)),
            };
            let after = self.pos + len;
            if after <= self.bytes.len() && (after == self.bytes.len() || !self.bytes[after].is_ascii_alphabetic()) {
                self.pos = after;
                self.col += len;
                return Ok(Tk::Number(n * mult));
            }
        }
        self.pos = save;
        self.line = save_line;
        self.col = save_col;
        Ok(Tk::Number(n))
    }

    fn read_string(&mut self) -> Result<Tk, String> {
        self.advance();
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b'"' {
                let s = self.src[start..self.pos].to_string();
                self.advance();
                return Ok(Tk::Str(s));
            }
            if b == b'\\' {
                self.advance();
            }
            self.advance();
        }
        Err("unterminated string literal".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple() {
        let mut lx = Lexer::new("MEMORY { ram (rwx) : origin = 0x100, length = 64K }");
        let toks = lx.tokenize().unwrap();
        let kinds: Vec<&str> = toks.iter().map(|t| match &t.kind {
            Tk::Ident(s) => s.as_str(),
            Tk::Number(n) => match *n { 256 => "256", 65536 => "65536", _ => "num" },
            Tk::LBrace => "{",
            Tk::RBrace => "}",
            Tk::LParen => "(",
            Tk::RParen => ")",
            Tk::Colon => ":",
            Tk::Equals => "=",
            Tk::Comma => ",",
            Tk::Gt => ">",
            Tk::Str(_) => "str",
            Tk::Eof => "EOF",
        }).collect();
        assert_eq!(kinds, vec!["MEMORY", "{", "ram", "(", "rwx", ")", ":", "origin", "=", "256", ",", "length", "=", "65536", "}", "EOF"]);
    }

    #[test]
    fn hex_and_suffix() {
        let mut lx = Lexer::new("0x7C00 1M 512K");
        let toks = lx.tokenize().unwrap();
        match (&toks[0].kind, &toks[1].kind, &toks[2].kind) {
            (Tk::Number(a), Tk::Number(b), Tk::Number(c)) => {
                assert_eq!(*a, 0x7C00);
                assert_eq!(*b, 1048576);
                assert_eq!(*c, 524288);
            }
            _ => panic!("expected numbers"),
        }
    }

    #[test]
    fn comments_skipped() {
        let mut lx = Lexer::new("# a comment\nARCH x86_64");
        let toks = lx.tokenize().unwrap();
        assert_eq!(toks[0].kind, Tk::Ident("ARCH".to_string()));
        assert_eq!(toks[1].kind, Tk::Ident("x86_64".to_string()));
    }

    #[test]
    fn string_literal() {
        let mut lx = Lexer::new(r#""hello world""#);
        let toks = lx.tokenize().unwrap();
        assert_eq!(toks[0].kind, Tk::Str("hello world".to_string()));
    }
}
