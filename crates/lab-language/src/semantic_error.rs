use thiserror::Error;

use crate::ParseError;
use crate::material_flow::MaterialFlowError;
use crate::source::Span;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ModuleError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Semantic(#[from] SemanticError),
    #[error(transparent)]
    MaterialFlow(#[from] MaterialFlowError),
}

/// A secondary source range that explains part of an error.
///
/// An error that spans two places — a type parameter fixed by one operand and
/// contradicted by another — is only legible when both are shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedSpan {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message} at bytes {}..{}", span.start, span.end)]
pub struct SemanticError {
    pub span: Span,
    pub message: String,
    /// Other places that participate in this error, each with its own note.
    pub related: Vec<RelatedSpan>,
    /// Suggested ways forward. Help has no source range: it describes what the
    /// author could write instead, which does not exist in the file yet.
    pub help: Vec<String>,
}

impl SemanticError {
    pub(crate) fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            related: Vec::new(),
            help: Vec::new(),
        }
    }

    pub(crate) fn related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related.push(RelatedSpan {
            span,
            message: message.into(),
        });
        self
    }

    pub(crate) fn help(mut self, message: impl Into<String>) -> Self {
        self.help.push(message.into());
        self
    }
}
