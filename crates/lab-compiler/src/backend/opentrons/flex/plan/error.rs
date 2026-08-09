use thiserror::Error;

use crate::ArtifactError;
use crate::backend::TargetConstraintError;
use crate::backend::error::PlanningError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FlexPlanningError {
    #[error(transparent)]
    Constraint(Box<TargetConstraintError>),
    #[error("invalid target-selected Protocol LAIR: {0}")]
    InvalidProtocol(String),
}

impl From<TargetConstraintError> for FlexPlanningError {
    fn from(error: TargetConstraintError) -> Self {
        Self::Constraint(Box::new(error))
    }
}

impl From<PlanningError> for FlexPlanningError {
    fn from(error: PlanningError) -> Self {
        match error {
            PlanningError::Constraint(constraint) => Self::Constraint(constraint),
            PlanningError::InvalidProtocol(message) => Self::InvalidProtocol(message),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum FlexEmissionError {
    #[error("failed to serialize the generated automation plan: {0}")]
    Serialization(String),
    /// A profile that clears planning can still name labware or wells the
    /// protocol authoring layer rejects; the authoring error carries the
    /// specific rule.
    #[error("failed to author the Flex JSON protocol: {0}")]
    Protocol(#[from] lab_opentrons_protocol::ProtocolError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

#[derive(Debug, Error, PartialEq)]
pub enum FlexBuildError {
    #[error(transparent)]
    Planning(#[from] FlexPlanningError),
    #[error(transparent)]
    Emission(#[from] FlexEmissionError),
}
