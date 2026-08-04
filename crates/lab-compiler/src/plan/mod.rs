//! Backend-neutral laboratory plans produced by the compiler.

mod model;
mod validation;

pub use lab_language::AcceptanceCriterion;
pub use model::{
    AcceptanceObligation, ExecutablePlan, OperationKind, PlanStep, PlanValue, ValueKind,
};
pub use validation::PlanError;
