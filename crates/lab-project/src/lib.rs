//! Package-aware compilation host for Lab projects.
//!
//! This crate owns filesystem resolution and compilation order. The language
//! crate remains I/O-free and accepts only explicit semantic environments.

mod facility;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lab_compiler::method::{
    MethodCatalogDocument, MethodCatalogError, MethodDefinition, MethodRegistry,
    MethodRegistryError,
};
use lab_language::Grounding;
use lab_language::{
    CheckedDeclaration, CheckedModule, ModuleId, SemanticEnvironment,
    compile_module_in_environment, parse_module,
};
use lab_package::{
    DependencySpec, DiscoveredRoot, LabPackage, LabWorkspace, PackageError, PackageSource,
    SbolSyntax, SourceLanguage,
};
use lab_sbol::KindIndex;
use sbol3::{Document, RdfFormat};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use facility::{
    FacilityPlanningResult, FacilityProjectError, load_package_inventory, plan_modules_for_package,
    resolve_package_adapter_bindings,
};

pub const LOCK_FILE: &str = "lab.lock";
pub const LOCK_SCHEMA_VERSION: u32 = 2;

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
        "failed to resolve Method catalog '{document}' in package '{package}' at {path}: {source}"
    )]
    ResolveMethodCatalog {
        package: String,
        document: PathBuf,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Method catalog '{document}' resolves outside package '{package}'")]
    MethodCatalogOutsidePackage { package: String, document: PathBuf },
    #[error("failed to read Method catalog {path} in package '{package}': {source}")]
    ReadMethodCatalog {
        package: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse Method catalog {path} in package '{package}': {source}")]
    ParseMethodCatalog {
        package: String,
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid Method catalog {path} in package '{package}': {source}")]
    InvalidMethodCatalog {
        package: String,
        path: PathBuf,
        #[source]
        source: Box<MethodCatalogError>,
    },
    #[error(
        "portable Method definitions reachable from package '{package}' do not form one registry: {source}"
    )]
    InvalidMethodRegistry {
        package: String,
        #[source]
        source: Box<MethodRegistryError>,
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
    #[error(
        "entry module '{module}' of package '{package}' declares no 'workflow main'; an entry point must state the work it runs"
    )]
    MissingMainWorkflow { package: String, module: String },
}

#[derive(Clone, Debug)]
struct ResolvedPackage {
    package: LabPackage,
    dependencies: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug)]
pub struct LabProject {
    root: PathBuf,
    /// Package roots the project itself owns: one for a package project, the
    /// declared members for a workspace.
    members: Vec<PathBuf>,
    default_member: PathBuf,
    packages: BTreeMap<PathBuf, ResolvedPackage>,
    order: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct CompiledProject {
    /// Member package names in declaration order.
    pub members: Vec<String>,
    pub modules: Vec<CompiledModule>,
    /// The standard and package-contributed Methods available to the default runnable package.
    pub methods: MethodRegistry,
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
    pub members: Vec<String>,
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
    /// The project root, when the root is itself a package.
    Root,
    /// A member of a workspace root.
    Member { path: PathBuf },
    /// A package reached through a path dependency.
    Path { path: PathBuf },
}

impl ProjectLock {
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

impl LabProject {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let (root, members, default_member) = match DiscoveredRoot::discover(path)? {
            DiscoveredRoot::Package(package) => {
                let root = canonicalize(&package.root)?;
                (root.clone(), vec![root.clone()], root)
            }
            DiscoveredRoot::Workspace(workspace) => Self::resolve_workspace(&workspace)?,
        };
        let mut project = Self {
            root,
            members: members.clone(),
            default_member,
            packages: BTreeMap::new(),
            order: Vec::new(),
        };
        let mut visiting = Vec::new();
        for member in members {
            project.load_recursive(member, &mut visiting)?;
        }
        Ok(project)
    }

    fn resolve_workspace(
        workspace: &LabWorkspace,
    ) -> Result<(PathBuf, Vec<PathBuf>, PathBuf), ProjectError> {
        let root = canonicalize(&workspace.root)?;
        let members = workspace
            .member_roots()
            .iter()
            .map(|member| canonicalize(member))
            .collect::<Result<Vec<_>, _>>()?;
        let default_member = canonicalize(&workspace.default_member_root())?;
        Ok((root, members, default_member))
    }

    /// The directory holding the project's own `lab.toml`: a package root, or
    /// a workspace root. Generated artifacts and the lockfile live here.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The package a command that acts on exactly one package operates on.
    pub fn default_package(&self) -> &LabPackage {
        &self.packages[&self.default_member].package
    }

