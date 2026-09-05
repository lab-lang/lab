//! Lab package manifests, source discovery, and module graphs.

mod graph;
mod manifest;
mod package;

pub use graph::{ImportResolution, ModuleGraph, ModuleGraphError, ModuleNode};
pub use manifest::{
    AdapterBinding, BuildMetadata, DependencyDetail, DependencySpec, ExecutionMetadata,
    InventoryMetadata, LabManifest, MethodCatalogMetadata, MethodPinMetadata, PackageManifest,
    PackageMetadata, PlanningAdapterRequirement, PlanningMetadata, WorkspaceManifest,
    WorkspaceMetadata,
};
pub use package::{
    DiscoveredRoot, LabPackage, LabWorkspace, PackageError, PackageSource, SbolSyntax,
    SourceLanguage, source_generator,
};

pub const MANIFEST_FILE: &str = "lab.toml";
