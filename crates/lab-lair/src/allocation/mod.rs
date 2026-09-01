//! Exact facility decisions applied to verifier-valid LAIR.

mod application;
mod extraction;
pub(crate) mod ir;
pub mod model;

pub use application::AllocationApplicationError;
pub(crate) use application::apply_facility_solution;
pub use extraction::{AllocatedProgramExtractionError, extract_allocated_program};
pub use model::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, AllocatedRequirementBinding,
    InvocationAdapter,
};
