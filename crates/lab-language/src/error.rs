use thiserror::Error;

use crate::source::Span;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("{message} at bytes {}..{}", span.start, span.end)]
    Syntax { span: Span, message: String },
}

pub(crate) fn syntax(offset: usize, message: impl Into<String>) -> ParseError {
    syntax_span(Span::at(offset), message)
}

pub(crate) fn syntax_span(span: Span, message: impl Into<String>) -> ParseError {
    ParseError::Syntax {
        span,
        message: message.into(),
    }
}
