use thiserror::Error;

use super::SpecError;
use super::source::Span;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("{message} at bytes {}..{}", span.start, span.end)]
    Syntax { span: Span, message: String },
    #[error("the source is valid Lab syntax but is not supported by the current lowering: {feature} at bytes {}..{}", span.start, span.end)]
    Unsupported { span: Span, feature: String },
    #[error(transparent)]
    Specification(#[from] SpecError),
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
