//! Facility-derived adapter lowering and immutable artifact staging.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_adapter_api::{
    AdapterInvocation, AdapterInvocationPlan, AdapterRegistry, ArtifactBundle,
    ValidatedAdapterProfile,
};
use lab_capability::ProcedureImplementationId;
use lab_compiler::method::MethodRegistry;
use lab_compiler::procedure::{ProcedureCompiler, ProcedureContractRegistry};
use lab_compiler::program::PortableLairProgram;
use lab_facility::{ExecutionPlanOptions, build_execution_plan_from_invocations};
use lab_inventory::InventorySnapshot;
use lab_language::{CheckedDeclaration, CheckedModule};
use lab_package::LabPackage;
use lab_runfmt::{
    EXECUTION_PLAN_FILE, ExecutionMethodSelection, ExecutionPlanDocument,
    ExecutionPlanningArtifact, ExecutionPlanningReference, ReviewedRunDocument,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const FACILITY_LOWERING_SCHEMA_VERSION: &str = "lab.facility-lowering.v2";

use crate::facility::{load_package_inventory, resolve_package_adapter_bindings};
use crate::{CompiledProject, FacilityPlanningResult, LOCK_FILE, LabProject};

/// Inputs to the shared package-build artifact operation.
pub(crate) struct ProjectBuildArtifactRequest<'a> {
    pub project: &'a LabProject,
    pub compiled: &'a CompiledProject,
    pub adapters: &'a AdapterRegistry,
    pub procedures: &'a ProcedureCompiler,
    pub methods: &'a MethodRegistry,
    pub entry_module: Option<&'a str>,
    pub output_root: &'a Path,
    pub facility: Option<FacilityArtifactBuild>,
}

/// Summary returned by every frontend after one complete package build.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectArtifactBuild {
    pub package: String,
    pub version: String,
    pub modules: usize,
    pub output: PathBuf,
    pub products: Vec<BuildProduct>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facility: Option<FacilityArtifactBuild>,
}

/// A biological artifact declared for construction by the selected program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuildProduct {
    pub package: String,
    pub module: String,
    pub kind: String,
    pub name: String,
}

#[derive(Serialize)]
struct BuildIndex {
    schema_version: u32,
    package: String,
    version: String,
    edition: String,
    entry: Option<PathBuf>,
    members: Vec<String>,
    modules: Vec<BuildModule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compiler: Option<BuildCompilerIndex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter_bindings: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    facility: Option<BuildFacilityIndex>,
}

#[derive(Serialize)]
struct BuildCompilerIndex {
    refined_lair: PathBuf,
    planning_problem: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    facility_solution: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allocated_lair: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter_invocations: Option<PathBuf>,
}

#[derive(Serialize)]
struct BuildFacilityIndex {
    facility: String,
    facility_solution: PathBuf,
    adapter_invocations: PathBuf,
    lowering: PathBuf,
    execution_plan: PathBuf,
    bundles: Vec<PathBuf>,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
}

#[derive(Serialize)]
struct BuildModule {
    package: String,
    module: String,
    source: PathBuf,
    artifact: PathBuf,
}

/// Optional presentation boundary supplied by a frontend that can render Typst.
///
/// Adapter lowering and reviewed run documents do not depend on a PDF implementation. The CLI
/// implements this trait with its in-process Typst renderer; other embedders receive the complete
/// source bundle without taking on presentation dependencies.
pub trait FacilityDocumentRenderer {
    fn render_typst(&self, root: &Path, document: &str) -> Result<Vec<u8>, String>;
}

/// Inputs to the shared reviewed-artifact operation.
pub(crate) struct FacilityArtifactRequest<'a> {
    pub package: &'a LabPackage,
    pub planning: &'a FacilityPlanningResult,
    pub output_root: &'a Path,
    pub renderer: Option<&'a dyn FacilityDocumentRenderer>,
}

/// Paths and summary counts for one complete reviewed facility artifact directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FacilityArtifactBuild {
    pub package: String,
    pub version: String,
    pub output: PathBuf,
    pub facility: String,
    pub selected_methods: usize,
    pub allocated_requirements: usize,
    pub adapter_lowerings: usize,
    pub refined_lair: PathBuf,
    pub planning_problem: PathBuf,
    pub facility_solution: PathBuf,
    pub allocated_lair: PathBuf,
    pub adapter_invocations: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_bindings: Option<PathBuf>,
    pub lowering: PathBuf,
    pub execution_plan: PathBuf,
    pub bundles: Vec<PathBuf>,
    pub protocols: Vec<PathBuf>,
    pub documents: Vec<PathBuf>,
}

