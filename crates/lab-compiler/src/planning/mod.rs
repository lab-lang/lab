//! Target-neutral planning shared by compiler backends.

mod adapters;
mod capability;
mod inventory;
mod model;
mod resolution;

pub use adapters::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingError, AdapterBindingRequest,
    AdapterBindingSnapshot, BoundCapabilityOffering, ResolvedAdapterBinding,
};
pub use capability::{
    CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION, CAPABILITY_REQUIREMENTS_SCHEMA_VERSION,
    CapabilityInstantiationError, CapabilityMaterialInput, CapabilityMaterialOutput,
    CapabilityParameterConstraint, CapabilityRequirement, CapabilityRequirementError,
    CapabilityRequirementInstance, CapabilityRequirementInstances, CapabilityRequirementSource,
    CapabilityRequirements, CapabilityValueInput, CapabilityValueOutput, ParameterRelation,
    RequirementControlMode, RequirementQualification, StatementBlock, StatementPathSegment,
    WorkflowCallSite, WorkflowIdentity,
};
pub use inventory::BuildInventoryError;
pub use model::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyInventorySource,
    DependencyNode, LegacyBuildInventory, MaterialLotBinding, MaterialLotBuildInventory,
};
pub use resolution::{DependencyGraphError, resolve_dependency_graph};
