use super::*;
use super::parser::parse_int;
use crate::lexer::Token;

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    pub(super) fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat(&Token::OrOr) {
            let right = self.parse_and()?;
            left = Expr::Binary(BinaryExpr { span: self.current_span(),
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_range()?;
        while self.eat(&Token::AndAnd) {
            let right = self.parse_range()?;
            left = Expr::Binary(BinaryExpr { span: self.current_span(),
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_range(&mut self) -> Result<Expr, ParseError> {
        if self.eat(&Token::DotDot) {
            let end = if self.at_expr_start() {
                Some(Box::new(self.parse_range()?))
            } else {
                None
            };
            return Ok(Expr::Range(RangeExpr { span: self.current_span(),
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
            return Ok(Expr::Range(RangeExpr { span: self.current_span(),
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
            left = Expr::Range(RangeExpr { span: self.current_span(),
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
            left = Expr::Range(RangeExpr { span: self.current_span(),
                start: Some(Box::new(left)),
                end,
                inclusive: true,
            });
        }
        Ok(left)
    }

    pub(super) fn parse_bitor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitxor()?;
        while self.eat(&Token::Or) {
            let right = self.parse_bitxor()?;
            left = Expr::Binary(BinaryExpr { span: self.current_span(),
                op: BinOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_bitxor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitand()?;
        while self.eat(&Token::Xor) {
            let right = self.parse_bitand()?;
            left = Expr::Binary(BinaryExpr { span: self.current_span(),
                op: BinOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_bitand(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.eat(&Token::And) {
            let right = self.parse_equality()?;
            left = Expr::Binary(BinaryExpr { span: self.current_span(),
                op: BinOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_rel()?;
        loop {
            if self.eat(&Token::Eq) {
                let right = self.parse_rel()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Eq,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Ne) {
                let right = self.parse_rel()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
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

    pub(super) fn parse_rel(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        loop {
            if self.eat(&Token::Lt) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Lt,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Le) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Le,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Gt) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Gt,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Ge) {
                let right = self.parse_shift()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
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

    pub(super) fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add()?;
        loop {
            if self.eat(&Token::Shl) {
                let right = self.parse_add()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Shl,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Shr) {
                let right = self.parse_add()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
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

    pub(super) fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        loop {
            if self.eat(&Token::Plus) {
                let right = self.parse_mul()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Minus) {
                let right = self.parse_mul()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
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

    pub(super) fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_power()?;
        loop {
            if self.eat(&Token::Star) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Slash) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::FloorDiv) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
                    op: BinOp::FloorDiv,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.eat(&Token::Percent) {
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryExpr { span: self.current_span(),
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

    pub(super) fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_unary()?;
        if self.eat(&Token::Power) {
            let right = self.parse_power()?;
            Ok(Expr::Binary(BinaryExpr { span: self.current_span(),
                op: BinOp::Power,
                left: Box::new(left),
                right: Box::new(right),
            }))
        } else {
            Ok(left)
        }
    }

    pub(super) fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        if self.eat(&Token::Minus) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { span: self.current_span(),
                op: UnOp::Neg,
                operand: Box::new(operand),
            }));
        }
        if self.eat(&Token::Bang) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { span: self.current_span(),
                op: UnOp::Not,
                operand: Box::new(operand),
            }));
        }
        if self.eat(&Token::Tilde) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { span: self.current_span(),
                op: UnOp::BitNot,
                operand: Box::new(operand),
            }));
        }
        if self.eat(&Token::Star) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Deref(DerefExpr { span: self.current_span(),
                expr: Box::new(operand),
            }));
        }
        if self.eat(&Token::And) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Ref(RefExpr { span: self.current_span(),
                expr: Box::new(operand),
            }));
        }
        if self.eat(&Token::Plus) {
            return self.parse_unary();
        }
        let expr = self.parse_primary()?;
        self.parse_postfix(expr)
    }

    pub(super) fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
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
                expr = Expr::Call(CallExpr { span: self.current_span(),
                    callee: Box::new(expr),
                    args,
                });
            } else if self.eat(&Token::LBracket) {
                let index = self.parse_expr()?;
                self.expect(Token::RBracket)?;
                expr = Expr::Index(IndexExpr { span: self.current_span(),
                    object: Box::new(expr),
                    index: Box::new(index),
                });
            } else if self.eat(&Token::Dot) {
                let field = self.expect_ident()?;
                expr = Expr::Field(FieldExpr { span: self.current_span(),
                    object: Box::new(expr),
                    field,
                });
            } else if self.peek() == &Token::LBrace && matches!(expr, Expr::Ident(_)) {
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
                    self.expect(Token::RBrace)?;
                }
                expr = Expr::StructLiteral { name, fields };
            } else if self.peek() == &Token::As {
                self.advance();
                let ty = self.parse_type_noskip()?;
                expr = Expr::Cast(CastExpr { span: self.current_span(),
                    expr: Box::new(expr),
                    ty: Box::new(ty),
                });
            } else {
                break;
            }
        }
        Ok(expr)
    }

    pub(super) fn parse_primary(&mut self) -> Result<Expr, ParseError> {
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
                    self.expect(Token::RBracket)?;
                }
                Ok(Expr::Array(exprs))
            }
            _ => self.parse_atom(),
        }
    }

    pub(super) fn parse_sizeof(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::Sizeof)?;
        self.expect(Token::LParen)?;
        let ty = self.parse_type()?;
        self.expect(Token::RParen)?;
        Ok(Expr::SizeOf(SizeOfExpr { span: self.current_span(), ty }))
    }

    pub(super) fn parse_offsetof(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::Offsetof)?;
        self.expect(Token::LParen)?;
        let ty = self.parse_type()?;
        self.expect(Token::Comma)?;
        let field = self.expect_ident()?;
        self.expect(Token::RParen)?;
        Ok(Expr::OffsetOf(OffsetOfExpr { span: self.current_span(), ty, field }))
    }

    pub(super) fn parse_asm(&mut self) -> Result<Expr, ParseError> {
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
        Ok(Expr::Asm(AsmExpr { span: self.current_span(),
            template,
            inputs,
            outputs,
            clobbers,
        }))
    }

    pub(super) fn parse_asm_kind(&mut self) -> Result<String, ParseError> {
        if self.peek() == &Token::In {
            self.advance();
            Ok("in".to_string())
        } else {
            self.expect_ident()
        }
    }

    pub(super) fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::If)?;
        let condition = Box::new(self.parse_expr()?);
        let then_block = self.parse_block()?;
        let else_block = if self.eat(&Token::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Expr::If(IfExpr { span: self.current_span(),
            condition,
            then_block,
            else_block,
        }))
    }

    pub(super) fn parse_match_expr(&mut self) -> Result<MatchExpr, ParseError> {
        self.expect(Token::Match)?;
        let scrutinee = Box::new(self.parse_expr()?);
        self.expect(Token::Colon)?;
        self.skip_newlines();
        let cases = self.parse_match_cases()?;
        Ok(MatchExpr { span: self.current_span(), scrutinee, cases })
    }

    pub(super) fn parse_atom(&mut self) -> Result<Expr, ParseError> {
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

    pub(super) fn try_parse_literal(&mut self) -> Result<Option<Literal>, ParseError> {
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

    pub(super) fn at_expr_start(&self) -> bool {
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

    pub(super) fn expect_int(&mut self) -> Result<i64, ParseError> {
        match self.advance() {
            Token::IntLit(s) => match parse_int(&s) {
                Literal::Int(i) => Ok(i),
                _ => unreachable!(),
            },
            other => Err(self.error(format!("expected integer, found {:?}", other))),
        }
    }

    pub(super) fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::StringLit(s) => Ok(s),
            other => Err(self.error(format!("expected string literal, found {:?}", other))),
        }
    }

    pub(super) fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        self.try_parse_literal()?
            .ok_or_else(|| self.error("expected literal".to_string()))
    }

}