#[derive(Debug, Error)]
#[error("failed to assemble reviewed facility artifacts")]
pub struct FacilityArtifactError(#[source] anyhow::Error);

#[derive(Debug, Error)]
#[error("failed to assemble project build artifacts")]
pub struct ProjectArtifactBuildError(#[source] anyhow::Error);

/// Device artifacts emitted only after capability requirements have been allocated to a facility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FacilityLoweringManifest {
    schema_version: String,
    inventory_sha256: String,
    facility: String,
    routes: Vec<FacilityLoweringRoute>,
}

/// One exact Asset and adapter implementation selected by allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FacilityLoweringRoute {
    id: String,
    asset: String,
    driver: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    procedure_implementations: BTreeSet<ProcedureImplementationId>,
    profile_path: PathBuf,
    profile_sha256: String,
    requirements: Vec<FacilityLoweredRequirement>,
    output: PathBuf,
    artifacts: Vec<FacilityLoweredArtifact>,
}

/// A semantic requirement whose allocated route caused this adapter lowering to exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FacilityLoweredRequirement {
    requirement_instance: String,
    capability_kind: String,
    offering: String,
}

/// One immutable artifact emitted by an allocated adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FacilityLoweredArtifact {
    path: PathBuf,
    media_type: String,
    sha256: String,
    role: FacilityLoweredArtifactRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FacilityLoweredArtifactRole {
    AutomationProtocol,
    OperatorDocument,
    Support,
}

struct FacilityLoweringOutput {
    manifest: FacilityLoweringManifest,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
    reviewed_documents: BTreeMap<String, ReviewedRunDocument>,
}

struct LowerableInvocation {
    invocation: AdapterInvocation,
    procedure_implementations: BTreeSet<ProcedureImplementationId>,
    requirements: Vec<FacilityLoweredRequirement>,
}

struct WrittenFacilityArtifacts {
    artifacts: Vec<FacilityLoweredArtifact>,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
    reviewed_documents: BTreeMap<String, ReviewedRunDocument>,
}

/// Writes the complete compiler-facing package build through one application-owned boundary.
///
/// Module snapshots, the exact Method-refined frontier, facility artifacts, package index, and
/// lockfile are deliberately emitted together. Frontends only choose an output directory and an
/// optional document renderer; they cannot reimplement or omit compiler stages.
pub(crate) fn build_project_artifacts(
    request: ProjectBuildArtifactRequest<'_>,
) -> Result<ProjectArtifactBuild, ProjectArtifactBuildError> {
    build_project_artifacts_inner(request).map_err(ProjectArtifactBuildError)
}

fn build_project_artifacts_inner(
    request: ProjectBuildArtifactRequest<'_>,
) -> Result<ProjectArtifactBuild> {
    let ProjectBuildArtifactRequest {
        project,
        compiled,
        adapters,
        procedures,
        methods,
        entry_module,
        output_root,
        facility,
    } = request;
    let package = project.default_package();
    fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    reset_managed_directory(&output_root.join("modules"), "module")?;

    let program_packages = project.program_packages();
    let products = build_products(&compiled.modules, &program_packages);
    let program_modules = compiled
        .modules
        .iter()
        .filter(|module| program_packages.contains(&module.package))
        .map(|module| &module.module)
        .collect::<Vec<_>>();

    let mut modules = Vec::new();
    for compiled_module in &compiled.modules {
        let source = &compiled_module.source;
        let relative_artifact = PathBuf::from("modules")
            .join(source.module.replace('.', "/"))
            .with_extension("module.json");
        write_frozen_artifact(
            output_root,
            &relative_artifact,
            &pretty_json_bytes(&compiled_module.module)?,
        )?;
        modules.push(BuildModule {
            package: compiled_module.package.clone(),
            module: source.module.clone(),
            source: source.relative_path.clone(),
            artifact: relative_artifact,
        });
    }

    let adapter_bindings = if let Some(path) = facility
        .as_ref()
        .and_then(|planned| planned.adapter_bindings.as_ref())
    {
        Some(relative_artifact_path(
            output_root,
            path,
            "adapter binding",
        )?)
    } else if facility.is_none() {
        write_project_adapter_bindings(package, adapters, output_root)?
    } else {
        None
    };

    let facility_index = facility
        .as_ref()
        .map(|planned| build_facility_index(planned, output_root))
        .transpose()?;
    let compiler = if let Some(planned) = &facility {
        Some(build_compiler_index(planned, output_root)?)
    } else if let Some(entry_module) = entry_module {
        Some(write_unallocated_compiler_frontier(
            &program_modules,
            methods,
            procedures,
            entry_module,
            output_root,
        )?)
    } else {
        None
    };
    let index = BuildIndex {
        schema_version: 8,
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        edition: package.manifest.package.edition.clone(),
        entry: package.manifest.build.entry.clone(),
        members: compiled.members.clone(),
        modules,
        compiler,
        adapter_bindings,
        facility: facility_index,
    };
    write_pretty_json(&output_root.join("package.json"), &index)?;
    fs::write(project.root().join(LOCK_FILE), compiled.lock.to_toml()?).with_context(|| {
        format!(
            "failed to write {}",
            project.root().join(LOCK_FILE).display()
        )
    })?;

    Ok(ProjectArtifactBuild {
        package: index.package,
        version: index.version,
        modules: index.modules.len(),
        output: output_root.to_path_buf(),
        products,
        facility,
    })
}

