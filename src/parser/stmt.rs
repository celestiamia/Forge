use super::*;
use crate::lexer::Token;

impl Parser {
    pub(super) fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.skip_newlines();
        self.eat(&Token::Colon);
        self.skip_newlines();
        if self.eat(&Token::LBrace) {
            let mut stmts = Vec::new();
            self.skip_newlines();
            if self.peek() == &Token::Indent {
                self.advance();
            }
            while self.peek() != &Token::RBrace && self.peek() != &Token::Dedent && !self.at_end() {
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
            let stmt = self.parse_stmt()?;
            Ok(Block { stmts: vec![stmt] })
        }
    }

    pub(super) fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
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

    pub(super) fn parse_let(&mut self) -> Result<Stmt, ParseError> {
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

    pub(super) fn parse_var(&mut self) -> Result<Stmt, ParseError> {
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

    pub(super) fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Return)?;
        let value = if self.is_at_stmt_boundary() {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return(value))
    }

    pub(super) fn is_at_stmt_boundary(&self) -> bool {
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

    pub(super) fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
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

    pub(super) fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::For)?;
        let var = self.expect_ident()?;
        self.expect(Token::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::For(ForStmt { var, iter, body }))
    }

    pub(super) fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::While)?;
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While(WhileStmt { condition, body }))
    }

    pub(super) fn parse_match_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Match)?;
        let scrutinee = self.parse_expr()?;
        self.expect(Token::Colon)?;
        self.skip_newlines();
        let cases = self.parse_match_cases()?;
        Ok(Stmt::Match(MatchStmt { scrutinee, cases }))
    }

    pub(super) fn parse_match_cases(&mut self) -> Result<Vec<MatchCase>, ParseError> {
        self.parse_delimited_block(|p| {
            p.expect(Token::Case)?;
            let pattern = p.parse_pattern()?;
            let body = p.parse_block()?;
            Ok(MatchCase { pattern, body })
        })
    }

    pub(super) fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
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

    pub(super) fn parse_unsafe(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Unsafe)?;
        let block = self.parse_block()?;
        Ok(Stmt::UnsafeBlock(block))
    }

    pub(super) fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Loop)?;
        let block = self.parse_block()?;
        Ok(Stmt::Loop(block))
    }

    pub(super) fn parse_expr_or_assign(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;
        self.skip_newlines();
        if self.eat(&Token::Assign) {
            let value = self.parse_expr()?;
            Ok(Stmt::Assign(AssignStmt {
                target: expr,
                value,
            }))
        } else {
            Ok(Stmt::Expr(expr))
        }
    }
}
