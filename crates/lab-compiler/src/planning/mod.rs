//! Target-neutral planning shared by compiler backends.

pub mod dependencies;

pub use dependencies::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyGraphError,
    DependencyNode, resolve_dependency_graph,
};
