use super::*;
use crate::lexer::Token;

/// Whether `name` is a built-in primitive type name (dual-spelled).
pub(super) fn is_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "void"
            | "bool"
            | "char"
            | "usize"
            | "isize"
            | "i8" | "int8"
            | "i16" | "int16"
            | "i32" | "int32" | "int"
            | "i64" | "int64"
            | "i128" | "int128"
            | "u8" | "uint8" | "byte"
            | "u16" | "uint16"
            | "u32" | "uint32" | "uint"
            | "u64" | "uint64"
            | "u128" | "uint128"
            | "f32" | "float32"
            | "f64" | "float64" | "float"
    )
}

impl Parser {
    /// After the first type argument of a generic application has been parsed,
    /// consume the remaining comma-separated arguments.  The closing `]` is
    /// consumed by the caller (it is shared with the slice/pointer forms).
    fn finish_generic_app(
        &mut self,
        base: String,
        first: TypeExpr,
    ) -> Result<TypeExpr, ParseError> {
        let mut args = vec![first];
        self.skip_newlines();
        while self.eat(&Token::Comma) {
            self.skip_newlines();
            args.push(self.parse_type()?);
            self.skip_newlines();
        }
        Ok(TypeExpr::GenericApp { base, args })
    }

    pub(super) fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        self.skip_newlines();
        self.parse_type_inner()
    }

    pub(super) fn parse_type_noskip(&mut self) -> Result<TypeExpr, ParseError> {
        let mut ty = self.parse_type_atom_noskip()?;
        loop {
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
                        TypeExpr::Name(n) if !is_primitive_name(&n) => {
                            self.finish_generic_app(n, inner)?
                        }
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

    fn parse_type_inner(&mut self) -> Result<TypeExpr, ParseError> {
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
                        TypeExpr::Name(n) if !is_primitive_name(&n) => {
                            self.finish_generic_app(n, inner)?
                        }
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

    pub(super) fn parse_type_atom(&mut self) -> Result<TypeExpr, ParseError> {
        self.skip_newlines();
        self.parse_type_atom_inner()
    }

    fn parse_type_atom_noskip(&mut self) -> Result<TypeExpr, ParseError> {
        self.parse_type_atom_inner()
    }

    fn parse_type_atom_inner(&mut self) -> Result<TypeExpr, ParseError> {
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
}
