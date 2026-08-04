use thiserror::Error;

use super::ParseError;
use super::material_flow::MaterialFlowError;
use super::source::Span;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ModuleError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Semantic(#[from] SemanticError),
    #[error(transparent)]
    MaterialFlow(#[from] MaterialFlowError),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message} at bytes {}..{}", span.start, span.end)]
pub struct SemanticError {
    pub span: Span,
    pub message: String,
}

impl SemanticError {
    pub(crate) fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}
