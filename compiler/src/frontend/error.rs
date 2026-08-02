use thiserror::Error;

use super::SpecError;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("{message} at byte {offset}")]
    Syntax { offset: usize, message: String },
    #[error(transparent)]
    Specification(#[from] SpecError),
}

pub(crate) fn syntax(offset: usize, message: impl Into<String>) -> ParseError {
    ParseError::Syntax {
        offset,
        message: message.into(),
    }
}
