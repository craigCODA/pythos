#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: &'static str,
    pub span: Span,
}

impl Diagnostic {
    pub const fn new(code: &'static str, message: &'static str, span: Span) -> Self {
        Self {
            code,
            message,
            span,
        }
    }
}
