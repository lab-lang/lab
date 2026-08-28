use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::planning::{
    CapabilityRequirements, ExecutionPlanOptions, FacilityAllocation, build_execution_plan,
    reviewed_lowering_bundles,
};
use lab_compiler::{
    CheckedDeclaration, DiagnosticSeverity, SourceId, analyze_module, render_diagnostic,
};
use lab_inventory::InventorySnapshot;
use lab_package::{LabPackage, PackageManifest};
use lab_project::{CompiledModule, CompiledProject, LOCK_FILE, LabProject};
use lab_runfmt::{EXECUTION_PLAN_FILE, ExecutionPlanDocument};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::Output;

pub(crate) fn new_project(path: PathBuf, name: Option<String>, output: &Output) -> Result<()> {
    if path.exists() && fs::read_dir(&path)?.next().is_some() {
        bail!("{} already exists and is not empty", path.display());
    }
    let package_name = name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("lab-project")
            .to_owned()
    });
    validate_package_name(&package_name)?;

    let programs = path.join("src").join("programs");
    fs::create_dir_all(&programs)
        .with_context(|| format!("failed to create {}", programs.display()))?;
    let manifest = format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[build]\nentry = \"src/programs/main.lab\"\n"
    );
    write_new(&path.join("lab.toml"), &manifest)?;
    write_new(
        &programs.join("main.lab"),
        r#"use std.bio.build
use std.bio.designs

plasmid starter:
  sequence = dna("ATGCGTACGTTAGCTA")
  require topology == circular
  accept sequence == design.sequence

workflow main() -> Material<Plasmid>:
  product <- realize starter
  return product
"#,
    )?;
    write_new(&path.join(".gitignore"), ".lab/\n")?;

    output.success(
        "created",
        ProjectCreated {
            package: package_name,
            root: path.clone(),
            entry: PathBuf::from("src/programs/main.lab"),
        },
        format!(
            "Created Lab project in {}\n  Next: cd {} && lab check",
            path.display(),
            path.display()
        ),
    )
}

pub(crate) fn check(path: PathBuf, output: &Output) -> Result<()> {
    if path.is_file() && path.extension().is_some_and(|extension| extension == "lab") {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        // A single file is analyzed rather than compiled, so a failure arrives
        // as a diagnostic with source ranges instead of a byte offset in a
        // message. Each one is rendered against the source; the returned error
        // is only the summary, so the excerpts are not printed inside it.
        let analysis = analyze_module(SourceId::new(path.display().to_string()), &text);
        if !analysis.is_valid() {
            for diagnostic in &analysis.diagnostics {
                eprintln!("{}\n", render_diagnostic(&text, diagnostic));
            }
            let errors = analysis
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                .count();
            bail!(
                "could not check {} ({errors} error{})",
                path.display(),
                if errors == 1 { "" } else { "s" }
            );
        }
        return output.success(
            "checked",
            FileChecked {
                source: path.clone(),
            },
            format!("Checked {}", path.display()),
        );
    }

    let project = LabProject::discover(&path)
        .with_context(|| format!("failed to load project from {}", path.display()))?;
    validate_project_inventories(&project)?;
    let compiled = project.compile()?;
    let package = project.default_package();
    output.success(
        "checked",
        PackageChecked {
            package: package.manifest.package.name.clone(),
            version: package.manifest.package.version.clone(),
            members: compiled.members.clone(),
            modules: compiled.modules.len(),
        },
        format!(
            "Checked {} {} ({} modules)",
            package.manifest.package.name,
            package.manifest.package.version,
            compiled.modules.len()
        ),
    )
}

