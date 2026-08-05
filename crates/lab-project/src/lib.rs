//! Package-aware compilation host for Lab projects.
//!
//! This crate owns filesystem resolution and compilation order. The language
//! crate remains I/O-free and accepts only explicit semantic environments.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lab_language::{
    CheckedModule, ModuleId, SemanticEnvironment, compile_module_in_environment, parse_module,
};
use lab_package::{DependencySpec, LabPackage, PackageError, PackageSource};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LOCK_FILE: &str = "lab.lock";
pub const LOCK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("failed to canonicalize package path {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read source module {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "dependency '{dependency}' of package '{package}' is not a path dependency; registry resolution is intentionally unavailable"
    )]
    UnsupportedDependency { package: String, dependency: String },
    #[error("path dependency cycle: {0}")]
    DependencyCycle(String),
    #[error(
        "dependency '{dependency}' requires {requirement}, but path package '{package}' has version {actual}"
    )]
    VersionMismatch {
        dependency: String,
        requirement: String,
        package: String,
        actual: String,
    },
    #[error("failed to parse module '{module}': {message}")]
    Parse { module: String, message: String },
    #[error("failed to compile module '{module}': {message}")]
    Compile { module: String, message: String },
    #[error("module import cycle among {0}")]
    ModuleCycle(String),
}

#[derive(Clone, Debug)]
struct ResolvedPackage {
    package: LabPackage,
    dependencies: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug)]
