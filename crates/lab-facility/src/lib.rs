//! Facility evidence, allocation, and reviewed-plan construction for Lab.
//!
//! LAIR owns the planning problem and durable allocation contracts. This crate combines those
//! contracts with an exact facility inventory and configured adapters, solves the resulting
//! constraint problem, and projects allocated invocations into a reviewed execution plan.

mod adapters;
mod execution;
mod explain;
mod inventory;
mod solver;

pub use adapters::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingError, AdapterBindingRequest,
    AdapterBindingSnapshot, BoundCapabilityOffering, BoundCapabilityParameter,
    BoundCapabilityParameterValue, BoundProcedureImplementation, ResolvedAdapterBinding,
};
pub use execution::{
    ExecutionPlanBuildError, ExecutionPlanOptions, build_execution_plan_from_invocations,
};
pub use explain::explain_facility_planning_error;
pub use inventory::{
    AllocatedMaterialInventoryValidationError, MaterialLotCandidates, MaterialLotInventory,
    MaterialLotInventoryError, MaterialLotInventoryValidationError, build_material_lot_inventory,
    validate_allocated_material_inventory,
};
pub use solver::{
    AlternativeMaterialBinding, AlternativeMethod, AlternativeRequirementBinding,
    FacilityPlanningError, PlanningAlternative, PlanningMaterialRejectionReason,
    RejectedMethodCandidate, RejectedPlanningMaterial, RejectedPlanningRequirement,
    solve_facility_planning,
};
