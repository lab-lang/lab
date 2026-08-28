use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::hamilton::star::StarTargetProfile;
use lab_compiler::backend::{TargetProfile, parse_target_profile};
use lab_compiler::planning::{
    BuildInventory, CapabilityRequirements, ExecutionPlanOptions, FacilityAllocation,
    build_execution_plan,
};
use lab_compiler::{
    DiagnosticSeverity, PortableLairProgram, SourceId, analyze_module, render_diagnostic,
};
use lab_inventory::InventorySnapshot;
use lab_package::{LabPackage, PackageManifest};
use lab_project::{CompiledProject, LOCK_FILE, LabProject};
use lab_runfmt::EXECUTION_PLAN_FILE;
use serde::Serialize;

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

pub(crate) fn build(
    path: PathBuf,
    out_dir: Option<PathBuf>,
    target: Option<String>,
    no_target: bool,
    output: &Output,
) -> Result<()> {
    let project = LabProject::discover(&path)
        .with_context(|| format!("failed to load project from {}", path.display()))?;
    validate_project_inventories(&project)?;
    let compiled = project.compile()?;
    let package = project.default_package();
    // A named target wins over the manifest's default, and `--no-target` asks
    // for portable module IR alone.
    let target = if no_target {
        None
    } else {
        target.or_else(|| package.manifest.build.target.clone())
    };
    let project_root = project.root().to_path_buf();
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => project_root.join(path),
        None => project_root.join(".lab").join("build"),
    };
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let program_packages = project.program_packages();
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

    let index = BuildIndex {
        schema_version: 5,
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        edition: package.manifest.package.edition.clone(),
        entry: package.manifest.build.entry.clone(),
        members: compiled.members.clone(),
        modules: artifacts,
        capability_requirements: capability_requirements_artifact,
        capability_instances: capability_instances_artifact,
        adapter_bindings: adapter_bindings_artifact,
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

    let built = match &target {
        Some(target) => Some(build_for_target(
            &project,
            &compiled,
            &project_root,
            &output_root,
            target,
        )?),
        None => None,
    };

    let mut human = format!(
        "Built {} {} ({} modules)\n  Artifacts: {}",
        index.package,
        index.version,
        index.modules.len(),
        output_root.display()
    );
    if let Some(built) = &built {
        human.push_str(&format!(
            "\n  Target {}: {}",
            target.as_deref().unwrap_or_default(),
            built.directory.display()
        ));
        // Name every runnable protocol, so the path can go straight into a
        // device application without hunting through the output directory.
        if !built.protocols.is_empty() {
            human.push_str("\n\nAutomation protocols:");
            for protocol in &built.protocols {
                human.push_str(&format!("\n  {}", protocol.display()));
            }
        }
        if !built.documents.is_empty() {
            human.push_str("\n\nDocuments:");
            for document in &built.documents {
                human.push_str(&format!("\n  {}", document.display()));
            }
        }
    }
    let (target_output, protocols, documents) = match built {
        Some(built) => (Some(built.directory), built.protocols, built.documents),
        None => (None, Vec::new(), Vec::new()),
    };
    output.success(
        "built",
        BuildCompleted {
            package: index.package.clone(),
            version: index.version.clone(),
            modules: index.modules.len(),
            output: output_root.clone(),
            target,
            target_output,
            protocols,
            documents,
        },
        human,
    )
}