fn build_products(
    modules: &[crate::CompiledModule],
    program_packages: &[String],
) -> Vec<BuildProduct> {
    modules
        .iter()
        .filter(|module| program_packages.contains(&module.package))
        .flat_map(|module| {
            module.module.declarations.iter().filter_map(|declaration| {
                let CheckedDeclaration::Artifact { artifact, name, .. } = declaration else {
                    return None;
                };
                Some(BuildProduct {
                    package: module.package.clone(),
                    module: module.source.module.clone(),
                    kind: artifact.clone(),
                    name: name.clone(),
                })
            })
        })
        .collect()
}

fn write_project_adapter_bindings(
    package: &LabPackage,
    adapters: &AdapterRegistry,
    output_root: &Path,
) -> Result<Option<PathBuf>> {
    let Some(snapshot) = load_package_inventory(package)? else {
        return Ok(None);
    };
    let Some(bindings) = resolve_package_adapter_bindings(package, &snapshot, adapters)? else {
        return Ok(None);
    };
    let artifact = PathBuf::from("adapter_bindings.json");
    write_frozen_artifact(output_root, &artifact, &pretty_json_bytes(&bindings)?)?;
    Ok(Some(artifact))
}

fn write_unallocated_compiler_frontier(
    modules: &[&CheckedModule],
    methods: &MethodRegistry,
    procedures: &ProcedureCompiler,
    entry_module: &str,
    output_root: &Path,
) -> Result<BuildCompilerIndex> {
    reset_managed_directory(&output_root.join("compiler"), "compiler")?;
    let refined = PortableLairProgram::lower_entry_program(modules, entry_module)
        .context("failed to lower the selected program into Design and Intent LAIR")?
        .refine_methods(methods, procedures)
        .context("failed to refine workflow intent into Method alternatives")?;
    let problem = refined
        .planning_problem()
        .context("failed to project the verified Method graph into a planning problem")?;
    let refined_lair = PathBuf::from("compiler/refined.lair");
    let planning_problem = PathBuf::from("compiler/planning-problem.json");
    write_frozen_artifact(output_root, &refined_lair, refined.ir().as_bytes())?;
    write_frozen_artifact(
        output_root,
        &planning_problem,
        &pretty_json_bytes(&problem)?,
    )?;
    Ok(BuildCompilerIndex {
        refined_lair,
        planning_problem,
        facility_solution: None,
        allocated_lair: None,
        adapter_invocations: None,
    })
}

fn build_facility_index(
    planned: &FacilityArtifactBuild,
    output_root: &Path,
) -> Result<BuildFacilityIndex> {
    Ok(BuildFacilityIndex {
        facility: planned.facility.clone(),
        facility_solution: relative_artifact_path(
            output_root,
            &planned.facility_solution,
            "facility solution",
        )?,
        adapter_invocations: relative_artifact_path(
            output_root,
            &planned.adapter_invocations,
            "adapter invocation",
        )?,
        lowering: relative_artifact_path(output_root, &planned.lowering, "lowering")?,
        execution_plan: relative_artifact_path(
            output_root,
            &planned.execution_plan,
            "execution plan",
        )?,
        bundles: planned
            .bundles
            .iter()
            .map(|path| relative_artifact_path(output_root, path, "asset bundle"))
            .collect::<Result<Vec<_>>>()?,
        protocols: planned
            .protocols
            .iter()
            .map(|path| relative_artifact_path(output_root, path, "automation protocol"))
            .collect::<Result<Vec<_>>>()?,
        documents: planned
            .documents
            .iter()
            .map(|path| relative_artifact_path(output_root, path, "operator document"))
            .collect::<Result<Vec<_>>>()?,
    })
}

fn build_compiler_index(
    planned: &FacilityArtifactBuild,
    output_root: &Path,
) -> Result<BuildCompilerIndex> {
    Ok(BuildCompilerIndex {
        refined_lair: relative_artifact_path(output_root, &planned.refined_lair, "refined LAIR")?,
        planning_problem: relative_artifact_path(
            output_root,
            &planned.planning_problem,
            "planning problem",
        )?,
        facility_solution: Some(relative_artifact_path(
            output_root,
            &planned.facility_solution,
            "facility solution",
        )?),
        allocated_lair: Some(relative_artifact_path(
            output_root,
            &planned.allocated_lair,
            "allocated LAIR",
        )?),
        adapter_invocations: Some(relative_artifact_path(
            output_root,
            &planned.adapter_invocations,
            "adapter invocation",
        )?),
    })
}

fn relative_artifact_path(output_root: &Path, path: &Path, role: &str) -> Result<PathBuf> {
    path.strip_prefix(output_root)
        .map(Path::to_path_buf)
        .with_context(|| {
            format!(
                "{role} artifact {} is outside build output {}",
                path.display(),
                output_root.display()
            )
        })
}

fn reset_managed_directory(path: &Path, role: &str) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    if !metadata.is_dir() {
        bail!(
            "refusing to replace managed {role} output {} because it is not a directory",
            path.display()
        );
    }
    fs::remove_dir_all(path).with_context(|| format!("failed to replace {}", path.display()))
}

