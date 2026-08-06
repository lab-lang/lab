use std::fs;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::{
    LabManifest, MANIFEST_FILE, ModuleGraph, ModuleGraphError, ModuleNode, PackageManifest,
    WorkspaceManifest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageSource {
    pub module: String,
    pub path: PathBuf,
    pub relative_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabPackage {
    pub root: PathBuf,
    pub manifest: PackageManifest,
    pub sources: Vec<PackageSource>,
}

/// A workspace root: membership only. It owns no source modules of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabWorkspace {
    pub root: PathBuf,
    pub manifest: WorkspaceManifest,
}

impl LabWorkspace {
    /// Absolute member directories in declaration order.
    pub fn member_roots(&self) -> Vec<PathBuf> {
        self.manifest
            .workspace
            .members
            .iter()
            .map(|member| self.root.join(member))
            .collect()
    }

    pub fn default_member_root(&self) -> PathBuf {
        self.root.join(self.manifest.default_member())
    }
}

/// What a `lab.toml` found by an upward search turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveredRoot {
    Package(LabPackage),
    Workspace(LabWorkspace),
}

impl DiscoveredRoot {
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, PackageError> {
        let root = find_manifest_directory(start.as_ref())?;
        match read_manifest(&root)? {
            LabManifest::Package(_) => Ok(Self::Package(LabPackage::load(root)?)),
            LabManifest::Workspace(manifest) => {
                Ok(Self::Workspace(LabWorkspace { root, manifest }))
            }
        }
    }

    pub fn root(&self) -> &Path {
        match self {
            Self::Package(package) => &package.root,
            Self::Workspace(workspace) => &workspace.root,
        }
    }
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("could not find {MANIFEST_FILE} from {0}")]
    ManifestNotFound(PathBuf),
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid package manifest {path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid package name '{0}'; use letters, digits, '-' or '_', beginning with a letter")]
    InvalidName(String),
    #[error("invalid package version '{version}': {source}")]
    InvalidVersion {
        version: String,
        #[source]
        source: semver::Error,
    },
    #[error("unsupported Lab edition '{0}'; this toolchain supports edition 2026")]
    UnsupportedEdition(String),
    #[error("invalid dependency '{name}': {message}")]
    InvalidDependency { name: String, message: String },
    #[error("package '{package}' has no Lab source modules under {source_root}")]
    NoSources {
        package: String,
        source_root: PathBuf,
    },
    #[error("package entry '{entry}' is not one of the discovered source modules")]
    MissingEntry { entry: PathBuf },
    #[error("failed to inspect package source directory {path}: {source}")]
    Walk {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("workspace declares no members")]
    EmptyWorkspace,
    #[error("invalid workspace member '{member}': {message}")]
    InvalidWorkspaceMember { member: PathBuf, message: String },
    #[error(
        "workspace has several members; declare 'default-member' to select the one a single-package command acts on"
    )]
    AmbiguousDefaultMember,
    #[error("{path} is a workspace root, not a package")]
    NotAPackage { path: PathBuf },
}

fn find_manifest_directory(start: &Path) -> Result<PathBuf, PackageError> {
    let mut directory = if start.is_file() {
        start
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if directory.join(MANIFEST_FILE).is_file() {
            return Ok(directory);
        }
        if !directory.pop() {
            return Err(PackageError::ManifestNotFound(start.to_path_buf()));
        }
    }
}

fn read_manifest(root: &Path) -> Result<LabManifest, PackageError> {
    let manifest_path = root.join(MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path).map_err(|source| PackageError::Read {
        path: manifest_path.clone(),
        source,
    })?;
    let manifest = LabManifest::parse(&text).map_err(|source| PackageError::Manifest {
        path: manifest_path,
        source,
    })?;
    manifest.validate()?;
    Ok(manifest)
}

impl LabPackage {
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, PackageError> {
        match DiscoveredRoot::discover(start)? {
            DiscoveredRoot::Package(package) => Ok(package),
            DiscoveredRoot::Workspace(workspace) => Err(PackageError::NotAPackage {
                path: workspace.root,
            }),
        }
    }

    pub fn load(root: impl AsRef<Path>) -> Result<Self, PackageError> {
        let root = root.as_ref().to_path_buf();
        let manifest = match read_manifest(&root)? {
            LabManifest::Package(manifest) => manifest,
            LabManifest::Workspace(_) => {
                return Err(PackageError::NotAPackage { path: root });
            }
        };

        let source_root = root.join("src");
        let mut files = Vec::new();
        collect_sources(&source_root, &mut files)?;
        files.sort();
        if files.is_empty() {
            return Err(PackageError::NoSources {
                package: manifest.package.name.clone(),
                source_root,
            });
        }

        let namespace = manifest.package.name.replace('-', "_");
        let sources = files
            .into_iter()
            .map(|path| {
                let relative_path = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();
                let module_path = path.strip_prefix(root.join("src")).unwrap_or(&path);
                PackageSource {
                    module: module_name(&namespace, module_path),
                    path,
                    relative_path,
                }
            })
            .collect::<Vec<_>>();

        if let Some(entry) = &manifest.build.entry {
            let normalized_entry = normalize_relative(entry);
            if !sources
                .iter()
                .any(|source| normalize_relative(&source.relative_path) == normalized_entry)
            {
                return Err(PackageError::MissingEntry {
                    entry: entry.clone(),
                });
            }
        }

        Ok(Self {
            root,
            manifest,
            sources,
        })
    }

    pub fn module_graph(&self) -> Result<ModuleGraph, ModuleGraphError> {
        ModuleGraph::new(
            &self.manifest,
            self.sources.iter().map(|source| ModuleNode {
                name: source.module.clone(),
                relative_path: source.relative_path.clone(),
            }),
        )
    }
}

fn collect_sources(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), PackageError> {
    let entries = fs::read_dir(directory).map_err(|source| PackageError::Walk {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| PackageError::Walk {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| PackageError::Walk {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            collect_sources(&path, files)?;
        } else if file_type.is_file()
            && path.extension().is_some_and(|extension| extension == "lab")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn module_name(namespace: &str, relative: &Path) -> String {
    let mut segments = vec![namespace.to_owned()];
    for component in relative.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        let mut segment = component.to_string_lossy().into_owned();
        if let Some(stem) = segment.strip_suffix(".lab") {
            segment = stem.to_owned();
        }
        segments.push(segment.replace('-', "_"));
    }
    segments.join(".")
}

fn normalize_relative(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|component| match component {
            Component::CurDir => None,
            Component::Normal(component) => Some(component),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::package::*;

    #[test]
    fn derives_namespaced_module_names() {
        assert_eq!(
            module_name("tet_reporter", Path::new("workflows/build-plasmid.lab")),
            "tet_reporter.workflows.build_plasmid"
        );
    }
}
