//! Concrete adapter contracts and device artifact lowering for Lab.
//!
//! LAIR owns the selected laboratory program and durable allocation records. This crate projects
//! that verifier-valid aggregate into immutable adapter invocations and schedules, validates
//! concrete adapter profiles, and emits reviewable device artifacts. It does not choose Methods,
//! Assets, material lots, or adapter bindings.

mod artifact;
mod backend;
mod invocation;
mod schedule;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use backend::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterConstraintError,
    AdapterDescriptor, AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterProfileContractError, AdapterServices, ProcedureImplementationDescriptor,
    ValidatedAdapterProfile, adapter_catalog, default_adapter_profile,
    lower_adapter_invocation_with_adapter, validate_adapter_profile,
};
pub use backend::{hamilton, inheco, opentrons};
pub use invocation::{
    ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationError,
    AdapterInvocationPlan, AdapterInvocationValidationError, adapter_invocation_id,
};
pub use schedule::{
    ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION, AllocatedExecutionGroup,
    AllocatedProcedureSchedule, AllocatedProcedureScheduleError, ScheduledPhysicalLocation,
    ScheduledValueRef,
};
