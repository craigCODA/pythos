use crate::span::Span;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Hex(String),
    Integer(u64),
    String(String),
    Symbol(Symbol),
    Eof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Symbol {
    LBrace,
    RBrace,
    LParen,
    RParen,
    Colon,
    Semicolon,
    Comma,
    Less,
    Greater,
    Eq,
    EqEq,
    Arrow,
    Plus,
    Minus,
    AndAnd,
    OrOr,
    Bang,
}
