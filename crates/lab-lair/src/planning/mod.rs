//! Target-neutral planning shared by compiler backends.

mod extraction;
mod invocation;
mod lowering;
mod material_inventory;
mod problem;
mod schedule;
mod solution;

pub use crate::allocation::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedRequirementBinding, InvocationAdapter,
};
pub use extraction::PlanningProblemExtractionError;
pub(crate) use extraction::extract_planning_problem;
pub(crate) use invocation::hex_sha256;
pub use invocation::{
    ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationError,
    AdapterInvocationPlan, AdapterInvocationValidationError, adapter_invocation_id,
};
pub use lowering::{
    FACILITY_LOWERING_SCHEMA_VERSION, FacilityLoweredArtifact, FacilityLoweredArtifactRole,
    FacilityLoweredRequirement, FacilityLoweringManifest, FacilityLoweringRoute,
};
pub use material_inventory::{
    MaterialLotCandidates, MaterialLotInventory, MaterialLotInventoryValidationError,
};
pub use problem::{
    PLANNING_PROBLEM_SCHEMA_VERSION, PlanningCapabilityRequirement, PlanningMaterialInput,
    PlanningMaterialSource, PlanningMethodCandidate, PlanningMethodChoice, PlanningMethodYield,
    PlanningPort, PlanningProblem, PlanningProblemValidationError, PlanningProcedureParameter,
    PlanningProcedureTask, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
};
pub use schedule::{
    ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION, AllocatedExecutionGroup,
    AllocatedProcedureSchedule, AllocatedProcedureScheduleError, ScheduledPhysicalLocation,
    ScheduledValueRef,
};
pub use solution::{
    AdapterRequirement, AssetPin, AssetPinSelector, FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION,
    FacilityPlanningPolicy, FacilityPlanningSolution, FacilityPlanningSolutionValidationError,
    MethodPin, MethodPinSelector, PlanningCandidateRejectionReason, PlanningRejectedOffering,
    SelectedAdapter, SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
    SelectedMethod, SelectedProcedureTask, SelectedRequirementBinding,
};