/// Materializes the complete immutable planning, lowering, and execution bundle for a facility.
///
/// This is the only filesystem-writing boundary for facility artifacts. All planning evidence,
/// exact adapter inputs, reviewed documents, and execution references are assembled together so a
/// frontend cannot accidentally omit one stage. PDF rendering is an optional presentation hook;
/// the reviewed Typst sources remain in the bundle regardless.
pub(crate) fn build_facility_artifacts(
    request: FacilityArtifactRequest<'_>,
) -> Result<FacilityArtifactBuild, FacilityArtifactError> {
    build_facility_artifacts_inner(request).map_err(FacilityArtifactError)
}

fn build_facility_artifacts_inner(
    request: FacilityArtifactRequest<'_>,
) -> Result<FacilityArtifactBuild> {
    let FacilityArtifactRequest {
        package,
        planning,
        output_root,
        renderer,
    } = request;
    let inventory = &planning.inventory;
    let adapter_bindings = planning.adapter_bindings.as_ref();
    let allocated = &planning.allocated;
    let invocations = &planning.adapter_invocations;
    let problem = planning.problem();
    let solution = planning.solution();
    let refined_ir = &planning.refined_lair;
    let allocated_ir = allocated.ir();
    fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    reset_facility_bundle_directories(output_root)?;

    let refined_lair_artifact = PathBuf::from("compiler/refined.lair");
    let planning_problem_artifact = PathBuf::from("compiler/planning-problem.json");
    let facility_solution_artifact = PathBuf::from("compiler/facility-solution.json");
    let allocated_lair_artifact = PathBuf::from("compiler/allocated.lair");
    let adapter_invocations_artifact = PathBuf::from("compiler/adapter-invocations.json");
    write_frozen_artifact(output_root, &refined_lair_artifact, refined_ir.as_bytes())?;
    let planning_problem_reference = write_frozen_artifact(
        output_root,
        &planning_problem_artifact,
        &pretty_json_bytes(problem)?,
    )?;
    let facility_solution_reference = write_frozen_artifact(
        output_root,
        &facility_solution_artifact,
        &pretty_json_bytes(solution)?,
    )?;
    let allocated_lair_reference = write_frozen_artifact(
        output_root,
        &allocated_lair_artifact,
        allocated_ir.as_bytes(),
    )?;
    if allocated_lair_reference.sha256 != invocations.allocated_lair_sha256 {
        bail!("allocated LAIR changed while projecting adapter invocations");
    }
    let adapter_invocations_reference = write_frozen_artifact(
        output_root,
        &adapter_invocations_artifact,
        &pretty_json_bytes(invocations)?,
    )?;

    let lowered = lower_adapter_invocations(
        package,
        inventory,
        &planning.adapters,
        planning.procedures.contracts(),
        invocations,
        output_root,
        renderer,
    )?;
    let inventory_document = staged_inventory_name(inventory)?;
    let planning_reference = ExecutionPlanningReference {
        problem_sha256: problem.sha256(),
        allocated_lair_sha256: invocations.allocated_lair_sha256.clone(),
        planning_problem: planning_problem_reference,
        facility_solution: facility_solution_reference,
        allocated_lair: allocated_lair_reference,
        adapter_invocations: adapter_invocations_reference,
        methods: invocations
            .allocated
            .methods
            .iter()
            .map(|method| ExecutionMethodSelection {
                choice: method.choice.to_string(),
                source_operation: method.source_operation.to_string(),
                source_intent: serde_json::to_value(&method.source_intent)
                    .expect("typed source Intent serializes infallibly"),
                method: method.method.to_string(),
                tasks: method
                    .tasks
                    .iter()
                    .map(|task| task.id.to_string())
                    .collect(),
            })
            .collect(),
    };
    let mut execution_plan = build_execution_plan_from_invocations(
        invocations,
        planning.procedures.contracts(),
        ExecutionPlanOptions {
            inventory_document: inventory_document.clone(),
            planning: Some(planning_reference),
            reviewed_documents: lowered.reviewed_documents.clone(),
            ..ExecutionPlanOptions::default()
        },
    )
    .context("failed to construct the reviewed execution plan")?;
    stage_execution_inputs(
        package,
        inventory,
        &planning.adapters,
        &mut execution_plan,
        output_root,
    )?;
    execution_plan
        .validate()
        .map_err(|message| anyhow::anyhow!("reviewed execution plan is invalid: {message}"))?;
    let lowering_path = output_root.join("facility_lowering.json");
    let execution_plan_path = output_root.join(EXECUTION_PLAN_FILE);
    write_pretty_json(&lowering_path, &lowered.manifest)?;
    write_pretty_json(&execution_plan_path, &execution_plan)?;
    let adapter_bindings_path = if let Some(bindings) = adapter_bindings {
        let path = output_root.join("adapter_bindings.json");
        write_pretty_json(&path, bindings)?;
        Some(path)
    } else {
        None
    };

    let mut documents = lowered.documents;
    if let Some(run_sheet) = write_manual_run_sheet(package, invocations, output_root, renderer)? {
        documents.push(run_sheet);
    }

    let bundles = lowered
        .manifest
        .routes
        .iter()
        .map(|route| output_root.join(&route.output))
        .collect();
    Ok(FacilityArtifactBuild {
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        output: output_root.to_path_buf(),
        facility: invocations.allocated.facility.clone(),
        selected_methods: invocations.allocated.methods.len(),
        allocated_requirements: invocations
            .allocated
            .methods
            .iter()
            .flat_map(|method| &method.tasks)
            .map(|task| task.requirements.len())
            .sum(),
        adapter_lowerings: lowered.manifest.routes.len(),
        refined_lair: output_root.join(refined_lair_artifact),
        planning_problem: output_root.join(planning_problem_artifact),
        facility_solution: output_root.join(facility_solution_artifact),
        allocated_lair: output_root.join(allocated_lair_artifact),
        adapter_invocations: output_root.join(adapter_invocations_artifact),
        adapter_bindings: adapter_bindings_path,
        lowering: lowering_path,
        execution_plan: execution_plan_path,
        bundles,
        protocols: lowered.protocols,
        documents,
    })
}