    /// Every package the project itself owns, in declaration order.
    pub fn member_packages(&self) -> Vec<&LabPackage> {
        self.members
            .iter()
            .map(|member| &self.packages[member].package)
            .collect()
    }

    /// The packages that make up one runnable program: the default member and
    /// everything it depends on, in dependency-first compilation order. A
    /// package build lowers exactly these packages' modules together, so an
    /// artifact declared in a dependency reaches planning and adapter lowering.
    pub fn program_packages(&self) -> Vec<String> {
        self.ordered_reachable_roots(&self.default_member)
            .into_iter()
            .map(|root| self.packages[root].package.manifest.package.name.clone())
            .collect()
    }

    fn ordered_reachable_roots(&self, root: &PathBuf) -> Vec<&PathBuf> {
        let mut reachable = BTreeSet::new();
        self.collect_reachable(root, &mut reachable);
        self.order
            .iter()
            .filter(|candidate| reachable.contains(*candidate))
            .collect()
    }

    fn collect_reachable(&self, root: &PathBuf, reachable: &mut BTreeSet<PathBuf>) {
        if !reachable.insert(root.clone()) {
            return;
        }
        for dependency in self.packages[root].dependencies.values() {
            self.collect_reachable(dependency, reachable);
        }
    }

    pub fn compile(&self) -> Result<CompiledProject, ProjectError> {
        let methods = self.validate_method_registries()?;
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
            members: self
                .member_packages()
                .iter()
                .map(|package| package.manifest.package.name.clone())
                .collect(),
            modules,
            methods,
            lock: self.lock(),
        })
    }

    /// Load package-contributed Methods reachable from the default runnable package.
    ///
    /// Definitions are returned dependency-first and do not include the compiler's standard
    /// catalog. Embedders can use this surface to compose package Methods with additional
    /// frontend-authored definitions under one authoritative `MethodRegistry` validation.
    pub fn package_method_definitions(&self) -> Result<Vec<MethodDefinition>, ProjectError> {
        let roots = self.ordered_reachable_roots(&self.default_member);
        self.load_method_definitions(&roots)
    }

    /// Build the complete Method registry for the default runnable package.
    pub fn method_registry(&self) -> Result<MethodRegistry, ProjectError> {
        self.method_registry_for(&self.default_member)
    }

    fn validate_method_registries(&self) -> Result<MethodRegistry, ProjectError> {
        let mut default = None;
        for member in &self.members {
            let registry = self.method_registry_for(member)?;
            if member == &self.default_member {
                default = Some(registry);
            }
        }
        Ok(default.expect("a validated project always has a default member"))
    }

    fn method_registry_for(&self, root: &PathBuf) -> Result<MethodRegistry, ProjectError> {
        let roots = self.ordered_reachable_roots(root);
        let mut definitions = lab_compiler::method::standard_method_definitions();
        definitions.extend(self.load_method_definitions(&roots)?);
        MethodRegistry::new(definitions).map_err(|source| ProjectError::InvalidMethodRegistry {
            package: self.packages[root].package.manifest.package.name.clone(),
            source: Box::new(source),
        })
    }

    fn load_method_definitions(
        &self,
        roots: &[&PathBuf],
    ) -> Result<Vec<MethodDefinition>, ProjectError> {
        let mut definitions = Vec::new();
        for root in roots {
            let package = &self.packages[*root].package;
            for document in &package.manifest.methods.documents {
                let joined = package.root.join(document);
                let path = fs::canonicalize(&joined).map_err(|source| {
                    ProjectError::ResolveMethodCatalog {
                        package: package.manifest.package.name.clone(),
                        document: document.clone(),
                        path: joined.clone(),
                        source,
                    }
                })?;
                if !path.starts_with(&package.root) {
                    return Err(ProjectError::MethodCatalogOutsidePackage {
                        package: package.manifest.package.name.clone(),
                        document: document.clone(),
                    });
                }
                let contents = fs::read_to_string(&path).map_err(|source| {
                    ProjectError::ReadMethodCatalog {
                        package: package.manifest.package.name.clone(),
                        path: path.clone(),
                        source,
                    }
                })?;
                let catalog =
                    serde_json::from_str::<MethodCatalogDocument>(&contents).map_err(|source| {
                        ProjectError::ParseMethodCatalog {
                            package: package.manifest.package.name.clone(),
                            path: path.clone(),
                            source,
                        }
                    })?;
                definitions.extend(catalog.into_methods().map_err(|source| {
                    ProjectError::InvalidMethodCatalog {
                        package: package.manifest.package.name.clone(),
                        path: path.clone(),
                        source: Box::new(source),
                    }
                })?);
            }
        }
        Ok(definitions)
    }

    pub fn lock(&self) -> ProjectLock {
        ProjectLock {
            schema_version: LOCK_SCHEMA_VERSION,
            members: self
                .member_packages()
                .iter()
                .map(|package| package.manifest.package.name.clone())
                .collect(),
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
                        } else if self.members.contains(root) {
                            LockedSource::Member {
                                path: relative_path(&self.root, root),
                            }
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

/// Compiles one SBOL document into a checked module.
///
/// Components that no kind in scope describes are skipped rather than fatal.
/// A registry export is large and partly outside any one program's vocabulary,
/// and refusing a whole file over one unrecognized term would make writing
/// designs in SBOL unusable against real registry data. What was skipped is
/// reported through the module's diagnostics rather than discarded silently.
fn compile_sbol_module(
    name: &str,
    text: &str,
    syntax: SbolSyntax,
    environment: &SemanticEnvironment,
) -> Result<lab_language::CheckedModule, ProjectError> {
    let format = match syntax {
        SbolSyntax::Turtle => RdfFormat::Turtle,
        SbolSyntax::NTriples => RdfFormat::NTriples,
        SbolSyntax::JsonLd => RdfFormat::JsonLd,
        SbolSyntax::RdfXml => RdfFormat::RdfXml,
    };
    let document = Document::read(text, format).map_err(|error| ProjectError::Parse {
        module: name.to_owned(),
        message: error.to_string(),
    })?;

    let mut grounding = Grounding::bundled();
    for interface in environment.interfaces() {
        grounding.add_interface(interface);
    }
    let kinds = KindIndex::new(&grounding);

    let (module, skipped) = lab_sbol::read_module(
        ModuleId::new(name.to_owned()),
        &document,
        &kinds,
        environment,
    );
    let module = module.map_err(|error| ProjectError::Compile {
        module: name.to_owned(),
        message: error.to_string(),
    })?;
    if !skipped.is_empty() {
        let detail = skipped
            .iter()
            .map(|skipped| skipped.reason.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ProjectError::Compile {
            module: name.to_owned(),
            message: format!("this document states designs Lab cannot read: {detail}"),
        });
    }
    Ok(module)
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
        // A document names no sibling module. It describes components and the
        // terms they stand for, and which package declares the kinds those
        // terms name is derived when it is read, so it depends on nothing
        // inside this package and is ready to compile from the start.
        let local_imports = match source.language {
            SourceLanguage::Sbol(_) => BTreeSet::new(),
            SourceLanguage::Lab => {
                let syntax = parse_module(&text).map_err(|error| ProjectError::Parse {
                    module: source.module.clone(),
                    message: error.to_string(),
                })?;
                syntax
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
                    .collect::<BTreeSet<_>>()
            }
        };
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
            let module = match source.language {
                SourceLanguage::Lab => {
                    compile_module_in_environment(ModuleId::new(name.clone()), &text, &environment)
                        .map_err(|error| ProjectError::Compile {
                            module: name.clone(),
                            message: error.to_string(),
                        })?
                }
                SourceLanguage::Sbol(syntax) => {
                    compile_sbol_module(&name, &text, syntax, &environment)?
                }
            };
            environment.insert(name.clone(), module.interface.clone());
            compiled_names.insert(name);
            result.push(CompiledModule {
                package: package.manifest.package.name.clone(),
                source,
                module,
            });
        }
    }

    // A package that names an entry point declares a program, and a program
    // has to say what it runs. Build order still comes from material
    // dataflow, so 'main' is where that dataflow starts rather than an
    // ordering directive.
    if let Some(entry) = package.entry_source() {
        let declares_main = result
            .iter()
            .find(|compiled| compiled.source.module == entry.module)
            .is_some_and(|compiled| {
                compiled.module.declarations.iter().any(|declaration| {
                    matches!(declaration, CheckedDeclaration::Workflow { name, .. } if name == "main")
                })
            });
        if !declares_main {
            return Err(ProjectError::MissingMainWorkflow {
                package: package.manifest.package.name.clone(),
                module: entry.module.clone(),
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

    use lab_capability::MethodId;

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

        fn workspace(&self, manifest: &str) -> PathBuf {
            fs::write(self.0.join("lab.toml"), manifest).unwrap();
            self.0.clone()
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const DONOR: &str = r#"use std.bio.designs

plasmid donor:
  sequence = dna("ACGT")
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
use std.bio.designs

plasmid derived:
  sequence = donor.sequence
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
use std.bio.designs

plasmid derived:
  sequence = donor.sequence
  require topology == circular
  accept sequence == design.sequence
"#,
            )],
        );
        let compiled = LabProject::discover(root).unwrap().compile().unwrap();
        assert_eq!(compiled.modules.len(), 2);
        assert_eq!(compiled.lock.packages[0].name, "shared");
        assert_eq!(compiled.lock.packages[1].dependencies["shared"], "shared");
        assert_eq!(compiled.members, ["app"]);
        let lock = compiled.lock.to_toml().unwrap();
        assert!(lock.contains(&format!("schema_version = {LOCK_SCHEMA_VERSION}")));
        assert!(lock.contains("members = [\"app\"]"));
    }

    #[test]
    fn composes_versioned_method_documents_from_reachable_dependencies() {
        let fixture = TestProject::new();
        let shared = fixture.package(
            "shared",
            r#"[package]
name = "shared"
version = "1.2.0"

[methods]
documents = ["methods/recovery.json"]
"#,
            &[("values.lab", DONOR)],
        );
        let mut custom = lab_compiler::method::standard_method_definitions()
            .into_iter()
            .find(|definition| definition.refines.as_str() == "std.lab.plasmid.recover")
            .unwrap();
        custom.id = MethodId::new("https://example.org/method/custom-recovery").unwrap();
        let catalog = MethodCatalogDocument::new(vec![custom.clone()]).unwrap();
        fs::create_dir_all(shared.join("methods")).unwrap();
        fs::write(
            shared.join("methods/recovery.json"),
            serde_json::to_string_pretty(&catalog).unwrap(),
        )
        .unwrap();
        let root = fixture.package(
            "app",
            r#"[package]
name = "app"
version = "0.1.0"

[dependencies]
shared = { path = "../shared" }
"#,
            &[("main.lab", DONOR)],
        );

        let project = LabProject::discover(root).unwrap();
        assert_eq!(project.package_method_definitions().unwrap(), [custom]);
        let compiled = project.compile().unwrap();
        let recovery = compiled.methods.methods_for(
            &lab_compiler::method::IntentOperationId::new("std.lab.plasmid.recover").unwrap(),
        );

        assert_eq!(
            recovery
                .iter()
                .map(|method| method.id.as_str())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                "https://www.lab-compiler.org/ns/method#manual-recovery",
                "https://www.lab-compiler.org/ns/method#controlled-recovery",
                "https://www.lab-compiler.org/ns/method#automated-recovery",
                "https://example.org/method/custom-recovery",
            ])
        );
    }

    #[test]
    fn compilation_rejects_an_unknown_method_catalog_schema() {
        let fixture = TestProject::new();
        let root = fixture.package(
            "app",
            r#"[package]
name = "app"
version = "0.1.0"

[methods]
documents = ["methods/catalog.json"]
"#,
            &[("main.lab", DONOR)],
        );
        fs::create_dir_all(root.join("methods")).unwrap();
        fs::write(
            root.join("methods/catalog.json"),
            r#"{"schema_version":"lab.method-catalog.v99","methods":[]}"#,
        )
        .unwrap();

        let error = LabProject::discover(root).unwrap().compile().unwrap_err();

        let ProjectError::InvalidMethodCatalog { source, .. } = error else {
            panic!("expected an invalid Method catalog, got {error}");
        };
        assert!(matches!(
            source.as_ref(),
            MethodCatalogError::UnsupportedSchema { .. }
        ));
    }

    #[test]
    fn compiles_every_workspace_member_and_locks_them_together() {
        let fixture = TestProject::new();
        fixture.package(
            "catalog",
            "[package]\nname = \"catalog\"\nversion = \"0.1.0\"\n",
            &[("values.lab", DONOR)],
        );
        fixture.package(
            "device",
            r#"[package]
name = "device"
version = "0.1.0"

[dependencies]
catalog = { path = "../catalog" }
"#,
            &[(
                "main.lab",
                r#"use catalog.values
use std.bio.designs

plasmid derived:
  sequence = donor.sequence
  require topology == circular
  accept sequence == design.sequence
"#,
            )],
        );
        let root = fixture.workspace(
            r#"[workspace]
members = ["catalog", "device"]
default-member = "device"
"#,
        );

        let project = LabProject::discover(&root).unwrap();
        assert_eq!(
            project.default_package().manifest.package.name,
            "device",
            "default-member selects the package a single-package command acts on"
        );
        assert_eq!(project.program_packages(), ["catalog", "device"]);

        let compiled = project.compile().unwrap();
        assert_eq!(compiled.members, ["catalog", "device"]);
        assert_eq!(compiled.modules.len(), 2);
        assert!(matches!(
            compiled.lock.packages[0].source,
            LockedSource::Member { .. }
        ));
    }

    #[test]
    fn rejects_a_workspace_that_does_not_select_a_default_member() {
        let fixture = TestProject::new();
        fixture.package(
            "catalog",
            "[package]\nname = \"catalog\"\nversion = \"0.1.0\"\n",
            &[("values.lab", DONOR)],
        );
        fixture.package(
            "device",
            "[package]\nname = \"device\"\nversion = \"0.1.0\"\n",
            &[("main.lab", DONOR)],
        );
        let root = fixture.workspace("[workspace]\nmembers = [\"catalog\", \"device\"]\n");

        let Err(error) = LabProject::discover(&root) else {
            panic!("an ambiguous workspace default member is rejected");
        };
        assert!(error.to_string().contains("default-member"), "{error}");
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

    const ENTRY_MANIFEST: &str =
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[build]\nentry = \"src/main.lab\"\n";

    #[test]
    fn an_entry_module_must_declare_a_main_workflow() {
        let fixture = TestProject::new();
        let root = fixture.package("app", ENTRY_MANIFEST, &[("main.lab", DONOR)]);
        let error = LabProject::discover(root).unwrap().compile().unwrap_err();
        assert!(matches!(
            error,
            ProjectError::MissingMainWorkflow { ref module, .. } if module == "app.main"
        ));
    }

    #[test]
    fn an_entry_module_that_declares_main_compiles() {
        let fixture = TestProject::new();
        let root = fixture.package(
            "app",
            ENTRY_MANIFEST,
            &[(
                "main.lab",
                &format!(
                    r#"use std.bio.build

{DONOR}
workflow main() -> Material<Plasmid>:
  product <- realize donor
  return product
"#
                ),
            )],
        );
        let compiled = LabProject::discover(root).unwrap().compile().unwrap();
        assert_eq!(compiled.modules.len(), 1);
    }

    /// A package that names no entry point is a library, and a library owes
    /// no `main`.
    #[test]
    fn a_package_without_an_entry_needs_no_main_workflow() {
        let fixture = TestProject::new();
        let root = fixture.package(
            "app",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
            &[("main.lab", DONOR)],
        );
        assert!(LabProject::discover(root).unwrap().compile().is_ok());
    }

    #[test]
    fn ebef_reference_package_compiles_as_a_portable_library() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ebef");
        let compiled = LabProject::discover(root).unwrap().compile().unwrap();
        assert_eq!(compiled.modules.len(), 1);
        assert_eq!(compiled.modules[0].source.module, "ebef_reference.facility");
    }

    #[test]
    fn plans_a_project_through_exact_facility_and_material_allocations() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/golden-gate");
        let project = LabProject::discover(root).unwrap();
        let compiled = project.compile().unwrap();
        let planned = project
            .plan_facility_with_package_methods(&compiled)
            .unwrap();

        assert_eq!(
            planned.inventory.facility().as_str(),
            "https://example.org/golden-gate/facility"
        );
        assert!(planned.refined_lair.contains("method.choice"));
        assert_eq!(
            planned.problem().sha256(),
            planned.solution().problem_sha256
        );
        let allocated_ir = planned.allocated.ir();
        let reparsed =
            lab_compiler::program::AllocatedLairProgram::parse_ir(&allocated_ir).unwrap();
        assert_eq!(
            reparsed.allocated_program().unwrap(),
            planned.allocated.allocated_program().unwrap()
        );
        let reprojected =
            lab_adapters::AdapterInvocationPlan::from_allocated_lair(&reparsed).unwrap();
        reprojected
            .allocated
            .validate_against_material_inventory(&planned.material_inventory)
            .unwrap();
        assert_eq!(reprojected, planned.adapter_invocations);
        assert_eq!(
            planned.material_inventory.source_sha256(),
            planned.inventory.source_sha256()
        );
        assert_eq!(
            planned.material_inventory.facility(),
            planned.inventory.facility().as_str()
        );
        assert!(
            planned
                .adapter_invocations
                .invocations
                .iter()
                .any(|invocation| invocation.adapter.driver == "opentrons.ot2")
        );
        assert!(planned.solution().selections.iter().any(|method| {
            method.tasks.iter().any(|task| {
                task.materials.iter().any(|material| {
                    matches!(
                        material.source,
                        lab_compiler::planning::SelectedMaterialSource::MaterialLot { .. }
                    )
                })
            })
        }));
    }
}