pub(crate) fn plan(path: PathBuf, out_dir: Option<PathBuf>, output: &Output) -> Result<()> {
    let project = LabProject::discover(&path)
        .with_context(|| format!("failed to load project from {}", path.display()))?;
    let compiled = project.compile()?;
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
    let execution_plan = build_execution_plan(&allocation, ExecutionPlanOptions::default())
        .context("failed to construct the reviewed execution plan")?;

    let project_root = project.root();
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => project_root.join(path),
        None => project_root.join(".lab").join("plan"),
    };
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    let requirements_path = output_root.join("capability_requirements.json");
    let instances_path = output_root.join("capability_instances.json");
    let allocation_path = output_root.join("facility_allocation.json");
    let execution_plan_path = output_root.join(EXECUTION_PLAN_FILE);
    write_pretty_json(&requirements_path, &requirements)?;
    write_pretty_json(&instances_path, &instances)?;
    write_pretty_json(&allocation_path, &allocation)?;
    write_pretty_json(&execution_plan_path, &execution_plan)?;
    let adapter_bindings_path = if let Some(bindings) = adapter_bindings.as_ref() {
        let path = output_root.join("adapter_bindings.json");
        write_pretty_json(&path, bindings)?;
        Some(path)
    } else {
        None
    };

    let human = format!(
        "Planned {} {} against {}\n  Requirements: {}\n  Reviewed plan: {}",
        package.manifest.package.name,
        package.manifest.package.version,
        allocation.facility,
        allocation.allocations.len(),
        execution_plan_path.display()
    );
    output.success(
        "planned",
        PlanCompleted {
            package: package.manifest.package.name.clone(),
            version: package.manifest.package.version.clone(),
            output: output_root,
            requirements: requirements_path,
            instances: instances_path,
            adapter_bindings: adapter_bindings_path,
            allocation: allocation_path,
            execution_plan: execution_plan_path,
        },
        human,
    )
}

/// What a target build produced: its package directory, every protocol a
/// device application can open, and the typeset operator documents.
struct TargetBuild {
    directory: PathBuf,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
}

/// A generated artifact is an automation protocol when it follows the emitters'
/// naming convention, whatever format the backend writes.
fn is_automation_protocol(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with("_protocol.py")
                || name.ends_with("_protocol.json")
                || name.ends_with(".star.json")
                || name.ends_with(".odtc.json")
                || name.ends_with(".read.json")
                || name == "plan.workcell.json"
        })
}