/// Derives concrete backend invocations from exact facility allocations.
///
/// A package never selects a device implementation here. Each route exists only because a reachable semantic
/// requirement was allocated to an offering, that offering belongs to an exact Asset, and the
/// Asset has an explicit local adapter binding whose implementation provides lowering.
fn lower_adapter_invocations(
    package: &LabPackage,
    inventory: &InventorySnapshot,
    adapters: &AdapterRegistry,
    contracts: &ProcedureContractRegistry,
    invocation_plan: &AdapterInvocationPlan,
    output_root: &Path,
    renderer: Option<&dyn FacilityDocumentRenderer>,
) -> Result<FacilityLoweringOutput> {
    invocation_plan
        .validate(contracts)
        .context("allocated adapter invocations are invalid")?;
    if invocation_plan.allocated.inventory_sha256 != inventory.source_sha256()
        || invocation_plan.allocated.facility != inventory.facility().as_str()
    {
        bail!("adapter invocations and the selected inventory snapshot do not match");
    }
    let requirements = invocation_plan
        .allocated
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .flat_map(|task| &task.requirements)
        .map(|requirement| (requirement.id.clone(), requirement))
        .collect::<BTreeMap<_, _>>();

    let mut lowerable = Vec::new();
    for invocation in &invocation_plan.invocations {
        let descriptor = adapters
            .descriptors()
            .descriptor(invocation.adapter.driver.as_str())
            .with_context(|| {
                format!(
                    "allocated adapter '{}' is not present in this compiler build",
                    invocation.adapter.driver
                )
            })?;
        let mut procedure_implementations = BTreeSet::new();
        let mut supports_lowering = true;
        for requirement_id in &invocation.requirements {
            let requirement = requirements
                .get(requirement_id)
                .expect("validated invocation Requirement exists");
            let implementation_id =
                requirement
                    .procedure_implementation
                    .as_ref()
                    .with_context(|| {
                        format!(
                            "adapter-bound requirement '{}' has no Procedure implementation",
                            requirement.id
                        )
                    })?;
            procedure_implementations.insert(implementation_id.clone());
            let implementation = descriptor
                .procedure_implementations
                .iter()
                .find(|implementation| &implementation.id == implementation_id)
                .with_context(|| {
                    format!(
                        "allocated Procedure implementation '{}' is not provided by adapter '{}' in this compiler build",
                        implementation_id, invocation.adapter.driver
                    )
                })?;
            supports_lowering &= implementation.services.lowering;
        }
        if !supports_lowering {
            continue;
        }
        let mut lowered_requirements = invocation
            .requirements
            .iter()
            .map(|requirement_id| {
                let requirement = requirements
                    .get(requirement_id)
                    .expect("validated invocation Requirement exists");
                FacilityLoweredRequirement {
                    requirement_instance: requirement.id.to_string(),
                    capability_kind: requirement.capability_kind.to_string(),
                    offering: requirement.offering.clone(),
                }
            })
            .collect::<Vec<_>>();
        lowered_requirements
            .sort_by(|left, right| left.requirement_instance.cmp(&right.requirement_instance));
        lowerable.push(LowerableInvocation {
            invocation: invocation.clone(),
            procedure_implementations,
            requirements: lowered_requirements,
        });
    }
    let mut lowering_directories =
        facility_lowering_directories(lowerable.iter().map(|lowering| {
            (
                lowering.invocation.asset.as_str(),
                lowering.invocation.adapter.driver.as_str(),
            )
        }));

    let mut routes = Vec::new();
    let mut protocols = Vec::new();
    let mut documents = Vec::new();
    let mut reviewed_documents = BTreeMap::new();
    if !lowerable.is_empty() {
        for lowering in lowerable {
            let invocation = lowering.invocation;
            let asset = invocation.asset.clone();
            let driver = invocation.adapter.driver.clone();
            let source_profile_path = invocation.adapter.profile_path.clone();
            let profile_sha256 = invocation.adapter.profile_sha256.clone();
            let source = package.root.join(&source_profile_path);
            let profile = load_and_validate_adapter_profile(adapters, &driver, &source)
                .with_context(|| {
                    format!(
                        "failed to load operational profile for Asset '{}' adapter '{}'",
                        asset, driver
                    )
                })?;
            if profile.sha256 != profile_sha256 {
                bail!(
                    "operational profile {} changed after adapter allocation for Asset '{}'",
                    source.display(),
                    asset
                );
            }
            let lowered = adapters
                .lower_invocation(&profile, invocation_plan, &invocation, contracts)
                .with_context(|| {
                    format!(
                        "failed to lower invocation '{}' for Asset '{}' through adapter '{}'",
                        invocation.id, asset, driver
                    )
                })?;
            let invocation_documents = lowered
                .documents
                .into_iter()
                .map(|document| {
                    (
                        document.path,
                        (
                            document
                                .requirements
                                .into_iter()
                                .map(|requirement| requirement.to_string())
                                .collect(),
                            document.format,
                        ),
                    )
                })
                .collect();
            let relative_output = lowering_directories
                .remove(&(asset.clone(), driver.clone()))
                .expect("every lowerable Asset and adapter has an output directory");
            let written = write_facility_artifacts(
                &lowered.artifacts,
                output_root,
                &relative_output,
                &invocation_documents,
                renderer,
            )?;
            protocols.extend(written.protocols);
            documents.extend(written.documents);
            for (requirement, document) in written.reviewed_documents {
                if reviewed_documents
                    .insert(requirement.clone(), document)
                    .is_some()
                {
                    bail!("several adapter routes implement requirement '{requirement}'");
                }
            }
            routes.push(FacilityLoweringRoute {
                id: facility_lowering_id(&asset, &driver),
                asset,
                driver: driver.clone(),
                procedure_implementations: lowering.procedure_implementations,
                profile_path: staged_adapter_profile_path(&driver, &profile_sha256),
                profile_sha256,
                requirements: lowering.requirements,
                output: relative_output,
                artifacts: written.artifacts,
            });
        }
    }
    routes.sort_by(|left, right| (&left.asset, &left.driver).cmp(&(&right.asset, &right.driver)));
    protocols.sort();
    documents.sort();
    Ok(FacilityLoweringOutput {
        manifest: FacilityLoweringManifest {
            schema_version: FACILITY_LOWERING_SCHEMA_VERSION.to_owned(),
            inventory_sha256: invocation_plan.allocated.inventory_sha256.clone(),
            facility: invocation_plan.allocated.facility.clone(),
            routes,
        },
        protocols,
        documents,
        reviewed_documents,
    })
}

