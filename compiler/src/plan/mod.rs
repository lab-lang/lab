//! Backend-neutral laboratory plans produced by the compiler.

mod model;
mod validation;

pub use crate::frontend::AcceptanceCriterion;
pub use model::{
    AcceptanceObligation, ExecutablePlan, OperationKind, PlanStep, PlanValue, ValueKind,
};
pub use validation::PlanError;
