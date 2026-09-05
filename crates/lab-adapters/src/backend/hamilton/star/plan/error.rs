use thiserror::Error;

use crate::ArtifactError;
use crate::backend::AdapterConstraintError;
use crate::backend::hamilton::star::liquid_classes::LiquidClassError;
use crate::backend::hamilton::star::profile::StarProfileError;

#[derive(Debug, Error, PartialEq)]
pub enum StarPlanningError {
    #[error(transparent)]
    Constraint(Box<AdapterConstraintError>),
    #[error(transparent)]
    Profile(#[from] StarProfileError),
    #[error(transparent)]
    LiquidClass(#[from] LiquidClassError),
}

impl From<AdapterConstraintError> for StarPlanningError {
    fn from(error: AdapterConstraintError) -> Self {
        Self::Constraint(Box::new(error))
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
