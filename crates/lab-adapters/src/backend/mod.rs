//! Exact adapter invocation lowering and concrete device implementations.
//!
//! Facility planning selects Methods, capability offerings, Assets, material lots, and explicit adapter bindings before this layer runs. Each adapter receives only the immutable Procedure tasks and requirements assigned to one exact Asset invocation, validates its private operational profile, and emits independently reviewable device documents. Adapters never select scientific methods, traverse source programs, or infer drivers from manufacturer and model metadata.

mod adapters;
mod constraints;
mod document;
pub mod hamilton;
pub mod inheco;
mod invocation;
pub mod opentrons;
mod procedure;
mod profile;
mod resources;
pub mod run_sheet;
mod typst;

pub use adapters::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterDescriptor,
    AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterProfileContractError, AdapterRegistration, AdapterRegistry, AdapterServices,
    InvocationLowerer, ProcedureImplementationDescriptor, ProfileValidator, ProgramFeasibility,
    ValidatedAdapterProfile, adapter_catalog, builtin_adapter_registry, default_adapter_profile,
    validate_adapter_profile,
};
pub use constraints::AdapterConstraintError;
