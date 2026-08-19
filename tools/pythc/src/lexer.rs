use crate::{
    span::{Diagnostic, Span},
    token::{Symbol, Token, TokenKind},
};

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let lexer = Lexer {
        source,
        bytes: source.as_bytes(),
        pos: 0,
        tokens: Vec::new(),
    };
    lexer.run()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\r' | b'\n' | b'\t' => {
                    self.pos += 1;
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident(),
                b'0' if self
                    .peek_next()
                    .is_some_and(|next| matches!(next, b'x' | b'X')) =>
                {
                    self.lex_hex()
                }
                b'0'..=b'9' => self.lex_integer()?,
                b'"' => self.lex_string()?,
                b'{' => self.push_symbol(Symbol::LBrace, 1),
                b'}' => self.push_symbol(Symbol::RBrace, 1),
                b'(' => self.push_symbol(Symbol::LParen, 1),
                b')' => self.push_symbol(Symbol::RParen, 1),
                b':' => self.push_symbol(Symbol::Colon, 1),
                b';' => self.push_symbol(Symbol::Semicolon, 1),
                b',' => self.push_symbol(Symbol::Comma, 1),
                b'<' => self.push_symbol(Symbol::Less, 1),
                b'>' => self.push_symbol(Symbol::Greater, 1),
                b'+' => self.push_symbol(Symbol::Plus, 1),
                b'!' => self.push_symbol(Symbol::Bang, 1),
                b'=' if self.peek_next() == Some(b'=') => self.push_symbol(Symbol::EqEq, 2),
                b'=' => self.push_symbol(Symbol::Eq, 1),
                b'-' if self.peek_next() == Some(b'>') => self.push_symbol(Symbol::Arrow, 2),
                b'-' => self.push_symbol(Symbol::Minus, 1),
                b'&' if self.peek_next() == Some(b'&') => self.push_symbol(Symbol::AndAnd, 2),
                b'|' if self.peek_next() == Some(b'|') => self.push_symbol(Symbol::OrOr, 2),
                _ => {
                    let span = Span::new(self.pos, self.pos + 1);
                    return Err(Diagnostic::new("P0001", "unexpected character", span));
                }
            }
        }

        self.tokens
            .push(Token::new(TokenKind::Eof, Span::new(self.pos, self.pos)));
        Ok(self.tokens)
    }

    fn lex_ident(&mut self) {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        {
            self.pos += 1;
        }
        let text = self.source[start..self.pos].to_string();
        self.tokens.push(Token::new(
            TokenKind::Ident(text),
            Span::new(start, self.pos),
        ));
    }

    fn lex_hex(&mut self) {
        let start = self.pos;
        self.pos += 2;
        while self.peek().is_some_and(|byte| byte.is_ascii_hexdigit()) {
            self.pos += 1;
        }
        let text = self.source[start..self.pos].to_string();
        self.tokens
            .push(Token::new(TokenKind::Hex(text), Span::new(start, self.pos)));
    }

    fn lex_integer(&mut self) -> Result<(), Diagnostic> {
        let start = self.pos;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.pos += 1;
        }
        let raw = &self.source[start..self.pos];
        let value = raw.parse::<u64>().map_err(|_| {
            Diagnostic::new("P0003", "unexpected token", Span::new(start, self.pos))
        })?;
        self.tokens.push(Token::new(
            TokenKind::Integer(value),
            Span::new(start, self.pos),
        ));
        Ok(())
    }

    fn lex_string(&mut self) -> Result<(), Diagnostic> {
        let start = self.pos;
        self.pos += 1;
        let mut value = String::new();
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.pos += 1;
                    self.tokens.push(Token::new(
                        TokenKind::String(value),
                        Span::new(start, self.pos),
                    ));
                    return Ok(());
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(escaped) = self.peek() else {
                        return Err(Diagnostic::new(
                            "P0002",
                            "unterminated string",
                            Span::new(start, self.pos),
                        ));
                    };
                    self.pos += 1;
                    value.push(match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        _ => escaped as char,
                    });
                }
                _ => {
                    self.pos += 1;
                    value.push(byte as char);
                }
            }
        }

        Err(Diagnostic::new(
            "P0002",
            "unterminated string",
            Span::new(start, self.pos),
        ))
    }

    fn push_symbol(&mut self, symbol: Symbol, len: usize) {
        let start = self.pos;
        self.pos += len;
        self.tokens.push(Token::new(
            TokenKind::Symbol(symbol),
            Span::new(start, self.pos),
        ));
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }
}