pub struct LabProject {
    root: PathBuf,
    packages: BTreeMap<PathBuf, ResolvedPackage>,
    order: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct CompiledProject {
    pub root_package: String,
    pub modules: Vec<CompiledModule>,
    pub lock: ProjectLock,
}

#[derive(Clone, Debug)]
pub struct CompiledModule {
    pub package: String,
    pub source: PackageSource,
    pub module: CheckedModule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLock {
    pub schema_version: u32,
    pub root: String,
    pub packages: Vec<LockedPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub source: LockedSource,
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockedSource {
    Root,
    Path { path: PathBuf },
}

impl ProjectLock {
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

impl LabProject {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let root_package = LabPackage::discover(path)?;
        let root = canonicalize(&root_package.root)?;
        let mut project = Self {
            root: root.clone(),
            packages: BTreeMap::new(),
            order: Vec::new(),
        };
        let mut visiting = Vec::new();
        project.load_recursive(root, &mut visiting)?;
        Ok(project)
    }

    pub fn root_package(&self) -> &LabPackage {
        &self.packages[&self.root].package
    }

    pub fn compile(&self) -> Result<CompiledProject, ProjectError> {
        let mut compiled_by_package = BTreeMap::<PathBuf, Vec<CompiledModule>>::new();
        for root in &self.order {
            let resolved = &self.packages[root];
            let mut environment = SemanticEnvironment::default();
            for (alias, dependency_root) in &resolved.dependencies {
                let dependency = &self.packages[dependency_root].package;
                let namespace = dependency.manifest.package.name.replace('-', "_");
                for module in &compiled_by_package[dependency_root] {
                    let suffix = module
                        .source
                        .module
                        .strip_prefix(&namespace)
                        .unwrap_or(&module.source.module);
                    environment.insert(
                        format!("{}{}", alias.replace('-', "_"), suffix),
                        module.module.interface.clone(),
                    );
                }
            }
            let compiled = compile_package(&resolved.package, environment)?;
            compiled_by_package.insert(root.clone(), compiled);
        }
        let modules = self
            .order
            .iter()
            .flat_map(|root| compiled_by_package[root].clone())
            .collect();
        Ok(CompiledProject {
            root_package: self.root_package().manifest.package.name.clone(),
            modules,
            lock: self.lock(),
        })
    }

    pub fn lock(&self) -> ProjectLock {
        ProjectLock {
            schema_version: LOCK_SCHEMA_VERSION,
            root: self.root_package().manifest.package.name.clone(),
            packages: self
                .order
                .iter()
                .map(|root| {
                    let resolved = &self.packages[root];
                    LockedPackage {
                        name: resolved.package.manifest.package.name.clone(),
                        version: resolved.package.manifest.package.version.clone(),
                        source: if *root == self.root {
                            LockedSource::Root
                        } else {
                            LockedSource::Path {
                                path: relative_path(&self.root, root),
                            }
                        },
                        dependencies: resolved
                            .dependencies
                            .iter()
                            .map(|(alias, root)| {
                                (
                                    alias.clone(),
                                    self.packages[root].package.manifest.package.name.clone(),
                                )
                            })
                            .collect(),
                    }
                })
                .collect(),
        }
    }

    fn load_recursive(
        &mut self,
        root: PathBuf,
        visiting: &mut Vec<PathBuf>,
    ) -> Result<(), ProjectError> {
        if self.packages.contains_key(&root) {
            return Ok(());
        }
        if let Some(index) = visiting.iter().position(|path| path == &root) {
            let cycle = visiting[index..]
                .iter()
                .chain(std::iter::once(&root))
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(ProjectError::DependencyCycle(cycle));
        }
        visiting.push(root.clone());
        let package = LabPackage::load(&root)?;
        let mut dependencies = BTreeMap::new();
        for (alias, spec) in &package.manifest.dependencies {
            let (path, requirement) = match spec {
                DependencySpec::Detailed(detail) if detail.path.is_some() => (
                    detail.path.as_ref().expect("checked above"),
                    detail.version.as_deref(),
                ),
                _ => {
                    return Err(ProjectError::UnsupportedDependency {
                        package: package.manifest.package.name.clone(),
                        dependency: alias.clone(),
                    });
                }
            };
            let dependency_root = canonicalize(&package.root.join(path))?;
            self.load_recursive(dependency_root.clone(), visiting)?;
            let dependency = &self.packages[&dependency_root].package;
            if let Some(requirement) = requirement {
                let required = VersionReq::parse(requirement).expect("manifest validation ran");
                let actual = Version::parse(&dependency.manifest.package.version)
                    .expect("dependency manifest validation ran");
                if !required.matches(&actual) {
                    return Err(ProjectError::VersionMismatch {
                        dependency: alias.clone(),
                        requirement: requirement.to_owned(),
                        package: dependency.manifest.package.name.clone(),
                        actual: actual.to_string(),
                    });
                }
            }
            dependencies.insert(alias.clone(), dependency_root);
        }
        visiting.pop();
        self.order.push(root.clone());
        self.packages.insert(
            root,
            ResolvedPackage {
                package,
                dependencies,
            },
        );
        Ok(())
    }
}

fn compile_package(
    package: &LabPackage,
    mut environment: SemanticEnvironment,
) -> Result<Vec<CompiledModule>, ProjectError> {
    let local_names = package
        .sources
        .iter()
        .map(|source| source.module.clone())
        .collect::<BTreeSet<_>>();
    let mut remaining = BTreeMap::new();
    for source in &package.sources {
        let text = fs::read_to_string(&source.path).map_err(|source_error| ProjectError::Read {
            path: source.path.clone(),
            source: source_error,
        })?;
        let syntax = parse_module(&text).map_err(|error| ProjectError::Parse {
            module: source.module.clone(),
            message: error.to_string(),
        })?;
        let local_imports = syntax
            .items
            .iter()
            .filter_map(|item| {
                let lab_language::ast::Item::Use(import) = item else {
                    return None;
                };
                let path = import
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                local_names.contains(&path).then_some(path)
            })
            .collect::<BTreeSet<_>>();
        remaining.insert(source.module.clone(), (source.clone(), text, local_imports));
    }

    let mut compiled_names = BTreeSet::new();
    let mut result = Vec::new();
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|(_, (_, _, imports))| imports.is_subset(&compiled_names))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err(ProjectError::ModuleCycle(
                remaining.keys().cloned().collect::<Vec<_>>().join(", "),
            ));
        }
        for name in ready {
            let (source, text, _) = remaining.remove(&name).expect("ready module exists");
            let module =
                compile_module_in_environment(ModuleId::new(name.clone()), &text, &environment)
                    .map_err(|error| ProjectError::Compile {
                        module: name.clone(),
                        message: error.to_string(),
                    })?;
            environment.insert(name.clone(), module.interface.clone());
            compiled_names.insert(name);
            result.push(CompiledModule {
                package: package.manifest.package.name.clone(),
                source,
                module,
            });
        }
    }
    Ok(result)
}

