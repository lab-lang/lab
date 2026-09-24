//! Exact facility decisions applied to verifier-valid LAIR.

mod application;
mod extraction;
pub(crate) mod ir;
mod model;
mod provision;
pub use provision::{
    MaterialProvision, ProvisionDemand, bind_stock, provision_demands, provisioned_programs,
};
mod validation;

pub use application::AllocationApplicationError;
pub(crate) use application::apply_facility_solution;
pub use extraction::{AllocatedProgramExtractionError, extract_allocated_program};
pub use model::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, AllocatedRequirementBinding,
    InvocationAdapter,
};
pub use validation::AllocatedProgramValidationError;
