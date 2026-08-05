//! Target-neutral planning shared by compiler backends and simulation.

pub mod dependencies;
pub mod protocol;

pub use dependencies::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyGraphError,
    DependencyNode, resolve_dependency_graph,
};
pub use protocol::{
    AcceptanceCriterion, AcceptanceObligation, OperationKind, PlanError, PlanStep, PlanValue,
    ProtocolPlan, ValueKind,
};
