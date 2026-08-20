use crate::{
    ast::{
        BinaryOp, Expression, Function, Ident, ImportDecl, Literal, Program, Statement, TypeName,
        UnaryOp,
    },
    lexer::lex,
    span::{Diagnostic, Span},
    token::{Symbol, Token, TokenKind},
};

pub fn parse_source(source: &str) -> Result<Program, Diagnostic> {
    let tokens = lex(source)?;
    parse_program(&tokens)
}

pub fn parse_program(tokens: &[Token]) -> Result<Program, Diagnostic> {
    Parser { tokens, pos: 0 }.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl Parser<'_> {
    fn parse_program(&mut self) -> Result<Program, Diagnostic> {
        let start = self.expect_keyword("program")?.start;
        let name = self.expect_ident()?;
        self.expect_keyword("principal")?;
        let principal_id = self.parse_principal()?;
        self.expect_symbol(Symbol::LBrace)?;

        let mut imports = Vec::new();
        while self.at_keyword("import") {
            let import = self.parse_import(&imports)?;
            imports.push(import);
        }

        let mut main = None;
        while !self.at_symbol(Symbol::RBrace) && !self.is_eof() {
            if !self.at_keyword("fn") {
                return Err(self.error_current("P0003", "unexpected token"));
            }

            let function = self.parse_function_after_fn(main.is_some())?;
            if main.replace(function).is_some() {
                return Err(self.error_previous("P0005", "duplicate main"));
            }
        }

        let Some(main) = main else {
            return Err(self.error_current("P0004", "missing main"));
        };
        let end = self.expect_symbol(Symbol::RBrace)?.end;
        self.expect_eof()?;

        Ok(Program {
            name,
            principal_id,
            imports,
            main,
            span: Span::new(start, end),
        })
    }

    fn parse_principal(&mut self) -> Result<u64, Diagnostic> {
        let token = self.advance();
        let TokenKind::Hex(raw) = token.kind else {
            return Err(Diagnostic::new(
                "P0006",
                "invalid principal hex",
                token.span,
            ));
        };
        let digits = raw.trim_start_matches("0x").trim_start_matches("0X");
        if digits.is_empty() {
            return Err(Diagnostic::new(
                "P0006",
                "invalid principal hex",
                token.span,
            ));
        }
        u64::from_str_radix(digits, 16)
            .map_err(|_| Diagnostic::new("P0006", "invalid principal hex", token.span))
    }

    fn parse_import(&mut self, imports: &[ImportDecl]) -> Result<ImportDecl, Diagnostic> {
        let start = self.expect_keyword("import")?.start;
        let name = self.expect_ident()?;
        if imports.iter().any(|import| import.name.text == name.text) {
            return Err(Diagnostic::new("P0010", "duplicate import name", name.span));
        }

        self.expect_symbol(Symbol::Colon)?;
        self.expect_keyword("capability")?;
        self.expect_symbol(Symbol::Less)?;
        let resource = self.expect_ident()?.text;
        self.expect_symbol(Symbol::Comma)?;
        let mut rights = self.expect_ident()?.text;
        while self.match_symbol(Symbol::Pipe) {
            rights.push('|');
            rights.push_str(&self.expect_ident()?.text);
        }
        self.expect_symbol(Symbol::Greater)?;
        let end = self.expect_symbol(Symbol::Semicolon)?.end;

        Ok(ImportDecl {
            name,
            resource,
            rights,
            span: Span::new(start, end),
        })
    }

    fn parse_function_after_fn(&mut self, main_seen: bool) -> Result<Function, Diagnostic> {
        let start = self.expect_keyword("fn")?.start;
        let name = self.expect_ident()?;
        if name.text != "main" {
            return Err(Diagnostic::new(
                "P0011",
                "additional functions unsupported",
                name.span,
            ));
        }
        if main_seen {
            return Err(Diagnostic::new("P0005", "duplicate main", name.span));
        }

        self.expect_symbol(Symbol::LParen)?;
        self.expect_symbol(Symbol::RParen)?;
        self.expect_symbol(Symbol::Arrow)?;
        self.expect_keyword("unit")?;
        let (statements, end) = self.parse_block()?;
        Ok(Function {
            statements,
            span: Span::new(start, end),
        })
    }

    fn parse_block(&mut self) -> Result<(Vec<Statement>, usize), Diagnostic> {
        self.expect_symbol(Symbol::LBrace)?;
        let mut statements = Vec::new();
        while !self.at_symbol(Symbol::RBrace) && !self.is_eof() {
            statements.push(self.parse_statement()?);
        }
        let end = self.expect_symbol(Symbol::RBrace)?.end;
        Ok((statements, end))
    }

    fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        if self.at_keyword("let") {
            return self.parse_let();
        }
        if self.at_keyword("if") {
            return self.parse_if();
        }
        if self.at_keyword("while") {
            return self.parse_while();
        }
        if self.at_keyword("return") {
            let start = self.advance().span.start;
            let end = self.expect_symbol(Symbol::Semicolon)?.end;
            return Ok(Statement::Return {
                span: Span::new(start, end),
            });
        }

        let expr = self.parse_expression(0)?;
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(Statement::Expr(expr))
    }

    fn parse_let(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword("let")?.start;
        let name = self.expect_ident()?;
        self.expect_symbol(Symbol::Colon)?;
        let ty = self.parse_type()?;
        self.expect_symbol(Symbol::Eq)?;
        let value = self.parse_expression(0)?;
        let end = self.expect_symbol(Symbol::Semicolon)?.end;
        Ok(Statement::Let {
            name,
            ty,
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_if(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword("if")?.start;
        let condition = self.parse_expression(0)?;
        let (then_statements, mut end) = self.parse_block()?;
        let else_statements = if self.match_keyword("else") {
            let (statements, else_end) = self.parse_block()?;
            end = else_end;
            statements
        } else {
            Vec::new()
        };
        Ok(Statement::If {
            condition,
            then_statements,
            else_statements,
            span: Span::new(start, end),
        })
    }

    fn parse_while(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword("while")?.start;
        if !self.match_keyword("budget") {
            return Err(self.error_current("P0007", "while requires literal budget"));
        }
        let budget_token = self.advance();
        let TokenKind::Integer(raw_budget) = budget_token.kind else {
            return Err(Diagnostic::new(
                "P0007",
                "while requires literal budget",
                budget_token.span,
            ));
        };
        let budget = raw_budget.parse::<u64>().map_err(|_| {
            Diagnostic::new("P0007", "while requires literal budget", budget_token.span)
        })?;
        if budget == 0 {
            return Err(Diagnostic::new(
                "P0008",
                "zero loop budget",
                budget_token.span,
            ));
        }

        let condition = self.parse_expression(0)?;
        let (statements, end) = self.parse_block()?;
        Ok(Statement::While {
            budget,
            condition,
            statements,
            span: Span::new(start, end),
        })
    }

    fn parse_type(&mut self) -> Result<TypeName, Diagnostic> {
        let ident = self.expect_ident()?;
        match ident.text.as_str() {
            "bool" => Ok(TypeName::Bool),
            "u64" => Ok(TypeName::U64),
            "i64" => Ok(TypeName::I64),
            "bytes" => Ok(TypeName::Bytes),
            "utf8" => Ok(TypeName::Utf8),
            "object_id" => Ok(TypeName::ObjectId),
            "revision_id" => Ok(TypeName::RevisionId),
            "task_id" => Ok(TypeName::TaskId),
            "proposal_id" => Ok(TypeName::ProposalId),
            "capability" => Ok(TypeName::Capability),
            "error_code" => Ok(TypeName::ErrorCode),
            "unit" => Ok(TypeName::Unit),
            _ => Err(Diagnostic::new(
                "P0009",
                "unknown type spelling",
                ident.span,
            )),
        }
    }

    fn parse_expression(&mut self, min_precedence: u8) -> Result<Expression, Diagnostic> {
        let mut left = self.parse_unary()?;

        while let Some((op, precedence)) = self.peek_binary_op() {
            if precedence < min_precedence {
                break;
            }
            self.advance();
            let right = self.parse_expression(precedence + 1)?;
            let span = Span::new(left.span().start, right.span().end);
            left = Expression::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expression, Diagnostic> {
        if self.at_symbol(Symbol::Bang) {
            let start = self.advance().span.start;
            let expr = self.parse_unary()?;
            let span = Span::new(start, expr.span().end);
            return Ok(Expression::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
                span,
            });
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expression, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Ident(text) if text == "true" || text == "false" => {
                Ok(Expression::Literal(Literal::Bool {
                    value: text == "true",
                    span: token.span,
                }))
            }
            TokenKind::Ident(text) => {
                let ident = Ident {
                    text,
                    span: token.span,
                };
                if self.match_symbol(Symbol::LParen) {
                    let mut args = Vec::new();
                    if !self.at_symbol(Symbol::RParen) {
                        loop {
                            args.push(self.parse_expression(0)?);
                            if !self.match_symbol(Symbol::Comma) {
                                break;
                            }
                        }
                    }
                    let end = self.expect_symbol(Symbol::RParen)?.end;
                    let span = Span::new(ident.span.start, end);
                    Ok(Expression::Call {
                        callee: ident,
                        args,
                        span,
                    })
                } else {
                    Ok(Expression::Name(ident))
                }
            }
            TokenKind::Integer(text) => Ok(Expression::Literal(Literal::Integer {
                text,
                span: token.span,
            })),
            TokenKind::String(value) => Ok(Expression::Literal(Literal::String {
                value,
                span: token.span,
            })),
            TokenKind::Symbol(Symbol::LParen) => {
                let expr = self.parse_expression(0)?;
                self.expect_symbol(Symbol::RParen)?;
                Ok(expr)
            }
            _ => Err(Diagnostic::new("P0003", "unexpected token", token.span)),
        }
    }

    fn peek_binary_op(&self) -> Option<(BinaryOp, u8)> {
        let symbol = match &self.peek().kind {
            TokenKind::Symbol(symbol) => *symbol,
            _ => return None,
        };
        match symbol {
            Symbol::OrOr => Some((BinaryOp::Or, 1)),
            Symbol::AndAnd => Some((BinaryOp::And, 2)),
            Symbol::EqEq => Some((BinaryOp::Equal, 3)),
            Symbol::Less => Some((BinaryOp::Less, 4)),
            Symbol::Plus => Some((BinaryOp::Add, 5)),
            Symbol::Minus => Some((BinaryOp::Subtract, 5)),
            _ => None,
        }
    }

    fn expect_ident(&mut self) -> Result<Ident, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Ident(text) => Ok(Ident {
                text,
                span: token.span,
            }),
            _ => Err(Diagnostic::new("P0003", "unexpected token", token.span)),
        }
    }

    fn expect_keyword(&mut self, expected: &'static str) -> Result<Span, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Ident(text) if text == expected => Ok(token.span),
            _ => Err(Diagnostic::new("P0003", "unexpected token", token.span)),
        }
    }

    fn expect_symbol(&mut self, expected: Symbol) -> Result<Span, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Symbol(symbol) if symbol == expected => Ok(token.span),
            _ => Err(Diagnostic::new("P0003", "unexpected token", token.span)),
        }
    }

    fn expect_eof(&mut self) -> Result<(), Diagnostic> {
        if self.is_eof() {
            Ok(())
        } else {
            Err(self.error_current("P0003", "unexpected token"))
        }
    }

    fn match_keyword(&mut self, expected: &'static str) -> bool {
        if self.at_keyword(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_symbol(&mut self, expected: Symbol) -> bool {
        if self.at_symbol(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_keyword(&self, expected: &'static str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(text) if text == expected)
    }

    fn at_symbol(&self, expected: Symbol) -> bool {
        matches!(self.peek().kind, TokenKind::Symbol(symbol) if symbol == expected)
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .expect("parser requires lexer to append eof token")
    }

    fn previous_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map_or_else(|| self.peek().span, |token| token.span)
    }

    fn error_current(&self, code: &'static str, message: &'static str) -> Diagnostic {
        Diagnostic::new(code, message, self.peek().span)
    }

    fn error_previous(&self, code: &'static str, message: &'static str) -> Diagnostic {
        Diagnostic::new(code, message, self.previous_span())
    }
}
