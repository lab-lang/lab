use thiserror::Error;

use crate::ArtifactError;
use crate::backend::TargetConstraintError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Ot2PlanningError {
    #[error(transparent)]
    Constraint(Box<TargetConstraintError>),
    #[error("invalid target-selected Protocol LAIR: {0}")]
    InvalidProtocol(String),
}

impl From<TargetConstraintError> for Ot2PlanningError {
    fn from(error: TargetConstraintError) -> Self {
        Self::Constraint(Box::new(error))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Ot2EmissionError {
    #[error("failed to serialize the generated automation plan: {0}")]
    Serialization(String),
    #[error("invalid OT-2 Python template '{template}': {message}")]
    Template {
        template: &'static str,
        message: String,
    },
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Ot2BuildError {
    #[error(transparent)]
    Planning(#[from] Ot2PlanningError),
    #[error(transparent)]
    Emission(#[from] Ot2EmissionError),
}
