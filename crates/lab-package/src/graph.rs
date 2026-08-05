use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DependencySpec, PackageManifest};

/// A deterministic, I/O-free view of the modules visible from one package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleGraph {
    pub package: String,
    pub modules: BTreeMap<String, ModuleNode>,
    pub dependencies: BTreeMap<String, DependencySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleNode {
    pub name: String,
    pub relative_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ImportResolution {
    StandardLibrary { module: String },
    Package { module: String },
    Dependency { dependency: String, module: String },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ModuleGraphError {
    #[error("module '{0}' occurs more than once in the package")]
    DuplicateModule(String),
    #[error("module '{0}' cannot be resolved from this package graph")]
    UnresolvedImport(String),
}

impl ModuleGraph {
    pub fn new(
        manifest: &PackageManifest,
        modules: impl IntoIterator<Item = ModuleNode>,
    ) -> Result<Self, ModuleGraphError> {
        let mut indexed = BTreeMap::new();
        for module in modules {
            let name = module.name.clone();
            if indexed.insert(name.clone(), module).is_some() {
                return Err(ModuleGraphError::DuplicateModule(name));
            }
        }
        Ok(Self {
            package: manifest.package.name.clone(),
            modules: indexed,
            dependencies: manifest.dependencies.clone(),
        })
    }

    pub fn resolve_import(&self, module: &str) -> Result<ImportResolution, ModuleGraphError> {
        if module == "std" || module.starts_with("std.") {
            return Ok(ImportResolution::StandardLibrary {
                module: module.to_owned(),
            });
        }
        if self.modules.contains_key(module) {
            return Ok(ImportResolution::Package {
                module: module.to_owned(),
            });
        }
        let namespace = module.split('.').next().unwrap_or_default();
        if let Some(dependency) = self
            .dependencies
            .keys()
            .find(|name| name.as_str() == namespace || name.replace('-', "_") == namespace)
        {
            return Ok(ImportResolution::Dependency {
                dependency: dependency.clone(),
                module: module.to_owned(),
            });
        }
        Err(ModuleGraphError::UnresolvedImport(module.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_standard_local_and_dependency_modules() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[dependencies]
shared-parts = "1"
"#,
        )
        .unwrap();
        let graph = ModuleGraph::new(
            &manifest,
            [ModuleNode {
                name: "tet_reporter.designs.parts".to_owned(),
                relative_path: "src/designs/parts.lab".into(),
            }],
        )
        .unwrap();

        assert!(matches!(
            graph.resolve_import("std.bio.parts"),
            Ok(ImportResolution::StandardLibrary { .. })
        ));
        assert!(matches!(
            graph.resolve_import("tet_reporter.designs.parts"),
            Ok(ImportResolution::Package { .. })
        ));
        assert!(matches!(
            graph.resolve_import("shared_parts.promoters"),
            Ok(ImportResolution::Dependency { .. })
        ));
    }
}
