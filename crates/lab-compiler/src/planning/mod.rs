//! Target-neutral planning shared by compiler backends.

mod adapters;
mod allocation;
mod capability;
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
pub use allocation::{
    AllocatedAdapter, AllocationScalarValue, CandidateRejectionReason, EligibleCapabilityCandidate,
    FACILITY_ALLOCATION_SCHEMA_VERSION, FacilityAllocation, FacilityAllocationError,
    MatchedCapabilityParameter, RejectedCapabilityCandidate, RequirementAllocation,
};
pub use capability::{
    CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION, CAPABILITY_REQUIREMENTS_SCHEMA_VERSION,
    CapabilityInstantiationError, CapabilityKind, CapabilityMaterialInput,
    CapabilityMaterialOutput, CapabilityParameterConstraint, CapabilityRequirement,
    CapabilityRequirementError, CapabilityRequirementInstance, CapabilityRequirementInstances,
    CapabilityRequirementSource, CapabilityRequirements, CapabilityValueInput,
    CapabilityValueOutput, ParameterRelation, PropertyKind, RequirementControlMode,
    RequirementQualification, StatementBlock, StatementPathSegment, UnitIri, WorkflowCallSite,
    WorkflowIdentity,
};
pub use execution::{
    ExecutionPlanBuildError, ExecutionPlanOptions, PlannedMaterialMove, build_execution_plan,
    build_execution_plan_from_invocations,
};
pub use inventory::BuildInventoryError;
pub(crate) use invocation::hex_sha256;
pub use invocation::{
    ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationError,
    AdapterInvocationPlan, AdapterInvocationValidationError, AllocatedMethod,
    AllocatedProcedureTask, AllocatedRequirementBinding, InvocationAdapter,
};
pub use lowering::{
    FACILITY_LOWERING_SCHEMA_VERSION, FacilityLoweredArtifact, FacilityLoweredArtifactRole,
    FacilityLoweredRequirement, FacilityLoweringManifest, FacilityLoweringProjectionError,
    FacilityLoweringRoute, reviewed_lowering_bundles,
};
pub use model::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyInventorySource,
    DependencyNode, LegacyBuildInventory, MaterialLotBinding, MaterialLotBuildInventory,
};
pub use problem::{
    PLANNING_PROBLEM_SCHEMA_VERSION, PlanningCapabilityRequirement, PlanningMethodCandidate,
    PlanningMethodChoice, PlanningMethodYield, PlanningPort, PlanningProblem,
    PlanningProblemValidationError, PlanningProcedureParameter, PlanningProcedureTask,
    PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
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
