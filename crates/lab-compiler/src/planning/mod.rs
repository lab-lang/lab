//! Target-neutral planning shared by compiler backends.

mod capability;
mod inventory;
mod model;
mod resolution;

pub use capability::{
    CAPABILITY_REQUIREMENTS_SCHEMA_VERSION, CapabilityMaterialInput, CapabilityMaterialOutput,
    CapabilityParameterConstraint, CapabilityRequirement, CapabilityRequirementError,
    CapabilityRequirementSource, CapabilityRequirements, CapabilityValueInput,
    CapabilityValueOutput, ParameterRelation, RequirementControlMode, RequirementQualification,
    StatementBlock, StatementPathSegment,
};
pub use inventory::BuildInventoryError;
pub use model::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyInventorySource,
    DependencyNode, LegacyBuildInventory, MaterialLotBinding, MaterialLotBuildInventory,
};
pub use resolution::{DependencyGraphError, resolve_dependency_graph};