fn facility_lowering_directories<'a>(
    routes: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<(String, String), PathBuf> {
    let routes = routes
        .into_iter()
        .map(|(asset, driver)| (asset, driver, facility_asset_name(asset)))
        .collect::<Vec<_>>();
    let mut name_counts = BTreeMap::<String, usize>::new();
    for (_, _, name) in &routes {
        *name_counts.entry(name.clone()).or_default() += 1;
    }
    routes
        .into_iter()
        .map(|(asset, driver, name)| {
            let directory = if name_counts[&name] == 1 {
                name
            } else {
                let identity = format!("{asset}\0{driver}");
                format!("{name}-{}", &sha256_hex(identity.as_bytes())[..8])
            };
            (
                (asset.to_owned(), driver.to_owned()),
                PathBuf::from("assets").join(directory),
            )
        })
        .collect()
}

fn facility_asset_name(asset: &str) -> String {
    let raw_name = asset
        .rsplit(['/', '#'])
        .find(|segment| !segment.is_empty())
        .unwrap_or("asset");
    let mut name = String::new();
    for character in raw_name.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_') {
            name.push(character);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "asset".to_owned()
    } else {
        name.to_owned()
    }
}

fn facility_lowering_id(asset: &str, driver: &str) -> String {
    let asset_hash = sha256_hex(asset.as_bytes());
    format!("{}-{}", driver.replace('.', "-"), &asset_hash[..12])
}