pub(crate) fn build(path: PathBuf, out_dir: Option<PathBuf>, output: &Output) -> Result<()> {
    let project = LabProject::discover(&path)
        .with_context(|| format!("failed to load project from {}", path.display()))?;
    validate_project_inventories(&project)?;
    let compiled = project.compile()?;
    let package = project.default_package();
    let project_root = project.root().to_path_buf();
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => project_root.join(path),
        None => project_root.join(".lab").join("build"),
    };
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let program_packages = project.program_packages();
    let products = build_products(&compiled.modules, &program_packages);
    let program_modules = compiled
        .modules
        .iter()
        .filter(|module| program_packages.contains(&module.package))
        .map(|module| &module.module)
        .collect::<Vec<_>>();
    let capability_requirements = CapabilityRequirements::extract(&program_modules)
        .context("failed to derive workflow capability requirements")?;
    let capability_requirements_artifact = PathBuf::from("capability_requirements.json");
    let capability_requirements_path = output_root.join(&capability_requirements_artifact);
    let mut capability_requirements_json = serde_json::to_string_pretty(&capability_requirements)?;
    capability_requirements_json.push('\n');
    fs::write(&capability_requirements_path, capability_requirements_json)
        .with_context(|| format!("failed to write {}", capability_requirements_path.display()))?;

    let capability_instances_artifact = if let Some(entry) = package.entry_source() {
        let instances = capability_requirements
            .instantiate_reachable(&program_modules, &entry.module, "main")
            .context("failed to instantiate reachable workflow capability requirements")?;
        let artifact = PathBuf::from("capability_instances.json");
        let path = output_root.join(&artifact);
        let mut json = serde_json::to_string_pretty(&instances)?;
        json.push('\n');
        fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
        Some(artifact)
    } else {
        None
    };

    let adapter_bindings_artifact = if let Some(snapshot) = package_inventory_snapshot(package)? {
        if let Some(bindings) = crate::adapters::resolve_package_bindings(package, &snapshot)? {
            let artifact = PathBuf::from("adapter_bindings.json");
            let path = output_root.join(&artifact);
            let mut json = serde_json::to_string_pretty(&bindings)?;
            json.push('\n');
            fs::write(&path, json)
                .with_context(|| format!("failed to write {}", path.display()))?;
            Some(artifact)
        } else {
            None
        }
    } else {
        None
    };

    let mut artifacts = Vec::new();
    for compiled_module in &compiled.modules {
        let source = &compiled_module.source;
        let module = &compiled_module.module;
        let relative_artifact = PathBuf::from("modules")
            .join(source.module.replace('.', "/"))
            .with_extension("module.json");
        let artifact_path = output_root.join(&relative_artifact);
        if let Some(parent) = artifact_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut json = serde_json::to_string_pretty(module)?;
        json.push('\n');
        fs::write(&artifact_path, json)
            .with_context(|| format!("failed to write {}", artifact_path.display()))?;
        artifacts.push(BuildModule {
            package: compiled_module.package.clone(),
            module: source.module.clone(),
            source: source.relative_path.clone(),
            artifact: relative_artifact,
        });
    }

    let facility =
        if package.manifest.inventory.document.is_some() && package.entry_source().is_some() {
            Some(write_facility_plan(&project, &compiled, &output_root)?)
        } else {
            None
        };
    let facility_index = facility
        .as_ref()
        .map(|planned| build_facility_index(planned, &output_root))
        .transpose()?;
    let index = BuildIndex {
        schema_version: 6,
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        edition: package.manifest.package.edition.clone(),
        entry: package.manifest.build.entry.clone(),
        members: compiled.members.clone(),
        modules: artifacts,
        capability_requirements: capability_requirements_artifact,
        capability_instances: capability_instances_artifact,
        adapter_bindings: adapter_bindings_artifact,
        facility: facility_index,
    };
    let index_path = output_root.join("package.json");
    let mut json = serde_json::to_string_pretty(&index)?;
    json.push('\n');
    fs::write(&index_path, json)
        .with_context(|| format!("failed to write {}", index_path.display()))?;
    let lock_path = project_root.join(LOCK_FILE);
    let lock = compiled.lock.to_toml()?;
    fs::write(&lock_path, lock)
        .with_context(|| format!("failed to write {}", lock_path.display()))?;

    let mut human = format!(
        "Built {} {} ({} modules)",
        index.package,
        index.version,
        index.modules.len()
    );
    if products.is_empty() {
        human.push_str("\n\nBuild products: none");
    } else {
        human.push_str("\n\nBuild products:");
        for product in &products {
            human.push_str(&format!("\n  {} {}", product.kind, product.name));
        }
    }
    human.push_str(&format!(
        "\n\nCompiler output: {}",
        human_path(&output_root)
    ));
    if let Some(facility) = &facility {
        human.push_str(&format!(
            "\n\nFacility outputs:\n  Facility: {}\n  Requirements allocated: {}\n  Adapter lowerings: {}\n  Allocation: {}\n  Lowering manifest: {}\n  Reviewed plan: {}",
            facility.facility,
            facility.allocated_requirements,
            facility.adapter_lowerings,
            artifact_path(facility, &facility.allocation),
            artifact_path(facility, &facility.lowering),
            artifact_path(facility, &facility.execution_plan)
        ));
        append_facility_artifacts(&mut human, facility);
    }
    output.success(
        "built",
        BuildCompleted {
            package: index.package.clone(),
            version: index.version.clone(),
            modules: index.modules.len(),
            output: output_root.clone(),
            products,
            facility,
        },
        human,
    )
}

