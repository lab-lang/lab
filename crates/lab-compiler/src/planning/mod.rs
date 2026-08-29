//! Target-neutral planning shared by compiler backends.

mod adapters;
mod execution;
mod inventory;
mod invocation;
mod lowering;
mod model;
mod problem;
mod resolution;
mod solver;

pub use adapters::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingError, AdapterBindingRequest,
    AdapterBindingSnapshot, BoundCapabilityOffering, BoundCapabilityParameter,
    BoundCapabilityParameterValue, ResolvedAdapterBinding,
};
pub use execution::{
    ExecutionPlanBuildError, ExecutionPlanOptions, build_execution_plan_from_invocations,
};
pub use inventory::BuildInventoryError;
pub(crate) use invocation::hex_sha256;
pub use invocation::{
    ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationError,
    AdapterInvocationPlan, AdapterInvocationValidationError, AllocatedMethod,
    AllocatedProcedureTask, AllocatedRequirementBinding, InvocationAdapter, adapter_invocation_id,
};
pub use lowering::{
    FACILITY_LOWERING_SCHEMA_VERSION, FacilityLoweredArtifact, FacilityLoweredArtifactRole,
    FacilityLoweredRequirement, FacilityLoweringManifest, FacilityLoweringProjectionError,
    FacilityLoweringRoute, FacilityLoweringScope, reviewed_lowering_bundles,
};
pub use model::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyInventorySource,
    DependencyNode, LegacyBuildInventory, MaterialLotBinding, MaterialLotBuildInventory,
};
pub use problem::{
    PLANNING_PROBLEM_SCHEMA_VERSION, PlanningCapabilityRequirement, PlanningMaterialInput,
    PlanningMaterialSource, PlanningMethodCandidate, PlanningMethodChoice, PlanningMethodYield,
    PlanningPort, PlanningProblem, PlanningProblemValidationError, PlanningProcedureParameter,
    PlanningProcedureTask, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
};
pub use resolution::{DependencyGraphError, resolve_dependency_graph};
pub use solver::{
    AdapterRequirement, AlternativeMethod, AlternativeRequirementBinding,
    FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION, FacilityPlanningError, FacilityPlanningPolicy,
    FacilityPlanningSolution, FacilityPlanningSolutionValidationError, MethodPin,
    MethodPinSelector, PlanningAlternative, PlanningCandidateRejectionReason,
    PlanningRejectedOffering, RejectedMethodCandidate, RejectedPlanningRequirement,
    SelectedAdapter, SelectedCapabilityParameter, SelectedMethod, SelectedProcedureTask,
    SelectedRequirementBinding,
};
