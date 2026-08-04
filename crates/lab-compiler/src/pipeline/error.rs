use crate::{Capability, SpecError};
use thiserror::Error;

use crate::{PlanError, SessionError};

#[derive(Debug, Error)]
pub enum CompilerError {
    #[error(transparent)]
    InvalidSpecification(#[from] SpecError),
    #[error("laboratory profile is missing required capabilities: {0:?}")]
    MissingCapabilities(Vec<Capability>),
    #[error("laboratory profile has no supported DNA assembly method")]
    NoAssemblyMethod,
    #[error("laboratory profile must name a preferred propagation host")]
    MissingPreferredHost,
    #[error("the initial plasmid lowering supports exactly one requested copy, found {0}")]
    UnsupportedCopyCount(u32),
    #[error("the design is valid but unsupported by the selected compiler pipeline: {0}")]
    UnsupportedDesign(String),
    #[error("compiler IR failed verification: {0}")]
    InvalidIr(String),
    #[error(transparent)]
    InvalidPlan(#[from] PlanError),
    #[error("compiler generated an invalid identifier '{0}'")]
    InvalidIdentifier(String),
    #[error(transparent)]
    Session(#[from] SessionError),
}
