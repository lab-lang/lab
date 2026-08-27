//! Target-neutral planning shared by compiler backends.

mod inventory;
mod model;
mod resolution;

pub use inventory::BuildInventoryError;
pub use model::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildGraphNode, BuildInventory,
    DependencyBuildManifest, DependencyBuildStatus, DependencyEdge, DependencyInventorySource,
    DependencyNode, LegacyBuildInventory, MaterialLotBinding, MaterialLotBuildInventory,
};
pub use resolution::{DependencyGraphError, resolve_dependency_graph};