/// Every biological artifact that the compiled program declares with `build`.
/// Bought declarations lower to catalog entries instead and therefore never
/// appear in this summary.
fn build_products(modules: &[CompiledModule], program_packages: &[String]) -> Vec<BuildProduct> {
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

pub(crate) fn plan(path: PathBuf, out_dir: Option<PathBuf>, output: &Output) -> Result<()> {
    let project = LabProject::discover(&path)
        .with_context(|| format!("failed to load project from {}", path.display()))?;
    let compiled = project.compile()?;
    let project_root = project.root();
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => project_root.join(path),
        None => project_root.join(".lab").join("plan"),
    };
    let planned = write_facility_plan(&project, &compiled, &output_root)?;
    let mut human = format!(
        "Planned {} {} against {}\n  Requirements: {}\n  Adapter lowerings: {}\n  Plan output: {}\n  Reviewed plan: {}",
        planned.package,
        planned.version,
        planned.facility,
        planned.allocated_requirements,
        planned.adapter_lowerings,
        human_path(&planned.output),
        artifact_path(&planned, &planned.execution_plan)
    );
    append_facility_artifacts(&mut human, &planned);
    output.success("planned", planned, human)
}

fn write_facility_plan(
    project: &LabProject,
    compiled: &CompiledProject,
    output_root: &Path,
) -> Result<PlanCompleted> {
    let package = project.default_package();
    let entry = package.entry_source().with_context(|| {
        format!(
            "package '{}' is a library with no build.entry; a facility plan needs an exact main workflow",
            package.manifest.package.name
        )
    })?;
    let inventory = package_inventory_snapshot(package)?.with_context(|| {
        format!(
            "package '{}' has no inventory.document; facility planning consumes a validated SBOLInventory document",
            package.manifest.package.name
        )
    })?;
    let program_packages = project.program_packages();
    let modules = compiled
        .modules
        .iter()
        .filter(|module| program_packages.contains(&module.package))
        .map(|module| &module.module)
        .collect::<Vec<_>>();
    let requirements = CapabilityRequirements::extract(&modules)
        .context("failed to derive workflow capability requirements")?;
    let instances = requirements
        .instantiate_reachable(&modules, &entry.module, "main")
        .context("failed to instantiate reachable workflow capability requirements")?;
    let adapter_bindings = crate::adapters::resolve_package_bindings(package, &inventory)?;
    let allocation = FacilityAllocation::allocate(
        &requirements,
        &instances,
        &inventory,
        adapter_bindings.as_ref(),
    )
    .context("failed to allocate reachable requirements across the selected facility")?;
    fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    reset_facility_bundle_directories(output_root)?;

    let lowered = crate::facility_lowering::lower_allocated_adapters(
        package,
        &modules,
        &inventory,
        &allocation,
        adapter_bindings.as_ref(),
        output_root,
    )?;
    let inventory_document = staged_inventory_name(&inventory)?;
    let reviewed_lowerings = reviewed_lowering_bundles(&lowered.manifest)
        .context("failed to freeze allocated adapter lowerings into the reviewed plan")?;
    let mut execution_plan = build_execution_plan(
        &allocation,
        ExecutionPlanOptions {
            inventory_document: inventory_document.clone(),
            ..ExecutionPlanOptions::default()
        },
    )
    .context("failed to construct the reviewed execution plan")?;
    stage_execution_inputs(package, &inventory, &mut execution_plan, output_root)?;
    execution_plan.lowerings = reviewed_lowerings;
    execution_plan
        .validate()
        .map_err(|message| anyhow::anyhow!("reviewed execution plan is invalid: {message}"))?;
    let requirements_path = output_root.join("capability_requirements.json");
    let instances_path = output_root.join("capability_instances.json");
    let allocation_path = output_root.join("facility_allocation.json");
    let lowering_path = output_root.join("facility_lowering.json");
    let execution_plan_path = output_root.join(EXECUTION_PLAN_FILE);
    write_pretty_json(&requirements_path, &requirements)?;
    write_pretty_json(&instances_path, &instances)?;
    write_pretty_json(&allocation_path, &allocation)?;
    write_pretty_json(&lowering_path, &lowered.manifest)?;
    write_pretty_json(&execution_plan_path, &execution_plan)?;
    let adapter_bindings_path = if let Some(bindings) = adapter_bindings.as_ref() {
        let path = output_root.join("adapter_bindings.json");
        write_pretty_json(&path, bindings)?;
        Some(path)
    } else {
        None
    };

    let bundles = lowered
        .manifest
        .routes
        .iter()
        .map(|route| output_root.join(&route.output))
        .collect();
    Ok(PlanCompleted {
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        output: output_root.to_path_buf(),
        facility: allocation.facility.clone(),
        allocated_requirements: allocation.allocations.len(),
        adapter_lowerings: lowered.manifest.routes.len(),
        requirements: requirements_path,
        instances: instances_path,
        adapter_bindings: adapter_bindings_path,
        allocation: allocation_path,
        lowering: lowering_path,
        execution_plan: execution_plan_path,
        bundles,
        protocols: lowered.protocols,
        documents: lowered.documents,
    })
}

