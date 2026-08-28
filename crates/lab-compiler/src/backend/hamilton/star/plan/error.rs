use thiserror::Error;

use crate::ArtifactError;
use crate::backend::AdapterConstraintError;
use crate::backend::error::PlanningError;
use crate::backend::hamilton::star::profile::StarProfileError;

#[derive(Debug, Error, PartialEq)]
pub enum StarPlanningError {
    #[error(transparent)]
    Constraint(Box<AdapterConstraintError>),
    #[error("invalid target-selected Protocol LAIR: {0}")]
    InvalidProtocol(String),
    #[error(transparent)]
    Profile(#[from] StarProfileError),
}

impl From<AdapterConstraintError> for StarPlanningError {
    fn from(error: AdapterConstraintError) -> Self {
        Self::Constraint(Box::new(error))
    }
}

impl From<PlanningError> for StarPlanningError {
    fn from(error: PlanningError) -> Self {
        match error {
            PlanningError::Constraint(constraint) => Self::Constraint(constraint),
            PlanningError::InvalidProtocol(message) => Self::InvalidProtocol(message),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum StarEmissionError {
    #[error("failed to serialize the generated automation plan: {0}")]
    Serialization(String),
    /// A plan that cleared validation can still ask the frame encoders for
    /// a value outside a firmware range; the command error carries the
    /// specific parameter and bound.
    #[error("failed to encode a STAR firmware frame: {0}")]
    Command(#[from] hamilton_star::CommandError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

#[derive(Debug, Error, PartialEq)]
pub enum StarBuildError {
    #[error(transparent)]
    Planning(#[from] StarPlanningError),
    #[error(transparent)]
    Emission(#[from] StarEmissionError),
}
