//! The generated-source-aware application boundary for project compilation and planning.
//!
//! Frontends should enter through [`ProjectCompilation::load`] instead of separately running
//! generators, discovering packages, compiling modules, and choosing an entry workflow. Keeping
//! that orchestration here makes the CLI and language bindings clients of the same service.

use std::path::{Path, PathBuf};
use std::process::Command;

use lab_adapter_api::AdapterRegistry;
use lab_compiler::method::{MethodDefinition, MethodRegistry, MethodRegistryError};
use lab_compiler::planning::{PlanningProblem, PlanningProblemExtractionError};
use lab_compiler::procedure::{ProcedureCompiler, ProcedureMethodRegistryError};
use lab_compiler::program::{PortableLairError, PortableLairProgram, RefinedLairError};
use lab_language::{
    Analysis, CheckedDeclaration, CheckedModule, ModuleId, SemanticEnvironment, SourceId,
    analyze_module_in_environment,
};
use lab_package::{LabPackage, PackageError, source_generator};
use thiserror::Error;

use crate::artifacts::{
    FacilityArtifactRequest, ProjectArtifactBuildError, ProjectBuildArtifactRequest,
    build_facility_artifacts, build_project_artifacts,
};
use crate::facility::{
    load_package_inventory, plan_modules_for_package, resolve_package_adapter_bindings,
};
use crate::{
    CompiledProject, FacilityArtifactBuild, FacilityArtifactError, FacilityDocumentRenderer,
    FacilityPlanningResult, FacilityProjectError, LabProject, ProjectArtifactBuild, ProjectError,
};

/// A loaded and compiled project, produced through the one supported application entry point.
#[derive(Debug)]
pub struct ProjectCompilation {
    project: LabProject,
    compiled: CompiledProject,
    adapters: AdapterRegistry,
    procedures: ProcedureCompiler,
}

/// A generated-source-aware project context for embedders that supply checked modules themselves.
#[derive(Debug)]
pub struct ProjectContext {
    project: LabProject,
    adapters: AdapterRegistry,
    procedures: ProcedureCompiler,
}

/// The facility-independent result of the shared source-to-Method application operation.
///
/// Language bindings receive owned records rather than reconstructing compiler passes or
/// retaining Pliron state.
#[derive(Clone, Debug)]
pub struct CompilerRefinement {
    pub refined_lair: String,
    pub planning_problem: PlanningProblem,
}

/// One in-memory source analysis resolved against a compiled project's package interfaces.
#[derive(Clone, Debug)]
pub struct ProjectModuleAnalysis {
    pub module: String,
    pub analysis: Analysis,
}

/// Selects the runnable workflow a project planning request should root at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectProgram<'a> {
    /// Use the default package's declared `build.entry`.
    #[default]
    Default,
    /// Select a file stem under the default package's `src/programs/` directory.
    Named(&'a str),
}

/// Inputs to the shared project-planning operation.
#[derive(Clone, Copy, Debug)]
pub struct ProjectPlanningRequest<'a> {
    pub program: ProjectProgram<'a>,
    /// Override the registry captured during package compilation. Embedders can use this to add
    /// in-memory Methods without recreating project planning.
    pub methods: Option<&'a MethodRegistry>,
}

/// Inputs to the end-to-end project operation that writes reviewed facility artifacts.
pub struct ProjectArtifactRequest<'a> {
    pub program: ProjectProgram<'a>,
    pub methods: Option<&'a MethodRegistry>,
    pub output_root: &'a Path,
    pub renderer: Option<&'a dyn FacilityDocumentRenderer>,
}

/// Inputs to the complete package-build operation shared by every frontend.
pub struct ProjectBuildRequest<'a> {
    pub program: ProjectProgram<'a>,
    pub methods: Option<&'a MethodRegistry>,
    pub output_root: &'a Path,
    pub renderer: Option<&'a dyn FacilityDocumentRenderer>,
}