fn append_facility_artifacts(human: &mut String, planned: &PlanCompleted) {
    if !planned.bundles.is_empty() {
        human.push_str("\n\nAsset bundles:");
        for bundle in &planned.bundles {
            human.push_str(&format!("\n  {}", artifact_path(planned, bundle)));
        }
    }
    if !planned.protocols.is_empty() {
        human.push_str("\n\nAutomation protocols:");
        for protocol in &planned.protocols {
            human.push_str(&format!("\n  {}", artifact_path(planned, protocol)));
        }
    }
    if !planned.documents.is_empty() {
        human.push_str("\n\nDocuments:");
        for document in &planned.documents {
            human.push_str(&format!("\n  {}", artifact_path(planned, document)));
        }
    }
}

fn artifact_path(planned: &PlanCompleted, path: &Path) -> String {
    path.strip_prefix(&planned.output)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn human_path(path: &Path) -> String {
    let displayed = std::env::current_dir()
        .ok()
        .and_then(|current| path.strip_prefix(current).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf());
    if displayed.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        displayed.display().to_string()
    }
}

/// Replace only compiler-owned adapter bundle directories. The legacy
/// `lowerings/` path is removed during migration so a successful rebuild never
/// leaves an obsolete protocol beside the reviewed `assets/` bundle.
fn reset_facility_bundle_directories(output_root: &Path) -> Result<()> {
    for name in ["assets", "lowerings"] {
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

fn build_facility_index(planned: &PlanCompleted, output_root: &Path) -> Result<BuildFacilityIndex> {
    let relative = |path: &Path| {
        path.strip_prefix(output_root)
            .map(Path::to_path_buf)
            .with_context(|| {
                format!(
                    "facility artifact {} is outside build output {}",
                    path.display(),
                    output_root.display()
                )
            })
    };
    Ok(BuildFacilityIndex {
        facility: planned.facility.clone(),
        allocation: relative(&planned.allocation)?,
        lowering: relative(&planned.lowering)?,
        execution_plan: relative(&planned.execution_plan)?,
        bundles: planned
            .bundles
            .iter()
            .map(|path| relative(path))
            .collect::<Result<Vec<_>>>()?,
        protocols: planned
            .protocols
            .iter()
            .map(|path| relative(path))
            .collect::<Result<Vec<_>>>()?,
        documents: planned
            .documents
            .iter()
            .map(|path| relative(path))
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(crate) fn metadata(path: PathBuf, output: &Output) -> Result<()> {
    let package = load_package(&path)?;
    let metadata = PackageMetadataOutput {
        root: package.root.clone(),
        manifest: package.manifest.clone(),
        modules: package
            .sources
            .iter()
            .map(|source| SourceMetadata {
                module: source.module.clone(),
                source: source.relative_path.clone(),
            })
            .collect(),
    };
    let human = format!(
        "{} {}\n{}",
        metadata.manifest.package.name,
        metadata.manifest.package.version,
        metadata
            .modules
            .iter()
            .map(|module| format!("  {}  {}", module.module, module.source.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    output.success("metadata", metadata, human)
}

fn load_package(path: &Path) -> Result<LabPackage> {
    LabPackage::discover(path)
        .with_context(|| format!("failed to load package from {}", path.display()))
}

fn validate_project_inventories(project: &LabProject) -> Result<()> {
    for package in project.member_packages() {
        let Some(snapshot) = package_inventory_snapshot(package)? else {
            continue;
        };
        crate::adapters::resolve_package_bindings(package, &snapshot)?;
    }
    Ok(())
}

fn package_inventory_snapshot(package: &LabPackage) -> Result<Option<InventorySnapshot>> {
    let inventory = &package.manifest.inventory;
    let Some(document) = inventory.document.as_ref() else {
        return Ok(None);
    };
    InventorySnapshot::load(&package.root, document, inventory.facility.as_deref())
        .map(Some)
        .with_context(|| {
            format!(
                "failed to load inventory for package '{}'",
                package.manifest.package.name
            )
        })
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
/// The resulting paths and digests are therefore sufficient for runtime preflight and provenance.
fn stage_execution_inputs(
    package: &LabPackage,
    inventory: &InventorySnapshot,
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
            "inventory source {} changed after validation; run `lab plan` again from a stable source",
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
        let profile = crate::adapters::load_and_validate(&adapter.driver, &source)?;
        if profile.sha256 != adapter.profile_sha256 {
            bail!(
                "adapter profile {} changed after allocation for '{}'",
                source.display(),
                requirement.requirement_instance
            );
        }
        fs::create_dir_all(&adapters_directory)
            .with_context(|| format!("failed to create {}", adapters_directory.display()))?;
        let relative = crate::facility_lowering::staged_adapter_profile_path(
            &adapter.driver,
            &adapter.profile_sha256,
        );
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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_package_name(name: &str) -> Result<()> {
    let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
    let parsed = PackageManifest::parse(&manifest)?;
    if parsed.package.name != name {
        bail!("invalid package name '{name}'");
    }
    let mut characters = name.chars();
    if !characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        || !characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        bail!("invalid package name '{name}'");
    }
    Ok(())
}

fn write_new(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        bail!("refusing to overwrite {}", path.display());
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn write_pretty_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

#[derive(Serialize)]
struct ProjectCreated {
    package: String,
    root: PathBuf,
    entry: PathBuf,
}

#[derive(Serialize)]
struct FileChecked {
    source: PathBuf,
}

#[derive(Serialize)]
struct PackageChecked {
    package: String,
    version: String,
    members: Vec<String>,
    modules: usize,
}

#[derive(Serialize)]
struct BuildCompleted {
    package: String,
    version: String,
    modules: usize,
    output: PathBuf,
    products: Vec<BuildProduct>,
    #[serde(skip_serializing_if = "Option::is_none")]
    facility: Option<PlanCompleted>,
}

#[derive(Serialize)]
struct BuildProduct {
    package: String,
    module: String,
    kind: String,
    name: String,
}

#[derive(Serialize)]
struct PlanCompleted {
    package: String,
    version: String,
    output: PathBuf,
    facility: String,
    allocated_requirements: usize,
    adapter_lowerings: usize,
    requirements: PathBuf,
    instances: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter_bindings: Option<PathBuf>,
    allocation: PathBuf,
    lowering: PathBuf,
    execution_plan: PathBuf,
    bundles: Vec<PathBuf>,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
}

#[derive(Serialize)]
struct PackageMetadataOutput {
    root: PathBuf,
    manifest: PackageManifest,
    modules: Vec<SourceMetadata>,
}

#[derive(Serialize)]
struct SourceMetadata {
    module: String,
    source: PathBuf,
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
    capability_requirements: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    capability_instances: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter_bindings: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    facility: Option<BuildFacilityIndex>,
}

#[derive(Serialize)]
struct BuildFacilityIndex {
    facility: String,
    allocation: PathBuf,
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