fn write_facility_artifacts(
    bundle: &ArtifactBundle,
    output_root: &Path,
    relative_output: &Path,
    invocation_documents: &BTreeMap<String, (Vec<String>, String)>,
    renderer: Option<&dyn FacilityDocumentRenderer>,
) -> Result<WrittenFacilityArtifacts> {
    let route_root = output_root.join(relative_output);
    if let Some(path) = invocation_documents
        .keys()
        .find(|path| bundle.get(path).is_none())
    {
        bail!("adapter names missing reviewed invocation artifact '{path}'");
    }
    let mut artifacts = Vec::new();
    let mut protocols = Vec::new();
    let mut documents = Vec::new();
    let mut reviewed_documents = BTreeMap::new();
    let mut typst_sources = Vec::new();
    for artifact in bundle.iter() {
        let relative_path = PathBuf::from(artifact.path());
        let path = route_root.join(&relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, artifact.contents())
            .with_context(|| format!("failed to write {}", path.display()))?;
        let invocation_document = invocation_documents.get(artifact.path());
        let role = if invocation_document.is_some() {
            protocols.push(path);
            FacilityLoweredArtifactRole::AutomationProtocol
        } else {
            FacilityLoweredArtifactRole::Support
        };
        if artifact.media_type() == "text/x-typst" && is_typeset_document(artifact.path()) {
            typst_sources.push(relative_path.clone());
        }
        let sha256 = sha256_hex(artifact.contents());
        let format = invocation_document.map(|(_, format)| format.clone());
        if let Some((requirements, format)) = invocation_document {
            let reviewed = ReviewedRunDocument {
                path: relative_output
                    .join(&relative_path)
                    .to_str()
                    .context("reviewed invocation document paths must be UTF-8")?
                    .to_owned(),
                format: format.clone(),
                sha256: sha256.clone(),
            };
            for requirement in requirements {
                if reviewed_documents
                    .insert(requirement.clone(), reviewed.clone())
                    .is_some()
                {
                    bail!("several adapter artifacts implement requirement '{requirement}'");
                }
            }
        }
        artifacts.push(FacilityLoweredArtifact {
            path: relative_path,
            media_type: artifact.media_type().to_owned(),
            sha256,
            role,
            format,
        });
    }

    typst_sources.sort();
    for source in typst_sources {
        let Some(renderer) = renderer else {
            documents.push(route_root.join(source));
            continue;
        };
        let source_text = source
            .to_str()
            .context("a generated Typst source path must be UTF-8")?;
        let pdf_bytes = renderer
            .render_typst(&route_root, source_text)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("failed to typeset {}", source.display()))?;
        let pdf_relative = source.with_extension("pdf");
        let pdf_path = route_root.join(&pdf_relative);
        fs::write(&pdf_path, &pdf_bytes)
            .with_context(|| format!("failed to write {}", pdf_path.display()))?;
        documents.push(pdf_path);
        artifacts.push(FacilityLoweredArtifact {
            path: pdf_relative,
            media_type: "application/pdf".to_owned(),
            sha256: sha256_hex(&pdf_bytes),
            role: FacilityLoweredArtifactRole::OperatorDocument,
            format: None,
        });
    }
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(WrittenFacilityArtifacts {
        artifacts,
        protocols,
        documents,
        reviewed_documents,
    })
}

/// Typeset the operator run sheet for the plan's manual steps when the frontend supplies a
/// renderer. Without one, the complete Typst source remains the reported operator document.
fn write_manual_run_sheet(
    package: &LabPackage,
    invocations: &AdapterInvocationPlan,
    output_root: &Path,
    renderer: Option<&dyn FacilityDocumentRenderer>,
) -> Result<Option<PathBuf>> {
    let steps = lab_facility::manual_run_steps(invocations);
    if steps.is_empty() {
        return Ok(None);
    }
    let source = lab_adapters::run_sheet::render_run_sheet(&lab_adapters::run_sheet::RunSheet {
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        facility: invocations.allocated.facility.clone(),
        steps,
    });
    let directory = output_root.join("documents");
    fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    fs::write(
        directory.join(lab_adapters::run_sheet::RUN_SHEET_STYLE_PATH),
        lab_adapters::run_sheet::RUN_SHEET_STYLE,
    )
    .context("failed to write the run-sheet style sheet")?;
    let source_path = directory.join("manual_protocol.typ");
    fs::write(&source_path, &source)
        .with_context(|| format!("failed to write {}", source_path.display()))?;
    let Some(renderer) = renderer else {
        return Ok(Some(source_path));
    };
    let pdf = renderer
        .render_typst(&directory, "manual_protocol.typ")
        .map_err(anyhow::Error::msg)
        .context("failed to typeset the manual run sheet")?;
    let pdf_path = directory.join("manual_protocol.pdf");
    fs::write(&pdf_path, &pdf)
        .with_context(|| format!("failed to write {}", pdf_path.display()))?;
    Ok(Some(pdf_path))
}

/// Replace only compiler-owned adapter bundle directories.
fn reset_facility_bundle_directories(output_root: &Path) -> Result<()> {
    for name in ["assets", "adapters", "compiler"] {
        let path = output_root.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
            }
        };
        if !metadata.is_dir() {
            bail!(
                "refusing to replace managed facility output {} because it is not a directory",
                path.display()
            );
        }
        fs::remove_dir_all(&path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
    }
    Ok(())
}

fn staged_inventory_name(inventory: &InventorySnapshot) -> Result<String> {
    let extension = inventory
        .source_path()
        .extension()
        .and_then(|extension| extension.to_str())
        .context("the inventory document needs a UTF-8 file extension")?;
    Ok(format!("inventory-source.{extension}"))
}

