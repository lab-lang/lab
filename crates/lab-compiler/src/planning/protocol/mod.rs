//! Backend-neutral laboratory plans produced by the compiler.

mod acceptance;
mod model;
mod validation;

pub use acceptance::AcceptanceCriterion;
pub use model::{
    AcceptanceObligation, OperationKind, PlanStep, PlanValue, ProtocolPlan, ValueKind,
};
pub use validation::PlanError;