fn canonicalize(path: &Path) -> Result<PathBuf, ProjectError> {
    path.canonicalize()
        .map_err(|source| ProjectError::Canonicalize {
            path: path.to_path_buf(),
            source,
        })
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = from.components().collect::<Vec<_>>();
    let to = to.components().collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    std::iter::repeat_n("..", from.len() - common)
        .map(PathBuf::from)
        .chain(
            to[common..]
                .iter()
                .map(|component| PathBuf::from(component.as_os_str())),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct TestProject(PathBuf);

    impl TestProject {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "lab-project-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn package(&self, relative: &str, manifest: &str, sources: &[(&str, &str)]) -> PathBuf {
            let root = self.0.join(relative);
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("lab.toml"), manifest).unwrap();
            for (path, source) in sources {
                let path = root.join("src").join(path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, source).unwrap();
            }
            root
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const DONOR: &str = r#"plasmid donor:
  sequence: dna("ACGT")
  require topology == circular
  accept sequence == design.sequence
"#;

    #[test]
    fn compiles_same_package_imports_in_dependency_order() {
        let fixture = TestProject::new();
        let root = fixture.package(
            "app",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
            &[
                ("parts.lab", DONOR),
                (
                    "main.lab",
                    r#"use app.parts

plasmid derived:
  sequence: donor.sequence
  require topology == circular
  accept sequence == design.sequence
"#,
                ),
            ],
        );
        let compiled = LabProject::discover(root).unwrap().compile().unwrap();
        assert_eq!(compiled.modules.len(), 2);
        assert_eq!(compiled.modules[0].source.module, "app.parts");
        assert_eq!(compiled.modules[1].source.module, "app.main");
        assert_eq!(compiled.modules[1].module.imports[0].provider, "package");
    }

    #[test]
    fn compiles_and_locks_recursive_path_dependencies() {
        let fixture = TestProject::new();
        fixture.package(
            "shared",
            "[package]\nname = \"shared\"\nversion = \"1.2.0\"\n",
            &[("values.lab", DONOR)],
        );
        let root = fixture.package(
            "app",
            r#"[package]
name = "app"
version = "0.1.0"

[dependencies]
shared = { path = "../shared", version = "^1.0" }
"#,
            &[(
                "main.lab",
                r#"use shared.values

plasmid derived:
  sequence: donor.sequence
  require topology == circular
  accept sequence == design.sequence
"#,
            )],
        );
        let compiled = LabProject::discover(root).unwrap().compile().unwrap();
        assert_eq!(compiled.modules.len(), 2);
        assert_eq!(compiled.lock.packages[0].name, "shared");
        assert_eq!(compiled.lock.packages[1].dependencies["shared"], "shared");
        let lock = compiled.lock.to_toml().unwrap();
        assert!(lock.contains("schema_version = 1"));
    }

    #[test]
    fn rejects_registry_dependencies_explicitly() {
        let fixture = TestProject::new();
        let root = fixture.package(
            "app",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nshared = \"1\"\n",
            &[("main.lab", DONOR)],
        );
        assert!(matches!(
            LabProject::discover(root),
            Err(ProjectError::UnsupportedDependency { dependency, .. }) if dependency == "shared"
        ));
    }
}
