//! Planning failures raised by robot-neutral analysis.

use thiserror::Error;

use crate::backend::AdapterConstraintError;

/// A failure from robot-neutral planning. Each backend converts this into its
/// own planning error, so the rendered message keeps that backend's identity
/// and framing while the analysis itself stays common.
#[derive(Debug, Error, PartialEq, Eq)]
pub(in crate::backend) enum PlanningError {
    #[error(transparent)]
    Constraint(Box<AdapterConstraintError>),
    #[error("{0}")]
    InvalidProtocol(String),
}

impl From<AdapterConstraintError> for PlanningError {
    fn from(error: AdapterConstraintError) -> Self {
        Self::Constraint(Box::new(error))
    }
}
