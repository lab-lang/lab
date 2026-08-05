//! Lab package manifests, source discovery, and module graphs.

mod graph;
mod manifest;
mod package;

pub use graph::{ImportResolution, ModuleGraph, ModuleGraphError, ModuleNode};
pub use manifest::{
    BuildMetadata, DependencyDetail, DependencySpec, PackageManifest, PackageMetadata,
};
pub use package::{LabPackage, PackageError, PackageSource};

pub const MANIFEST_FILE: &str = "lab.toml";