impl Default for ProjectPlanningRequest<'_> {
    fn default() -> Self {
        Self {
            program: ProjectProgram::Default,
            methods: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ProjectApplicationError {
    #[error("application extension composition is invalid")]
    Extensions(#[from] crate::ApplicationExtensionError),
    #[error("failed to inspect build.generate for {path}")]
    GeneratorLookup {
        path: PathBuf,
        #[source]
        source: Box<PackageError>,
    },
    #[error("failed to run build.generate command `{command}` from {root}")]
    GeneratorSpawn {
        root: PathBuf,
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("build.generate command `{command}` failed from {root}:\n{stdout}{stderr}")]
    GeneratorFailed {
        root: PathBuf,
        command: String,
        stdout: String,
        stderr: String,
    },
    #[error("failed to load project from {path}")]
    Discover {
        path: PathBuf,
        #[source]
        source: Box<ProjectError>,
    },
    #[error("project compilation failed")]
    Compile(#[source] ProjectError),
    #[error("project inventory or adapter configuration is invalid")]
    Inventory(#[source] FacilityProjectError),
    #[error("application Method composition is invalid")]
    Methods(#[source] MethodRegistryError),
    #[error("application Method and Procedure composition is invalid")]
    Procedures(#[source] ProcedureMethodRegistryError),
    #[error("{0}")]
    ProgramSelection(String),
    #[error("facility planning failed")]
    Planning(#[source] FacilityProjectError),
}

#[derive(Debug, Error)]
pub enum CompilerRefinementError {
    #[error("failed to lower checked modules into Lab Intent")]
    Lower(#[source] PortableLairError),
    #[error("failed to refine Lab Intent through the supplied Method composition")]
    Refine(#[source] RefinedLairError),
    #[error("failed to project the refined program into a planning problem")]
    Planning(#[source] PlanningProblemExtractionError),
}

#[derive(Debug, Error)]
pub enum ProjectArtifactError {
    #[error("project planning failed")]
    Planning(#[source] Box<ProjectApplicationError>),
    #[error(transparent)]
    Artifacts(#[from] FacilityArtifactError),
}

#[derive(Debug, Error)]
pub enum ProjectBuildError {
    #[error("failed to select the build program")]
    ProgramSelection(#[source] Box<ProjectApplicationError>),
    #[error("failed to plan and lower the build against its facility")]
    Facility(#[source] Box<ProjectArtifactError>),
    #[error(transparent)]
    Artifacts(#[from] ProjectArtifactBuildError),
}

impl ProjectCompilation {
    /// Runs the nearest package source generator, then discovers, validates, and compiles the
    /// project from the resulting source tree.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectApplicationError> {
        ProjectContext::load(path)?.compile()
    }

    /// Runs the project pipeline with the exact application extension composition.
    pub fn load_with_extensions(
        path: impl AsRef<Path>,
        adapters: AdapterRegistry,
        procedures: ProcedureCompiler,
    ) -> Result<Self, ProjectApplicationError> {
        ProjectContext::load_with_extensions(path, adapters, procedures)?.compile()
    }

    pub fn project(&self) -> &LabProject {
        &self.project
    }

    pub fn compiled(&self) -> &CompiledProject {
        &self.compiled
    }

    pub fn adapters(&self) -> &AdapterRegistry {
        &self.adapters
    }

    pub fn procedures(&self) -> &ProcedureCompiler {
        &self.procedures
    }

    /// Compose package Methods with frontend-authored definitions without re-reading project
    /// files. Package compilation captures and validates the authoritative Method set once.
    pub fn compose_methods(
        &self,
        additional: impl IntoIterator<Item = MethodDefinition>,
        include_standard: bool,
    ) -> Result<MethodRegistry, ProjectApplicationError> {
        let standard_ids = lab_compiler::method::standard_method_definitions()
            .into_iter()
            .map(|definition| definition.id)
            .collect::<std::collections::BTreeSet<_>>();
        let mut definitions = self
            .compiled
            .methods
            .definitions()
            .filter(|definition| include_standard || !standard_ids.contains(&definition.id))
            .cloned()
            .collect::<Vec<_>>();
        definitions.extend(additional);
        let registry =
            MethodRegistry::new(definitions).map_err(ProjectApplicationError::Methods)?;
        self.procedures
            .validate_methods(&registry)
            .map_err(ProjectApplicationError::Procedures)?;
        Ok(registry)
    }

    /// Analyze in-memory modules against all interfaces captured by this project compilation.
    /// Modules are then added in caller order so they may import earlier in-memory modules too.
    pub fn analyze_modules(&self, modules: &[(String, String)]) -> Vec<ProjectModuleAnalysis> {
        let program_packages = self.project.program_packages();
        let mut environment = SemanticEnvironment::new(
            self.compiled
                .modules
                .iter()
                .filter(|module| program_packages.contains(&module.package))
                .map(|module| module.module.interface.clone()),
        );
        modules
            .iter()
            .map(|(name, source)| {
                let analysis = analyze_module_in_environment(
                    SourceId::new(name.clone()),
                    ModuleId::new(name.clone()),
                    source,
                    &environment,
                );
                if let Some(checked) = &analysis.checked {
                    environment.insert(name.clone(), checked.interface.clone());
                }
                ProjectModuleAnalysis {
                    module: name.clone(),
                    analysis,
                }
            })
            .collect()
    }

    /// Plans one selected program through Method refinement, facility allocation, and exact
    /// adapter invocation projection.
    pub fn plan(
        &self,
        request: ProjectPlanningRequest<'_>,
    ) -> Result<FacilityPlanningResult, ProjectApplicationError> {
        let methods = request.methods.unwrap_or(&self.compiled.methods);
        match request.program {
            ProjectProgram::Default => {
                let entry = self
                    .project
                    .default_package()
                    .entry_source()
                    .ok_or_else(|| {
                        ProjectApplicationError::ProgramSelection(missing_program_selection(
                            self.project.default_package(),
                        ))
                    })?
                    .module
                    .clone();
                self.project
                    .plan_facility_program_with_methods(
                        &self.compiled,
                        &entry,
                        methods,
                        &self.procedures,
                        &self.adapters,
                    )
                    .map_err(ProjectApplicationError::Planning)
            }
            ProjectProgram::Named(program) => {
                let entry =
                    resolve_program(self.project.default_package(), &self.compiled, program)?;
                self.project
                    .plan_facility_program_with_methods(
                        &self.compiled,
                        &entry,
                        methods,
                        &self.procedures,
                        &self.adapters,
                    )
                    .map_err(ProjectApplicationError::Planning)
            }
        }
    }

    /// Plans already checked in-memory modules against this compiled project's captured package
    /// context. This is the embedding counterpart of [`Self::plan`].
    pub fn plan_modules(
        &self,
        modules: &[&CheckedModule],
        entry_module: &str,
        methods: &MethodRegistry,
    ) -> Result<FacilityPlanningResult, ProjectApplicationError> {
        let program_packages = self.project.program_packages();
        let mut complete_program = self
            .compiled
            .modules
            .iter()
            .filter(|module| program_packages.contains(&module.package))
            .map(|module| &module.module)
            .collect::<Vec<_>>();
        complete_program.extend_from_slice(modules);
        plan_modules_for_package(
            self.project.default_package(),
            &complete_program,
            entry_module,
            methods,
            &self.procedures,
            &self.adapters,
        )
        .map_err(ProjectApplicationError::Planning)
    }

    /// Refines already checked in-memory modules together with the package modules they import.
    pub fn refine_modules(
        &self,
        modules: &[&CheckedModule],
        entry_module: &str,
        methods: &MethodRegistry,
    ) -> Result<CompilerRefinement, CompilerRefinementError> {
        let program_packages = self.project.program_packages();
        let mut complete_program = self
            .compiled
            .modules
            .iter()
            .filter(|module| program_packages.contains(&module.package))
            .map(|module| &module.module)
            .collect::<Vec<_>>();
        complete_program.extend_from_slice(modules);
        refine_modules(&complete_program, entry_module, methods, &self.procedures)
    }

    /// Plans the selected program and writes its complete reviewed artifact bundle in one
    /// operation. Frontends provide only an optional presentation renderer.
    pub fn write_facility_artifacts(
        &self,
        request: ProjectArtifactRequest<'_>,
    ) -> Result<FacilityArtifactBuild, ProjectArtifactError> {
        let planning = self
            .plan(ProjectPlanningRequest {
                program: request.program,
                methods: request.methods,
            })
            .map_err(|error| ProjectArtifactError::Planning(Box::new(error)))?;
        build_facility_artifacts(FacilityArtifactRequest {
            package: self.project.default_package(),
            planning: &planning,
            output_root: request.output_root,
            renderer: request.renderer,
        })
        .map_err(Into::into)
    }

    /// Writes module snapshots, compiler frontier, optional facility bundle, package index, and
    /// lockfile as one coherent build. This is the sole build-artifact service for frontends.
    pub fn write_build_artifacts(
        &self,
        request: ProjectBuildRequest<'_>,
    ) -> Result<ProjectArtifactBuild, ProjectBuildError> {
        let package = self.project.default_package();
        let entry_module = match request.program {
            ProjectProgram::Default => package.entry_source().map(|source| source.module.clone()),
            ProjectProgram::Named(program) => Some(
                resolve_program(package, &self.compiled, program)
                    .map_err(|error| ProjectBuildError::ProgramSelection(Box::new(error)))?,
            ),
        };
        let facility = if package.manifest.inventory.document.is_some() && entry_module.is_some() {
            Some(
                self.write_facility_artifacts(ProjectArtifactRequest {
                    program: request.program,
                    methods: request.methods,
                    output_root: request.output_root,
                    renderer: request.renderer,
                })
                .map_err(|error| ProjectBuildError::Facility(Box::new(error)))?,
            )
        } else {
            None
        };
        build_project_artifacts(ProjectBuildArtifactRequest {
            project: &self.project,
            compiled: &self.compiled,
            adapters: &self.adapters,
            procedures: &self.procedures,
            methods: request.methods.unwrap_or(&self.compiled.methods),
            entry_module: entry_module.as_deref(),
            output_root: request.output_root,
            facility,
        })
        .map_err(Into::into)
    }
}

/// Refines checked modules through an explicitly composed application pipeline.
///
/// This is the in-memory counterpart of [`ProjectCompilation::plan`]. Language bindings and
/// other embedders supply both extension registries; this boundary never selects a built-in
/// composition on their behalf.
pub fn refine_modules(
    modules: &[&CheckedModule],
    entry_module: &str,
    methods: &MethodRegistry,
    procedures: &ProcedureCompiler,
) -> Result<CompilerRefinement, CompilerRefinementError> {
    let refined = PortableLairProgram::lower_entry_program(modules, entry_module)
        .map_err(CompilerRefinementError::Lower)?
        .refine_methods(methods, procedures)
        .map_err(CompilerRefinementError::Refine)?;
    let planning_problem = refined
        .planning_problem()
        .map_err(CompilerRefinementError::Planning)?;
    Ok(CompilerRefinement {
        refined_lair: refined.ir(),
        planning_problem,
    })
}

impl ProjectContext {
    /// Runs source generation and validates the package, inventory, and adapter context without
    /// compiling its Lab modules. This is the appropriate boundary for non-file frontends.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectApplicationError> {
        let extensions = crate::application_extensions()?;
        Self::load_with_extensions(path, extensions.adapters, extensions.procedures)
    }

    /// Loads a project against the exact adapter and Procedure composition supplied by the
    /// embedding application. This is the sole out-of-tree extension assembly point.
    pub fn load_with_extensions(
        path: impl AsRef<Path>,
        adapters: AdapterRegistry,
        procedures: ProcedureCompiler,
    ) -> Result<Self, ProjectApplicationError> {
        let path = path.as_ref();
        run_source_generator(path)?;
        let project =
            LabProject::discover(path).map_err(|source| ProjectApplicationError::Discover {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
        validate_project_inventories(&project, &adapters)?;
        Ok(Self {
            project,
            adapters,
            procedures,
        })
    }

    pub fn project(&self) -> &LabProject {
        &self.project
    }

    pub fn adapters(&self) -> &AdapterRegistry {
        &self.adapters
    }

    pub fn procedures(&self) -> &ProcedureCompiler {
        &self.procedures
    }

    pub fn compile(self) -> Result<ProjectCompilation, ProjectApplicationError> {
        let compiled = self
            .project
            .compile()
            .map_err(ProjectApplicationError::Compile)?;
        self.procedures
            .validate_methods(&compiled.methods)
            .map_err(ProjectApplicationError::Procedures)?;
        Ok(ProjectCompilation {
            project: self.project,
            compiled,
            adapters: self.adapters,
            procedures: self.procedures,
        })
    }
}

fn run_source_generator(path: &Path) -> Result<(), ProjectApplicationError> {
    let generator =
        source_generator(path).map_err(|source| ProjectApplicationError::GeneratorLookup {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
    let Some((root, command)) = generator else {
        return Ok(());
    };
    let generated = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&root)
        .output()
        .map_err(|source| ProjectApplicationError::GeneratorSpawn {
            root: root.clone(),
            command: command.clone(),
            source,
        })?;
    if !generated.status.success() {
        return Err(ProjectApplicationError::GeneratorFailed {
            root,
            command,
            stdout: String::from_utf8_lossy(&generated.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&generated.stderr).into_owned(),
        });
    }
    Ok(())
}

fn validate_project_inventories(
    project: &LabProject,
    adapters: &AdapterRegistry,
) -> Result<(), ProjectApplicationError> {
    for package in project.member_packages() {
        let Some(snapshot) =
            load_package_inventory(package).map_err(ProjectApplicationError::Inventory)?
        else {
            continue;
        };
        resolve_package_adapter_bindings(package, &snapshot, adapters)
            .map_err(ProjectApplicationError::Inventory)?;
    }
    Ok(())
}

/// The entry module of one named program under `src/programs/`.
fn resolve_program(
    package: &LabPackage,
    compiled: &CompiledProject,
    program: &str,
) -> Result<String, ProjectApplicationError> {
    let stem = program.replace('-', "_");
    let source = package
        .program_sources()
        .find(|source| {
            source
                .relative_path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(|name| name.replace('-', "_"))
                .as_deref()
                == Some(stem.as_str())
        })
        .ok_or_else(|| {
            ProjectApplicationError::ProgramSelection(unknown_program(package, program))
        })?;
    let declares_main = compiled
        .modules
        .iter()
        .find(|module| module.source.module == source.module)
        .is_some_and(|module| {
            module.module.declarations.iter().any(|declaration| {
                matches!(declaration, CheckedDeclaration::Workflow { name, .. } if name == "main")
            })
        });
    if !declares_main {
        return Err(ProjectApplicationError::ProgramSelection(format!(
            "program '{program}' ({}) declares no `main` workflow",
            source.module
        )));
    }
    Ok(source.module.clone())
}

fn program_names(package: &LabPackage) -> Vec<String> {
    package
        .program_sources()
        .filter_map(|source| {
            source
                .relative_path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .collect()
}

fn unknown_program(package: &LabPackage, program: &str) -> String {
    let programs = program_names(package);
    if programs.is_empty() {
        format!(
            "package '{}' has no programs under src/programs/",
            package.manifest.package.name
        )
    } else {
        format!(
            "no program '{program}' under src/programs/; available: {}",
            programs.join(", ")
        )
    }
}

fn missing_program_selection(package: &LabPackage) -> String {
    let programs = program_names(package);
    if programs.is_empty() {
        format!(
            "package '{}' is a library with no build.entry; a facility plan needs an exact main workflow",
            package.manifest.package.name
        )
    } else {
        format!(
            "package declares no build.entry; pick a program with --program <name>: {}",
            programs.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn compilation_runs_the_declared_source_generator_first() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("src/programs")).unwrap();
        fs::write(
            directory.path().join("lab.toml"),
            r#"[package]
name = "generated"
version = "0.1.0"
edition = "2026"

[build]
entry = "src/programs/main.lab"
generate = "cp source.lab src/programs/main.lab"
"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("source.lab"),
            r#"use std.bio.designs
use std.bio.build

build medium LB_broth:
  components = [
    Ingredient { substance: "tryptone", concentration: 10 g/L },
  ]

workflow main() -> Material<Medium>:
  product <- realize LB_broth
  return product
"#,
        )
        .unwrap();

        let compilation = ProjectCompilation::load(directory.path()).unwrap();

        assert_eq!(compilation.compiled().modules.len(), 1);
        assert!(directory.path().join("src/programs/main.lab").is_file());
    }
}