/// Lower the program the default member forms, together with everything it
/// depends on, and hand the verified Protocol to the named target's backend.
fn build_for_target(
    project: &LabProject,
    compiled: &CompiledProject,
    project_root: &Path,
    output_root: &Path,
    target: &str,
) -> Result<TargetBuild> {
    let profile_path = project_root.join("targets").join(format!("{target}.toml"));
    let profile = if profile_path.is_file() {
        let contents = fs::read_to_string(&profile_path)
            .with_context(|| format!("failed to read {}", profile_path.display()))?;
        parse_target_profile(target, &contents)
            .with_context(|| format!("failed to load target profile {}", profile_path.display()))?
    } else {
        bail!(
            "no target profile at {}; a target is a TOML file under 'targets/'",
            profile_path.display()
        )
    };

    let program_packages = project.program_packages();
    let modules = compiled
        .modules
        .iter()
        .filter(|module| program_packages.contains(&module.package))
        .map(|module| &module.module)
        .collect::<Vec<_>>();
    let lair = PortableLairProgram::lower_program(&modules)
        .context("failed to lower the program for a target build")?;
    let protocol = lair
        .select_protocol()
        .context("failed to select a concrete protocol for a target build")?;

    let package = project.default_package();
    let declared = &package.manifest.inventory;
    let inventory = if let Some(document) = declared.document.as_ref() {
        let snapshot =
            InventorySnapshot::load(&package.root, document, declared.facility.as_deref())
                .with_context(|| {
                    format!(
                        "failed to load inventory for package '{}'",
                        package.manifest.package.name
                    )
                })?;
        let material_lots = snapshot
            .active_material_lots()
            .context("failed to index active SBOLInventory MaterialLots")?;
        let lots_by_component = material_lots
            .components()
            .map(|(component, lots)| {
                (
                    component.as_str().to_owned(),
                    lots.iter().map(|lot| lot.as_str().to_owned()).collect(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        BuildInventory::from_material_lots(
            &modules,
            snapshot.source_sha256(),
            snapshot.facility().as_str(),
            &lots_by_component,
        )
        .context("failed to bind checked designs to SBOLInventory MaterialLots")?
    } else {
        BuildInventory::legacy(
            declared.materials.iter().cloned(),
            declared.artifacts.iter().cloned(),
        )
    };

    let artifacts = match &profile {
        TargetProfile::Ot2(profile) => {
            lab_compiler::backend::opentrons::ot2::compile_dependency_build(
                &protocol, profile, &inventory,
            )
            .with_context(|| format!("failed to compile the {target} build"))?
            .artifacts()
            .clone()
        }
        TargetProfile::Flex(profile) => {
            lab_compiler::backend::opentrons::flex::compile_dependency_build(
                &protocol, profile, &inventory,
            )
            .with_context(|| format!("failed to compile the {target} build"))?
            .artifacts()
            .clone()
        }
        TargetProfile::Star(profile) => {
            lab_compiler::backend::hamilton::star::compile_dependency_build(
                &protocol, profile, &inventory,
            )
            .with_context(|| format!("failed to compile the {target} build"))?
            .artifacts()
            .clone()
        }
        TargetProfile::Workcell(profile) => {
            let station = profile.liquid_handler();
            let station_profile = station
                .profile
                .as_deref()
                .expect("workcell validation requires the liquid handler to name a profile");
            let station_path = project_root
                .join("targets")
                .join(format!("{station_profile}.toml"));
            let station_contents = fs::read_to_string(&station_path).with_context(|| {
                format!(
                    "station '{}' names profile '{station_profile}', but there is no target profile at {}",
                    station.name,
                    station_path.display()
                )
            })?;
            let star_profile = StarTargetProfile::parse(station_profile, &station_contents)
                .with_context(|| {
                    format!("failed to load station profile {}", station_path.display())
                })?;
            lab_compiler::backend::workcell::compile_dependency_build(
                &protocol,
                profile,
                &star_profile,
                &inventory,
            )
            .with_context(|| format!("failed to compile the {target} build"))?
            .artifacts()
            .clone()
        }
    };
    let target_root = output_root.join(target);
    let mut protocols = Vec::new();
    let mut typst_sources = Vec::new();
    for artifact in artifacts.iter() {
        let path = target_root.join(artifact.path());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, artifact.contents())
            .with_context(|| format!("failed to write {}", path.display()))?;
        if is_automation_protocol(&path) {
            protocols.push(path);
        }
        if artifact.media_type() == "text/x-typst" && is_typeset_document(artifact.path()) {
            typst_sources.push(artifact.path().to_owned());
        }
    }
    protocols.sort();
    typst_sources.sort();

    // Typeset every emitted document to a PDF beside its source. A failure
    // here is a bug in the emitters — the sources are generated — so the
    // build stops rather than shipping a package with missing documents.
    let mut documents = Vec::new();
    let typesetter = crate::typeset::Typesetter::new();
    for source in &typst_sources {
        let pdf_bytes = typesetter
            .compile_pdf(&target_root, source)
            .with_context(|| format!("failed to typeset {source}"))?;
        let pdf_path = target_root.join(source).with_extension("pdf");
        fs::write(&pdf_path, pdf_bytes)
            .with_context(|| format!("failed to write {}", pdf_path.display()))?;
        documents.push(pdf_path);
    }

    Ok(TargetBuild {
        directory: target_root,
        protocols,
        documents,
    })
}

/// A `text/x-typst` artifact is a complete document unless it is the shared
/// style sheet the documents import.
fn is_typeset_document(path: &str) -> bool {
    !path.ends_with("lab-style.typ")
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
    target: Option<String>,
    target_output: Option<PathBuf>,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
}

#[derive(Serialize)]
struct PlanCompleted {
    package: String,
    version: String,
    output: PathBuf,
    requirements: PathBuf,
    instances: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter_bindings: Option<PathBuf>,
    allocation: PathBuf,
    execution_plan: PathBuf,
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
}

#[derive(Serialize)]
struct BuildModule {
    package: String,
    module: String,
    source: PathBuf,
    artifact: PathBuf,
}
