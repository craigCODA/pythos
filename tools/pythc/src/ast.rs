use crate::span::Span;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub name: Ident,
    pub principal_id: u64,
    pub imports: Vec<ImportDecl>,
    pub main: Function,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ident {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportDecl {
    pub name: Ident,
    pub resource: String,
    pub rights: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    Let {
        name: Ident,
        ty: TypeName,
        value: Expression,
        span: Span,
    },
    If {
        condition: Expression,
        then_statements: Vec<Statement>,
        else_statements: Vec<Statement>,
        span: Span,
    },
    While {
        budget: u64,
        condition: Expression,
        statements: Vec<Statement>,
        span: Span,
    },
    Expr(Expression),
    Return {
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeName {
    Bool,
    U64,
    I64,
    Bytes,
    Utf8,
    ObjectId,
    RevisionId,
    TaskId,
    ProposalId,
    Capability,
    ErrorCode,
    Unit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expression {
    Literal(Literal),
    Name(Ident),
    Call {
        callee: Ident,
        args: Vec<Expression>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expression>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
}

impl Expression {
    pub const fn span(&self) -> Span {
        match self {
            Self::Literal(literal) => literal.span(),
            Self::Name(name) => name.span,
            Self::Call { span, .. } | Self::Unary { span, .. } | Self::Binary { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Literal {
    Bool { value: bool, span: Span },
    Integer { text: String, span: Span },
    String { value: String, span: Span },
}

impl Literal {
    pub const fn span(&self) -> Span {
        match self {
            Self::Bool { span, .. } | Self::Integer { span, .. } | Self::String { span, .. } => {
                *span
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOp {
    Not,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOp {
    Equal,
    Less,
    Add,
    Subtract,
    And,
    Or,
}
