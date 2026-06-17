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
pub fn parse_module(src: &str) -> Result<Module, ParseError> {
    let tokens = tokenize_with_pos(src)?;
    let mut parser = Parser::new(tokens);
    parser.parse_module()
}

/// A recursive-descent parser for Forge `.dev` source.
pub struct Parser {
    tokens: Vec<TokenPos>,
    pos: usize,
}

impl Parser {
    /// Create a parser from a pre-lexed, position-aware token stream.
    pub fn new(tokens: Vec<TokenPos>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ------------------------------------------------------------------
    // Low-level helpers
    // ------------------------------------------------------------------

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|tp| &tp.token)
            .unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if !self.at_end() {
            self.pos += 1;
        }
        tok
    }

    fn at_end(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn line_col(&self) -> (usize, usize) {
        self.tokens
            .get(self.pos)
            .map(|tp| (tp.line, tp.col))
            .unwrap_or((0, 0))
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        let (line, col) = self.line_col();
        ParseError {
            line,
            col,
            message: message.into(),
        }
    }

    fn expect(&mut self, kind: Token) -> Result<Token, ParseError> {
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

    fn eat(&mut self, kind: &Token) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    /// Peek at the first token ahead of the cursor, skipping any `Newline`
    /// tokens but without consuming them. Used to decide whether a postfix
    /// chain legitimately continues onto the next non-blank line.
    fn peek_past_newlines(&self) -> Token {
        let mut i = self.pos;
        while i < self.tokens.len() && matches!(self.tokens[i].token, Token::Newline) {
            i += 1;
        }
        self.tokens
            .get(i)
            .map(|tp| tp.token.clone())
            .unwrap_or(Token::Eof)
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            other => Err(self.error(format!("expected identifier, found {:?}", other))),
        }
    }

    /// Consume the next token if it is an identifier or a keyword that can be
    /// used as an attribute name (e.g. `@extern("c")`).
    fn expect_ident_or_keyword(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            Token::Extern => Ok("extern".to_string()),
            other => Err(self.error(format!("expected identifier, found {:?}", other))),
        }
    }

    // ------------------------------------------------------------------
    // Module / imports / items
    // ------------------------------------------------------------------

    fn parse_module(&mut self) -> Result<Module, ParseError> {
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

    fn parse_import(&mut self) -> Result<Import, ParseError> {
        if self.eat(&Token::Import) {
            let path = self.parse_path()?;
            let alias = if self.eat(&Token::As) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            Ok(Import::Path { path, alias })
        } else if self.eat(&Token::From) {
            let path = self.parse_path()?;
            self.expect(Token::Import)?;
            let items = if self.eat(&Token::Star) {
                None
            } else {
                Some(self.parse_ident_list()?)
            };
            Ok(Import::From { path, items })
        } else {
            Err(self.error("expected import declaration"))
        }
    }

    fn parse_path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut segments = vec![self.expect_ident()?];
        while self.eat(&Token::Dot) {
            segments.push(self.expect_ident()?);
        }
        Ok(segments)
    }

    fn parse_ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut list = vec![self.expect_ident()?];
        while self.eat(&Token::Comma) {
            list.push(self.expect_ident()?);
        }
        Ok(list)
    }

    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let attrs = self.parse_attrs()?;
        let vis = self.parse_visibility();

        match self.peek() {
            Token::Def | Token::Unsafe => self.parse_function(attrs, vis),
            Token::Struct => self.parse_struct(attrs, vis),
            Token::Union => self.parse_union(attrs, vis),
            Token::Enum => self.parse_enum(attrs, vis),
            Token::Impl => self.parse_impl(),
            Token::Extern => self.parse_extern_fn(attrs),
            Token::Const | Token::Static => self.parse_const_item(vis),
            other => Err(self.error(format!("expected item, found {:?}", other))),
        }
    }

    fn parse_attrs(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attrs = Vec::new();
        loop {
            self.skip_newlines();
            if !self.eat(&Token::At) {
                break;
            }
            let name = self.expect_ident_or_keyword()?;
            let attr = match name.as_str() {
                "packed" => Attribute::Packed,
                "freestanding" => Attribute::Freestanding,
                "c_enum" => Attribute::CEnum,
                "align" => {
                    self.expect(Token::LParen)?;
                    let n = self.expect_int()? as u64;
                    self.expect(Token::RParen)?;
                    Attribute::Align(n)
                }
                "extern" => {
                    self.expect(Token::LParen)?;
                    let abi = self.expect_string()?;
                    self.expect(Token::RParen)?;
                    Attribute::Extern(abi)
                }
                _ => return Err(self.error(format!("unknown attribute @{}", name))),
            };
            attrs.push(attr);
        }
        Ok(attrs)
    }

    fn parse_visibility(&mut self) -> Visibility {
        if self.eat(&Token::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    fn parse_function(
        &mut self,
        attrs: Vec<Attribute>,
        vis: Visibility,
    ) -> Result<Item, ParseError> {
        let mut unsafe_kw = false;
        if self.eat(&Token::Unsafe) {
            unsafe_kw = true;
        }
        self.expect(Token::Def)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;
        let params = self.parse_params()?;
        let ret = if self.eat(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.skip_newlines();
        let body = if self.peek() == &Token::LBrace
            || self.peek() == &Token::Colon
            || self.peek() == &Token::Indent
        {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Item::Function(Function {
            attrs,
            vis,
            unsafe_kw,
            name,
            generics,
            params,
            ret,
            body,
        }))
    }

    fn parse_struct(&mut self, attrs: Vec<Attribute>, vis: Visibility) -> Result<Item, ParseError> {
        self.expect(Token::Struct)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;
        let fields = self.parse_field_block()?;
        Ok(Item::Struct(Struct {
            attrs,
            vis,
            name,
            generics,
            fields,
        }))
    }

    fn parse_union(&mut self, attrs: Vec<Attribute>, vis: Visibility) -> Result<Item, ParseError> {
        self.expect(Token::Union)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;
        let fields = self.parse_field_block()?;
        Ok(Item::Union(Union {
            attrs,
            vis,
            name,
            generics,
            fields,
        }))
    }

    fn parse_enum(&mut self, attrs: Vec<Attribute>, vis: Visibility) -> Result<Item, ParseError> {
        self.expect(Token::Enum)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;
        let variants = self.parse_variant_block()?;
        Ok(Item::Enum(Enum {
            attrs,
            vis,
            name,
            generics,
            variants,
        }))
    }

    fn parse_impl(&mut self) -> Result<Item, ParseError> {
        self.expect(Token::Impl)?;
        let target = self.parse_type()?;
        self.expect(Token::Colon)?;
        self.skip_newlines();
        let methods = self.parse_delimited_block(|p| {
            let item = p.parse_function(Vec::new(), Visibility::Private)?;
            match item {
                Item::Function(f) => Ok(f),
                _ => Err(p.error("impl block may only contain functions")),
            }
        })?;
        Ok(Item::Impl(Impl { target, methods }))
    }

    fn parse_extern_fn(&mut self, attrs: Vec<Attribute>) -> Result<Item, ParseError> {
        self.expect(Token::Extern)?;
        let abi = if self.peek() == &Token::LParen {
            self.expect(Token::LParen)?;
            let s = self.expect_string()?;
            self.expect(Token::RParen)?;
            Some(s)
        } else {
            None
        };
        let mut attrs = attrs;
        if let Some(abi) = abi {
            attrs.push(Attribute::Extern(abi));
        }
        self.expect(Token::Def)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;
        let params = self.parse_params()?;
        let ret = if self.eat(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        Ok(Item::ExternFn(ExternFn {
            attrs,
            name,
            generics,
            params,
            ret,
        }))
    }

    fn parse_const_item(&mut self, vis: Visibility) -> Result<Item, ParseError> {
        // `const` and `static` are treated identically in the AST.
        self.advance();
        let name = self.expect_ident()?;
        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(Item::Const(ConstItem {
            vis,
            name,
            ty,
            value,
        }))
    }

    fn parse_generics(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.eat(&Token::LBracket) {
            return Ok(Vec::new());
        }
        let mut names = vec![self.expect_ident()?];
        while self.eat(&Token::Comma) {
            names.push(self.expect_ident()?);
        }
        self.expect(Token::RBracket)?;
        Ok(names)
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.parse_delimited(Token::LParen, Token::RParen, |p| {
            let name = p.expect_ident()?;
            p.expect(Token::Colon)?;
            let ty = p.parse_type()?;
            Ok(Param { name, ty })
        })
    }

    fn parse_field_block(&mut self) -> Result<Vec<Field>, ParseError> {
        self.expect(Token::Colon)?;
        self.parse_delimited_block(|p| {
            let name = p.expect_ident()?;
            p.expect(Token::Colon)?;
            let ty = p.parse_type()?;
            Ok(Field { name, ty })
        })
    }

    fn parse_variant_block(&mut self) -> Result<Vec<Variant>, ParseError> {
        self.expect(Token::Colon)?;
        self.parse_delimited_block(|p| {
            let vname = p.expect_ident()?;
            let payload = if p.eat(&Token::LParen) {
                let ty = p.parse_type()?;
                p.expect(Token::RParen)?;
                Some(ty)
            } else {
                None
            };
            Ok(Variant {
                name: vname,
                payload,
            })
        })
    }

    /// Parse a comma-separated list enclosed by `open` and `close`, tolerating
    /// optional newlines/indentation inside the delimiters.
    fn parse_delimited<T>(
        &mut self,
        open: Token,
        close: Token,
        mut parse_elem: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        self.expect(open)?;
        let mut items = Vec::new();
        self.skip_newlines();
        if self.peek() == &Token::Indent {
            self.advance();
        }
        if self.peek() == &close {
            self.expect(close)?;
            return Ok(items);
        }
        loop {
            items.push(parse_elem(self)?);
            self.skip_newlines();
            if self.eat(&Token::Comma) {
                self.skip_newlines();
                self.eat(&Token::Dedent);
                if self.peek() == &close {
                    self.expect(close)?;
                    break;
                }
                continue;
            }
            self.eat(&Token::Dedent);
            self.expect(close)?;
            break;
        }
        Ok(items)
    }

    /// Parse a brace-delimited or colon-plus-indentation block of elements.
    fn parse_delimited_block<T>(
        &mut self,
        mut parse_elem: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        self.skip_newlines();
        if self.eat(&Token::LBrace) {
            let mut items = Vec::new();
            self.skip_newlines();
            if self.peek() == &Token::Indent {
                self.advance();
            }
            while self.peek() != &Token::RBrace && !self.at_end() {
                items.push(parse_elem(self)?);
                self.skip_newlines();
            }
            self.eat(&Token::Dedent);
            self.expect(Token::RBrace)?;
            return Ok(items);
        }

        self.expect(Token::Indent)?;
        let mut items = Vec::new();
        self.skip_newlines();
        while self.peek() != &Token::Dedent && !self.at_end() {
            items.push(parse_elem(self)?);
            self.skip_newlines();
        }
        self.expect(Token::Dedent)?;
        Ok(items)
    }

    // ------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        self.skip_newlines();
        let mut ty = self.parse_type_atom()?;
        loop {
            self.skip_newlines();
            if self.eat(&Token::LBracket) {
                let inner = self.parse_type()?;
                if self.eat(&Token::Semicolon) {
                    let size = self.parse_expr()?;
                    self.skip_newlines();
                    self.eat(&Token::Dedent);
                    self.expect(Token::RBracket)?;
                    ty = TypeExpr::Array(Box::new(inner), Box::new(size));
                } else {
                    self.skip_newlines();
                    self.eat(&Token::Dedent);
                    self.expect(Token::RBracket)?;
                    ty = match ty {
                        TypeExpr::Name(ref n) if n == "ptr" => TypeExpr::Pointer(Box::new(inner)),
                        TypeExpr::Name(ref n) if n == "slice" => TypeExpr::Slice(Box::new(inner)),
                        TypeExpr::Name(ref n) if n == "own" => TypeExpr::Own(Box::new(inner)),
                        TypeExpr::Name(ref n) if n == "ref" => TypeExpr::Ref(Box::new(inner)),
                        TypeExpr::Name(ref n) if n == "refmut" => TypeExpr::RefMut(Box::new(inner)),
                        _ => TypeExpr::Slice(Box::new(inner)),
                    };
                }
            } else if self.eat(&Token::Arrow) {
                let params = if let TypeExpr::Tuple(ts) = ty {
                    ts
                } else {
                    vec![ty]
                };
                let ret = if self.peek() == &Token::LBrace
                    || self.peek() == &Token::Indent
                    || self.peek() == &Token::Newline
                    || self.peek() == &Token::Eof
                {
                    None
                } else {
                    Some(Box::new(self.parse_type()?))
                };
                ty = TypeExpr::Function { params, ret };
            } else {
                break;
            }
        }
        Ok(ty)
    }

    fn parse_type_atom(&mut self) -> Result<TypeExpr, ParseError> {
        self.skip_newlines();
        match self.advance() {
            Token::Ident(name) => Ok(TypeExpr::Name(name)),
            Token::Star => Ok(TypeExpr::Pointer(Box::new(self.parse_type_atom()?))),
            Token::LParen => {
                let mut ts = Vec::new();
                self.skip_newlines();
                if self.peek() == &Token::Indent {
                    self.advance();
                }
                if self.peek() == &Token::RParen {
                    self.expect(Token::RParen)?;
                    return Ok(TypeExpr::Tuple(ts));
                }
                loop {
                    ts.push(self.parse_type()?);
                    self.skip_newlines();
                    if self.eat(&Token::Comma) {
                        self.skip_newlines();
                        self.eat(&Token::Dedent);
                        if self.peek() == &Token::RParen {
                            self.expect(Token::RParen)?;
                            break;
                        }
                        continue;
                    }
                    self.eat(&Token::Dedent);
                    self.expect(Token::RParen)?;
                    break;
                }
                if ts.len() == 1 {
                    Ok(ts.into_iter().next().unwrap())
                } else {
                    Ok(TypeExpr::Tuple(ts))
                }
            }
            Token::LBracket => {
                let inner = self.parse_type()?;
                if self.eat(&Token::Semicolon) {
                    let size = self.parse_expr()?;
                    self.skip_newlines();
                    self.eat(&Token::Dedent);
                    self.expect(Token::RBracket)?;
                    Ok(TypeExpr::Array(Box::new(inner), Box::new(size)))
                } else {
                    self.skip_newlines();
                    self.eat(&Token::Dedent);
                    self.expect(Token::RBracket)?;
                    Ok(TypeExpr::Slice(Box::new(inner)))
                }
            }
            other => Err(self.error(format!("expected type, found {:?}", other))),
        }
    }

    // ------------------------------------------------------------------
    // Blocks and statements
    // ------------------------------------------------------------------

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.skip_newlines();
        // Allow an optional colon before indentation-style blocks.
        self.eat(&Token::Colon);
        self.skip_newlines();
        if self.eat(&Token::LBrace) {
            let mut stmts = Vec::new();
            self.skip_newlines();
            if self.peek() == &Token::Indent {
                self.advance();
            }
            while self.peek() != &Token::RBrace
                && self.peek() != &Token::Dedent
                && !self.at_end()
            {
                stmts.push(self.parse_stmt()?);
                self.skip_newlines();
            }
            self.eat(&Token::Dedent);
            self.expect(Token::RBrace)?;
            Ok(Block { stmts })
        } else if self.eat(&Token::Indent) {
            let mut stmts = Vec::new();
            self.skip_newlines();
            while self.peek() != &Token::Dedent && !self.at_end() {
                stmts.push(self.parse_stmt()?);
                self.skip_newlines();
            }
            self.expect(Token::Dedent)?;
            Ok(Block { stmts })
        } else {
            // Single-statement body (e.g. `if x: return y`).
            let stmt = self.parse_stmt()?;
            Ok(Block { stmts: vec![stmt] })
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        match self.peek() {
            Token::Let => self.parse_let(),
            Token::Var => self.parse_var(),
            Token::Return => self.parse_return(),
            Token::If => self.parse_if_stmt(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Match => self.parse_match_stmt(),
            Token::Unsafe => self.parse_unsafe(),
            Token::Loop => self.parse_loop(),
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::LBrace => {
                let block = self.parse_block()?;
                Ok(Stmt::Expr(Expr::Block(block)))
            }
            _ => self.parse_expr_or_assign(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Let)?;
        let name = self.expect_ident()?;
        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.eat(&Token::Assign) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::Let(LetStmt { name, ty, value }))
    }

    fn parse_var(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Var)?;
        let name = self.expect_ident()?;
        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.eat(&Token::Assign) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::Var(VarStmt { name, ty, value }))
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Return)?;
        let value = if self.is_at_stmt_boundary() {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return(value))
    }

    fn is_at_stmt_boundary(&self) -> bool {
        matches!(
            self.peek(),
            Token::Newline
                | Token::Dedent
                | Token::RBrace
                | Token::Eof
                | Token::Else
                | Token::Elif
                | Token::Case
        )
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::If)?;
        let condition = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let mut elifs = Vec::new();
        while self.eat(&Token::Elif) {
            let cond = self.parse_expr()?;
            let block = self.parse_block()?;
            elifs.push((cond, block));
        }
        let else_block = if self.eat(&Token::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::If(IfStmt {
            condition,
            then_block,
            elifs,
            else_block,
        }))
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::For)?;
        let var = self.expect_ident()?;
        self.expect(Token::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::For(ForStmt { var, iter, body }))
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::While)?;
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While(WhileStmt { condition, body }))
    }

    fn parse_match_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Match)?;
        let scrutinee = self.parse_expr()?;
        self.expect(Token::Colon)?;
        self.skip_newlines();
        let cases = self.parse_match_cases()?;
        Ok(Stmt::Match(MatchStmt { scrutinee, cases }))
    }

    fn parse_match_cases(&mut self) -> Result<Vec<MatchCase>, ParseError> {
        self.parse_delimited_block(|p| {
            p.expect(Token::Case)?;
            let pattern = p.parse_pattern()?;
            let body = p.parse_block()?;
            Ok(MatchCase { pattern, body })
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.peek() == &Token::Minus {
            self.advance();
            return match self.parse_literal()? {
                Literal::Int(i) => Ok(Pattern::Literal(Literal::Int(-i))),
                Literal::Float(f) => Ok(Pattern::Literal(Literal::Float(-f))),
                other => Err(self.error(format!("cannot negate literal {:?}", other))),
            };
        }
        match self.peek() {
            Token::IntLit(_)
            | Token::FloatLit(_)
            | Token::StringLit(_)
            | Token::CharLit(_)
            | Token::True
            | Token::False
            | Token::Null => Ok(Pattern::Literal(self.parse_literal()?)),
            Token::Ident(name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                if self.eat(&Token::LParen) {
                    let mut pats = vec![Pattern::Ident(name)];
                    if !self.eat(&Token::RParen) {
                        loop {
                            pats.push(self.parse_pattern()?);
                            self.skip_newlines();
                            if !self.eat(&Token::Comma) {
                                break;
                            }
                        }
                        self.expect(Token::RParen)?;
                    }
                    Ok(Pattern::Tuple(pats))
                } else {
                    Ok(Pattern::Ident(name))
                }
            }
            Token::LParen => {
                self.advance();
                let mut pats = Vec::new();
                if !self.eat(&Token::RParen) {
                    loop {
                        pats.push(self.parse_pattern()?);
                        self.skip_newlines();
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(Token::RParen)?;
                }
                Ok(Pattern::Tuple(pats))
            }
            _ => Err(self.error(format!("expected pattern, found {:?}", self.peek()))),
        }
    }

    fn parse_unsafe(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Unsafe)?;
        let block = self.parse_block()?;
        Ok(Stmt::UnsafeBlock(block))
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Loop)?;
        let block = self.parse_block()?;
        Ok(Stmt::Loop(block))
    }

    fn parse_expr_or_assign(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;
        self.skip_newlines();
        if self.eat(&Token::Assign) {
            let value = self.parse_expr()?;
            Ok(Stmt::Assign(AssignStmt { target: expr, value }))
        } else {
            Ok(Stmt::Expr(expr))
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat(&Token::OrOr) {
            let right = self.parse_and()?;
            left = Expr::Binary(BinaryExpr {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_range()?;
        while self.eat(&Token::AndAnd) {
            let right = self.parse_range()?;
            left = Expr::Binary(BinaryExpr {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        if self.eat(&Token::DotDot) {
            let end = if self.at_expr_start() {
                Some(Box::new(self.parse_range()?))
            } else {
                None
            };
            return Ok(Expr::Range(RangeExpr {
                start: None,
                end,
                inclusive: false,
            }));
        }
        if self.eat(&Token::DotDotEq) {
            let end = if self.at_expr_start() {
                Some(Box::new(self.parse_range()?))
            } else {
                None
            };
            return Ok(Expr::Range(RangeExpr {
                start: None,
                end,
                inclusive: true,
            }));
        }

        let mut left = self.parse_bitor()?;
        if self.eat(&Token::DotDot) {
            let end = if self.at_expr_start() {
                Some(Box::new(self.parse_range()?))
            } else {
                None
            };
            left = Expr::Range(RangeExpr {
                start: Some(Box::new(left)),
                end,
                inclusive: false,
            });
        } else if self.eat(&Token::DotDotEq) {
            let end = if self.at_expr_start() {
                Some(Box::new(self.parse_range()?))
            } else {
                None
            };
            left = Expr::Range(RangeExpr {
                start: Some(Box::new(left)),
                end,
                inclusive: true,
            });
        }
        Ok(left)
    }

    fn parse_bitor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitxor()?;
        while self.eat(&Token::Or) {
            let right = self.parse_bitxor()?;
            left = Expr::Binary(BinaryExpr {
                op: BinOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_bitxor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitand()?;
        while self.eat(&Token::Xor) {
            let right = self.parse_bitand()?;
            left = Expr::Binary(BinaryExpr {
                op: BinOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_bitand(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.eat(&Token::And) {
            let right = self.parse_equality()?;
            left = Expr::Binary(BinaryExpr {
                op: BinOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_rel()?;
        loop {
            if self.eat(&Token::Eq) {
                let right = self.parse_rel()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Eq,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Ne) {
                let right = self.parse_rel()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Ne,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_rel(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        loop {
            if self.eat(&Token::Lt) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Lt,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Le) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Le,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Gt) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Gt,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Ge) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Ge,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add()?;
        loop {
            if self.eat(&Token::Shl) {
                let right = self.parse_add()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Shl,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Shr) {
                let right = self.parse_add()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Shr,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        loop {
            if self.eat(&Token::Plus) {
                let right = self.parse_mul()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Minus) {
                let right = self.parse_mul()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Sub,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_power()?;
        loop {
            if self.eat(&Token::Star) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Slash) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::FloorDiv) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::FloorDiv,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Percent) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr {
                    op: BinOp::Mod,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_unary()?;
        if self.eat(&Token::Power) {
            let right = self.parse_power()?;
            Ok(Expr::Binary(BinaryExpr {
                op: BinOp::Power,
                left: Box::new(left),
                right: Box::new(right),
            }))
        } else {
            Ok(left)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        if self.eat(&Token::Minus) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr {
                op: UnOp::Neg,
                operand: Box::new(operand),
            }));
        }
        if self.eat(&Token::Bang) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr {
                op: UnOp::Not,
                operand: Box::new(operand),
            }));
        }
        if self.eat(&Token::Tilde) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr {
                op: UnOp::BitNot,
                operand: Box::new(operand),
            }));
        }
        if self.eat(&Token::Star) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Deref(DerefExpr {
                expr: Box::new(operand),
            }));
        }
        if self.eat(&Token::And) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Ref(RefExpr {
                expr: Box::new(operand),
            }));
        }
        if self.eat(&Token::Plus) {
            return self.parse_unary();
        }
        let expr = self.parse_primary()?;
        self.parse_postfix(expr)
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
            // A newline normally terminates an expression (it is the statement
            // boundary). Only cross it to continue a postfix chain when a
            // postfix operator actually follows on the next non-blank line;
            // otherwise a leading `*`/`+`/`&`/... on the next line would be
            // swallowed into the current expression (e.g. a `*p = x` deref
            // assignment on the following line being parsed as multiplication).
            if matches!(self.peek(), Token::Newline) {
                let ahead = self.peek_past_newlines();
                let continues = matches!(
                    ahead,
                    Token::LParen | Token::LBracket | Token::Dot | Token::As
                ) || (matches!(ahead, Token::LBrace) && matches!(expr, Expr::Ident(_)));
                if !continues {
                    break;
                }
                self.skip_newlines();
            }
            if self.eat(&Token::LParen) {
                let mut args = Vec::new();
                self.skip_newlines();
                if !self.eat(&Token::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        self.skip_newlines();
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                        self.skip_newlines();
                        if self.eat(&Token::RParen) {
                            break;
                        }
                    }
                    self.expect(Token::RParen)?;
                }
                expr = Expr::Call(CallExpr {
                    callee: Box::new(expr),
                    args,
                });
            } else if self.eat(&Token::LBracket) {
                let index = self.parse_expr()?;
                self.expect(Token::RBracket)?;
                expr = Expr::Index(IndexExpr {
                    object: Box::new(expr),
                    index: Box::new(index),
                });
            } else if self.eat(&Token::Dot) {
                let field = self.expect_ident()?;
                expr = Expr::Field(FieldExpr {
                    object: Box::new(expr),
                    field,
                });
            } else if self.peek() == &Token::LBrace && matches!(expr, Expr::Ident(_)) {
                // Struct literal: Name { field: expr, ... }
                self.advance();
                let name = match expr {
                    Expr::Ident(n) => n,
                    _ => unreachable!(),
                };
                let mut fields = Vec::new();
                self.skip_newlines();
                if !self.eat(&Token::RBrace) {
                    loop {
                        let fname = self.expect_ident()?;
                        self.expect(Token::Colon)?;
                        let fvalue = self.parse_expr()?;
                        fields.push((fname, fvalue));
                        self.skip_newlines();
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                        self.skip_newlines();
                        if self.eat(&Token::RBrace) {
                            break;
                        }
                    }
                    if fields.is_empty() || self.peek() != &Token::RBrace {
                        self.expect(Token::RBrace)?;
                    }
                }
                expr = Expr::StructLiteral { name, fields };
            } else if self.peek() == &Token::As {
                // Postfix cast: `expr as Type`.
                self.advance();
                let ty = self.parse_type()?;
                expr = Expr::Cast(CastExpr {
                    expr: Box::new(expr),
                    ty: Box::new(ty),
                });
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        match self.peek() {
            Token::Sizeof => self.parse_sizeof(),
            Token::Offsetof => self.parse_offsetof(),
            Token::Asm => self.parse_asm(),
            Token::If => self.parse_if_expr(),
            Token::Match => {
                let m = self.parse_match_expr()?;
                Ok(Expr::Match(m))
            }
            Token::Loop => {
                self.advance();
                let block = self.parse_block()?;
                Ok(Expr::Loop(block))
            }
            Token::Unsafe => {
                self.advance();
                let block = self.parse_block()?;
                Ok(Expr::UnsafeBlock(block))
            }
            Token::LBrace => {
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
            }
            Token::LParen => {
                self.advance();
                let mut exprs = Vec::new();
                self.skip_newlines();
                let mut closed = false;
                if !self.eat(&Token::RParen) {
                    loop {
                        exprs.push(self.parse_expr()?);
                        self.skip_newlines();
                        if self.eat(&Token::RParen) {
                            closed = true;
                            break;
                        }
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                        self.skip_newlines();
                    }
                } else {
                    closed = true;
                }
                if !closed {
                    self.expect(Token::RParen)?;
                }
                if exprs.len() == 1 {
                    Ok(exprs.into_iter().next().unwrap())
                } else {
                    Ok(Expr::Tuple(exprs))
                }
            }
            Token::LBracket => {
                self.advance();
                let mut exprs = Vec::new();
                self.skip_newlines();
                if !self.eat(&Token::RBracket) {
                    loop {
                        exprs.push(self.parse_expr()?);
                        self.skip_newlines();
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                        self.skip_newlines();
                        if self.eat(&Token::RBracket) {
                            break;
                        }
                    }
                    if exprs.is_empty() || self.peek() != &Token::RBracket {
                        self.expect(Token::RBracket)?;
                    }
                }
                Ok(Expr::Array(exprs))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_sizeof(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::Sizeof)?;
        self.expect(Token::LParen)?;
        let ty = self.parse_type()?;
        self.expect(Token::RParen)?;
        Ok(Expr::SizeOf(SizeOfExpr { ty }))
    }

    fn parse_offsetof(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::Offsetof)?;
        self.expect(Token::LParen)?;
        let ty = self.parse_type()?;
        self.expect(Token::Comma)?;
        let field = self.expect_ident()?;
        self.expect(Token::RParen)?;
        Ok(Expr::OffsetOf(OffsetOfExpr { ty, field }))
    }

    fn parse_asm(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::Asm)?;
        self.expect(Token::LParen)?;
        let template = self.expect_string()?;
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut clobbers = Vec::new();
        self.skip_newlines();
        if self.eat(&Token::Comma) {
            self.skip_newlines();
            if !self.eat(&Token::RParen) {
                loop {
                    let kind = self.parse_asm_kind()?;
                    let constraint = self.expect_string()?;
                    match kind.as_str() {
                        "in" => {
                            let expr = self.parse_expr()?;
                            inputs.push(AsmOperand {
                                constraint,
                                expr: Box::new(expr),
                            });
                        }
                        "out" => {
                            let expr = self.parse_expr()?;
                            outputs.push(AsmOperand {
                                constraint,
                                expr: Box::new(expr),
                            });
                        }
                        "clobber" => {
                            clobbers.push(constraint);
                        }
                        _ => return Err(self.error(format!("unknown asm operand {}", kind))),
                    }
                    self.skip_newlines();
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                    self.skip_newlines();
                    if self.eat(&Token::RParen) {
                        break;
                    }
                }
                self.expect(Token::RParen)?;
            }
        } else {
            self.expect(Token::RParen)?;
        }
        Ok(Expr::Asm(AsmExpr {
            template,
            inputs,
            outputs,
            clobbers,
        }))
    }

    fn parse_asm_kind(&mut self) -> Result<String, ParseError> {
        if self.peek() == &Token::In {
            self.advance();
            Ok("in".to_string())
        } else {
            self.expect_ident()
        }
    }

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::If)?;
        let condition = Box::new(self.parse_expr()?);
        let then_block = self.parse_block()?;
        let else_block = if self.eat(&Token::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Expr::If(IfExpr {
            condition,
            then_block,
            else_block,
        }))
    }

    fn parse_match_expr(&mut self) -> Result<MatchExpr, ParseError> {
        self.expect(Token::Match)?;
        let scrutinee = Box::new(self.parse_expr()?);
        self.expect(Token::Colon)?;
        self.skip_newlines();
        let cases = self.parse_match_cases()?;
        Ok(MatchExpr { scrutinee, cases })
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let lit = self.try_parse_literal()?;
        if let Some(l) = lit {
            return self.parse_postfix(Expr::Literal(l));
        }
        if let Token::Ident(name) = self.peek() {
            let name = name.clone();
            self.advance();
            return self.parse_postfix(Expr::Ident(name));
        }
        Err(self.error(format!(
            "expected expression, found {:?}",
            self.peek()
        )))
    }

    fn try_parse_literal(&mut self) -> Result<Option<Literal>, ParseError> {
        match self.peek() {
            Token::IntLit(s) => {
                let s = s.clone();
                self.advance();
                Ok(Some(parse_int(&s)))
            }
            Token::FloatLit(s) => {
                let s = s.clone();
                self.advance();
                match s.parse::<f64>() {
                    Ok(f) => Ok(Some(Literal::Float(f))),
                    Err(_) => Err(self.error(format!("invalid float literal {}", s))),
                }
            }
            Token::StringLit(s) => {
                let s = s.clone();
                self.advance();
                Ok(Some(Literal::String(s)))
            }
            Token::CharLit(s) => {
                let s = s.clone();
                self.advance();
                let c = s.chars().next().unwrap_or('\0');
                Ok(Some(Literal::Char(c)))
            }
            Token::True => {
                self.advance();
                Ok(Some(Literal::Bool(true)))
            }
            Token::False => {
                self.advance();
                Ok(Some(Literal::Bool(false)))
            }
            Token::Null => {
                self.advance();
                Ok(Some(Literal::Null))
            }
            _ => Ok(None),
        }
    }

    fn at_expr_start(&self) -> bool {
        matches!(
            self.peek(),
            Token::IntLit(_)
                | Token::FloatLit(_)
                | Token::StringLit(_)
                | Token::CharLit(_)
                | Token::True
                | Token::False
                | Token::Null
                | Token::Ident(_)
                | Token::LParen
                | Token::LBracket
                | Token::LBrace
                | Token::Minus
                | Token::Bang
                | Token::Tilde
                | Token::Star
                | Token::And
                | Token::Plus
                | Token::Sizeof
                | Token::Offsetof
                | Token::Asm
                | Token::Unsafe
                | Token::If
                | Token::Match
                | Token::Loop
                | Token::Break
                | Token::Continue
                | Token::DotDot
                | Token::DotDotEq
        )
    }

    fn expect_int(&mut self) -> Result<i64, ParseError> {
        match self.advance() {
            Token::IntLit(s) => match parse_int(&s) {
                Literal::Int(i) => Ok(i),
                _ => unreachable!(),
            },
            other => Err(self.error(format!("expected integer, found {:?}", other))),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::StringLit(s) => Ok(s),
            other => Err(self.error(format!("expected string literal, found {:?}", other))),
        }
    }

    fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        self.try_parse_literal()?
            .ok_or_else(|| self.error("expected literal".to_string()))
    }
}

// ------------------------------------------------------------------
// Literal parsing helpers
// ------------------------------------------------------------------

fn parse_int(s: &str) -> Literal {
    let s = s.trim_end_matches(|c: char| c.is_alphabetic());
    let s_clean = s.replace('_', "");
    if s_clean.starts_with("0x") || s_clean.starts_with("0X") {
        Literal::Int(i64::from_str_radix(&s_clean[2..], 16).unwrap_or(0))
    } else if s_clean.starts_with("0b") || s_clean.starts_with("0B") {
        Literal::Int(i64::from_str_radix(&s_clean[2..], 2).unwrap_or(0))
    } else if s_clean.starts_with("0o") || s_clean.starts_with("0O") {
        Literal::Int(i64::from_str_radix(&s_clean[2..], 8).unwrap_or(0))
    } else {
        Literal::Int(s_clean.parse::<i64>().unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let m = parse_ok(
            "package test\n\ndef add(a: i32, b: i32) -> i32:\n    return a + b\n",
        );
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
        assert!(matches!(
            stmt.condition,
            Expr::Literal(Literal::Bool(true))
        ));
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
        assert!(matches!(
            f.body.as_ref().unwrap().stmts[0],
            Stmt::Return(_)
        ));
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
        assert_eq!(body.stmts.len(), 2, "expected two statements, got {}", body.stmts.len());
        assert!(matches!(body.stmts[0], Stmt::Var(_)), "first stmt should be a var");
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