/// Copies every mutable package input named by a reviewed plan into its artifact directory.
fn stage_execution_inputs(
    package: &LabPackage,
    inventory: &InventorySnapshot,
    adapters: &AdapterRegistry,
    plan: &mut ExecutionPlanDocument,
    output_root: &Path,
) -> Result<()> {
    let inventory_bytes = fs::read(inventory.source_path()).with_context(|| {
        format!(
            "failed to re-read inventory source {}",
            inventory.source_path().display()
        )
    })?;
    let observed_inventory_hash = sha256_hex(&inventory_bytes);
    if observed_inventory_hash != inventory.source_sha256() {
        bail!(
            "inventory source {} changed after validation; plan again from a stable source",
            inventory.source_path().display()
        );
    }
    let inventory_path = output_root.join(&plan.inventory.document);
    fs::write(&inventory_path, inventory_bytes)
        .with_context(|| format!("failed to stage {}", inventory_path.display()))?;

    let canonical_root = fs::canonicalize(&package.root)
        .with_context(|| format!("failed to resolve package root {}", package.root.display()))?;
    let adapters_directory = output_root.join("adapters");
    for requirement in &mut plan.requirements {
        let Some(adapter) = requirement.adapter.as_mut() else {
            continue;
        };
        let source =
            fs::canonicalize(canonical_root.join(&adapter.profile_path)).with_context(|| {
                format!(
                    "failed to resolve adapter profile {} for '{}'",
                    adapter.profile_path, requirement.requirement_instance
                )
            })?;
        if !source.starts_with(&canonical_root) {
            bail!(
                "adapter profile '{}' for '{}' resolves outside package '{}'",
                adapter.profile_path,
                requirement.requirement_instance,
                package.manifest.package.name
            );
        }
        let profile = load_and_validate_adapter_profile(adapters, &adapter.driver, &source)?;
        if profile.sha256 != adapter.profile_sha256 {
            bail!(
                "adapter profile {} changed after allocation for '{}'",
                source.display(),
                requirement.requirement_instance
            );
        }
        fs::create_dir_all(&adapters_directory)
            .with_context(|| format!("failed to create {}", adapters_directory.display()))?;
        let relative = staged_adapter_profile_path(&adapter.driver, &adapter.profile_sha256);
        let destination = output_root.join(&relative);
        fs::write(&destination, profile.canonical_toml.as_bytes())
            .with_context(|| format!("failed to stage {}", destination.display()))?;
        if sha256_hex(profile.canonical_toml.as_bytes()) != adapter.profile_sha256 {
            bail!(
                "canonical adapter profile for '{}' does not match its frozen digest",
                requirement.requirement_instance
            );
        }
        adapter.profile_path = relative.to_string_lossy().into_owned();
    }
    plan.validate()
        .map_err(|message| anyhow::anyhow!("staged execution plan is invalid: {message}"))
}

fn write_pretty_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, pretty_json_bytes(value)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn pretty_json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut json = serde_json::to_vec_pretty(value)?;
    json.push(b'\n');
    Ok(json)
}

fn write_frozen_artifact(
    output_root: &Path,
    relative_path: &Path,
    bytes: &[u8],
) -> Result<ExecutionPlanningArtifact> {
    let path = output_root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(ExecutionPlanningArtifact {
        path: relative_path
            .to_str()
            .context("compiler artifact paths must be UTF-8")?
            .to_owned(),
        sha256: sha256_hex(bytes),
    })
}

fn staged_adapter_profile_path(driver: &str, profile_sha256: &str) -> PathBuf {
    PathBuf::from("adapters").join(format!("{driver}-{}.toml", &profile_sha256[..12]))
}

fn is_typeset_document(path: &str) -> bool {
    !path.ends_with("lab-style.typ")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn load_and_validate_adapter_profile(
    adapters: &AdapterRegistry,
    driver: &str,
    path: &Path,
) -> Result<ValidatedAdapterProfile> {
    if !path.is_file() {
        bail!("no adapter profile at {}", path.display());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("an adapter profile file needs a UTF-8 file name")?;
    adapters
        .validate_profile(driver, name, &contents)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unique_asset_gets_a_short_readable_directory() {
        let directories = facility_lowering_directories([(
            "https://example.org/facility/Opentrons_OT2",
            "opentrons.ot2",
        )]);

        assert_eq!(
            directories[&(
                "https://example.org/facility/Opentrons_OT2".to_owned(),
                "opentrons.ot2".to_owned(),
            )],
            PathBuf::from("assets/opentrons_ot2")
        );
    }

    #[test]
    fn colliding_asset_names_get_only_the_hash_they_need() {
        let directories = facility_lowering_directories([
            ("https://example.org/room-a/reader", "reader.alpha"),
            ("https://example.org/room-b/reader", "reader.beta"),
        ]);
        let first = &directories[&(
            "https://example.org/room-a/reader".to_owned(),
            "reader.alpha".to_owned(),
        )];
        let second = &directories[&(
            "https://example.org/room-b/reader".to_owned(),
            "reader.beta".to_owned(),
        )];

        assert_ne!(first, second);
        assert!(first.to_string_lossy().starts_with("assets/reader-"));
        assert!(second.to_string_lossy().starts_with("assets/reader-"));
    }
}
