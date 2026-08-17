use super::*;
use crate::lexer::Token;

impl Parser {
    pub(super) fn parse_import(&mut self) -> Result<Import, ParseError> {
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

    pub(super) fn parse_path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut segments = vec![self.expect_ident()?];
        while self.eat(&Token::Dot) {
            segments.push(self.expect_ident()?);
        }
        Ok(segments)
    }

    pub(super) fn parse_ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut list = vec![self.expect_ident()?];
        while self.eat(&Token::Comma) {
            list.push(self.expect_ident()?);
        }
        Ok(list)
    }

    pub(super) fn parse_item(&mut self) -> Result<Item, ParseError> {
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
            Token::Embed => self.parse_embed_item(vis),
            other => Err(self.error(format!("expected item, found {:?}", other))),
        }
    }

    pub(super) fn parse_attrs(&mut self) -> Result<Vec<Attribute>, ParseError> {
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

    pub(super) fn parse_visibility(&mut self) -> Visibility {
        if self.eat(&Token::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    pub(super) fn parse_function(
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
        self.push_scope_generics(&generics);
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
        self.pop_scope_generics(generics.len());
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

    pub(super) fn parse_struct(
        &mut self,
        attrs: Vec<Attribute>,
        vis: Visibility,
    ) -> Result<Item, ParseError> {
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

    pub(super) fn parse_union(
        &mut self,
        attrs: Vec<Attribute>,
        vis: Visibility,
    ) -> Result<Item, ParseError> {
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

    pub(super) fn parse_enum(
        &mut self,
        attrs: Vec<Attribute>,
        vis: Visibility,
    ) -> Result<Item, ParseError> {
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

    pub(super) fn parse_impl(&mut self) -> Result<Item, ParseError> {
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

    pub(super) fn parse_extern_fn(&mut self, attrs: Vec<Attribute>) -> Result<Item, ParseError> {
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

    pub(super) fn parse_const_item(&mut self, vis: Visibility) -> Result<Item, ParseError> {
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

    pub(super) fn parse_embed_item(&mut self, vis: Visibility) -> Result<Item, ParseError> {
        self.expect(Token::Embed)?;
        let name = self.expect_ident()?;
        self.expect(Token::Assign)?;
        let path = match self.advance() {
            Token::StringLit(p) => p,
            other => {
                return Err(self.error(format!(
                    "embed path must be a string literal, found {:?}",
                    other
                )));
            }
        };
        let full = self.base_dir.join(&path);
        let data = std::fs::read(&full)
            .map_err(|_| self.error(format!("cannot read embedded file `{}`", full.display())))?;
        Ok(Item::Embed(EmbedItem { vis, name, data }))
    }

    pub(super) fn parse_generics(&mut self) -> Result<Vec<String>, ParseError> {
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

    pub(super) fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.parse_delimited(Token::LParen, Token::RParen, |p| {
            let name = p.expect_ident()?;
            p.expect(Token::Colon)?;
            let ty = p.parse_type()?;
            Ok(Param { name, ty })
        })
    }

    pub(super) fn parse_field_block(&mut self) -> Result<Vec<Field>, ParseError> {
        self.expect(Token::Colon)?;
        self.parse_delimited_block(|p| {
            let name = p.expect_ident()?;
            p.expect(Token::Colon)?;
            let ty = p.parse_type()?;
            Ok(Field { name, ty })
        })
    }

    pub(super) fn parse_variant_block(&mut self) -> Result<Vec<Variant>, ParseError> {
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
    pub(super) fn parse_delimited<T>(
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
    pub(super) fn parse_delimited_block<T>(
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
}
