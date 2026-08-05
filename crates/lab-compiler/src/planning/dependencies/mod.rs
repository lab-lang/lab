//! Artifact dependency models and target-neutral resolution.

mod model;
mod resolution;

pub use model::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyNode,
};
pub use resolution::{DependencyGraphError, resolve_dependency_graph};
