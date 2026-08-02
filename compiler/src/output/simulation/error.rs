use crate::PlanError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SimulationError {
    #[error("cannot simulate an invalid executable plan: {0}")]
    InvalidPlan(#[from] PlanError),
}
