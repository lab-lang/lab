//! Lab package manifests, source discovery, and module graphs.

mod graph;
mod manifest;
mod package;

pub use graph::{ImportResolution, ModuleGraph, ModuleGraphError, ModuleNode};
pub use manifest::{
    BuildMetadata, DependencyDetail, DependencySpec, InventoryMetadata, LabManifest,
    PackageManifest, PackageMetadata, WorkspaceManifest, WorkspaceMetadata,
};
pub use package::{
    DiscoveredRoot, LabPackage, LabWorkspace, PackageError, PackageSource, SbolSyntax,
    SourceLanguage,
};

pub const MANIFEST_FILE: &str = "lab.toml";
