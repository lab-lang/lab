use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_language::{DiagnosticSeverity, SourceId, analyze_module, render_diagnostic};
use lab_package::{LabPackage, PackageManifest};
use lab_project::{
    FacilityArtifactBuild, FacilityDocumentRenderer, ProjectArtifactRequest, ProjectBuildRequest,
    ProjectCompilation, ProjectProgram,
};
use lab_python_bindings::PackageModule;
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

pub(crate) fn new_method_pack(path: PathBuf, name: Option<String>, output: &Output) -> Result<()> {
    let package_name = contribution_name(&path, name)?;
    let module_namespace = package_name.replace('-', "_");
    prepare_empty_directory(&path)?;
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("methods"))?;
    write_new(
        &path.join("lab.toml"),
        &format!(
            "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[methods]\ndocuments = [\"methods/methods.json\"]\n"
        ),
    )?;
    write_new(
        &path.join("src/vocabulary.lab"),
        &format!(
            "/*!\n * Scientific vocabulary refined by the `{package_name}` Method pack.\n */\n\nrecord ExampleSample\n\n/** A concrete action whose implementation is supplied by this Method pack. */\naction prepare <sample> -> prepared:\n  sample: take Material<ExampleSample>\n  prepared: Material<ExampleSample> continues from sample\n"
        ),
    )?;
    let method_catalog = serde_json::json!({
        "schema_version": "lab.method-catalog.v2",
        "methods": [
            {
                "id": "https://example.org/lab-method#prepare-example-sample",
                "refines": format!("{module_namespace}.vocabulary.prepare"),
                "inputs": [
                    {
                        "name": "sample",
                        "port_type": {"kind": "material_as_supplied"}
                    }
                ],
                "tasks": [
                    {
                        "id": "prepare",
                        "operation": "https://example.org/lab-procedure#PrepareExampleSample",
                        "inputs": [{"kind": "input", "input": "sample"}],
                        "outputs": [
                            {
                                "name": "prepared",
                                "port_type": {"kind": "material_as_requested"}
                            }
                        ],
                        "execution": {
                            "kind": "primitive",
                            "requirements": [
                                {
                                    "id": "sample-preparation",
                                    "capability_kind": "https://example.org/lab-capability#SamplePreparation",
                                    "minimum_qualification": "https://sbol.io/ns/facility#Plannable",
                                    "accepted_control_modes": [
                                        "https://sbol.io/ns/facility#ManualControl"
                                    ]
                                }
                            ]
                        }
                    }
                ],
                "outputs": [
                    {
                        "name": "prepared",
                        "source": {
                            "kind": "task_output",
                            "task": "prepare",
                            "output": "prepared"
                        }
                    }
                ]
            }
        ]
    });
    let mut method_catalog = serde_json::to_string_pretty(&method_catalog)?;
    method_catalog.push('\n');
    write_new(&path.join("methods/methods.json"), &method_catalog)?;
    write_new(
        &path.join("README.md"),
        &format!(
            "# {package_name}\n\nThis package contributes portable Methods without changing the compiler. `src/vocabulary.lab` declares a small example action and `methods/methods.json` refines its exact operation ID into a complete manual Procedure task. Replace those two matching definitions with your own vocabulary and implementation.\n\nEvery Procedure task chooses one execution form: a declarative `template`, a registered `builder`, or direct `primitive` requirements. Templates substitute only explicit `$lab` slots such as `{{\"$lab\": {{\"kind\": \"integer\", \"id\": \"cycles\"}}}}` and are validated against their named Procedure contract.\n\nRun `lab check .` as the conformance test. It validates the Lab interfaces, the `lab.method-catalog.v2` envelope, every Method graph, and the composed registry.\n"
        ),
    )?;
    write_new(&path.join(".gitignore"), ".lab/\n")?;
    output.success(
        "created",
        ContributionCreated {
            kind: "method-pack",
            name: package_name.clone(),
            root: path.clone(),
            conformance: "lab check .",
        },
        format!(
            "Created Method pack {package_name} in {}\n  Conformance: cd {} && lab check .",
            path.display(),
            path.display()
        ),
    )
}

pub(crate) fn new_adapter(
    path: PathBuf,
    name: Option<String>,
    driver: Option<String>,
    output: &Output,
) -> Result<()> {
    let crate_name = contribution_name(&path, name)?;
    let driver = driver.unwrap_or_else(|| crate_name.replace('-', "."));
    if driver.is_empty()
        || driver
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("adapter driver ID must be non-empty and contain no whitespace or controls");
    }
    let adapter_api_version = env!("CARGO_PKG_VERSION");
    prepare_empty_directory(&path)?;
    fs::create_dir_all(path.join("src"))?;
    write_new(
        &path.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nlab-adapter-api = \"{adapter_api_version}\"\nserde_json = \"1\"\n"
        ),
    )?;
    let rust_driver = format!("{driver:?}");
    write_new(
        &path.join("src/lib.rs"),
        &format!(
            r#"//! Adapter registration for `{driver}`.

use std::collections::BTreeSet;

use lab_adapter_api::{{
    AdapterDescriptor, AdapterInvocation, AdapterInvocationLowering, AdapterInvocationPlan,
    AdapterLoweringError, AdapterProfileContractError, AdapterRegistration,
    PlanningProcedureTask, ProcedureContractRegistry, ProcedureImplementationDescriptor,
    ValidatedAdapterProfile,
}};

pub const DRIVER: &str = {rust_driver};

fn validate_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {{
    if !contents.trim().is_empty() {{
        return Err(AdapterProfileContractError::Invalid {{
            driver: DRIVER.to_owned(),
            message: "the scaffold accepts only an empty profile; replace validate_profile"
                .to_owned(),
        }});
    }}
    lab_adapter_api::canonical_adapter_profile(DRIVER, name, &serde_json::json!({{}}))
}}

/// All immutable facts this adapter contributes to application composition.
pub fn descriptor() -> AdapterDescriptor {{
    AdapterDescriptor {{
        id: DRIVER.to_owned(),
        display_name: "Describe this instrument".to_owned(),
        manufacturer: None,
        features: BTreeSet::new(),
        procedure_implementations: Vec::new(),
        profile_schema: serde_json::json!({{"type": "object"}}),
        default_profile: validate_profile("default", "")
            .expect("the built-in scaffold profile is valid"),
    }}
}}

/// Reject programs this profile cannot realize before the facility solver selects this adapter.
fn check_program_feasibility(
    _profile: &ValidatedAdapterProfile,
    _implementation: &ProcedureImplementationDescriptor,
    _task: &PlanningProcedureTask,
    _contracts: &ProcedureContractRegistry,
) -> Result<(), String> {{
    Err("replace the scaffold's Procedure feasibility callback".to_owned())
}}

/// Lower one exact invocation from its immutable allocated Procedure plan.
fn lower_invocation(
    profile: &ValidatedAdapterProfile,
    _plan: &AdapterInvocationPlan,
    _invocation: &AdapterInvocation,
    _contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {{
    Err(AdapterLoweringError::UnsupportedInvocation {{
        driver: profile.driver.clone(),
    }})
}}

/// The complete executable registration an application composes in one line.
pub fn registration() -> AdapterRegistration {{
    AdapterRegistration::new(
        descriptor(),
        validate_profile,
        check_program_feasibility,
        lower_invocation,
    )
}}

#[cfg(test)]
mod tests {{
    #[test]
    fn registration_conforms_to_the_adapter_api() {{
        let registry = lab_adapter_api::AdapterRegistry::new([super::registration()]).unwrap();
        assert!(registry.descriptors().descriptor(super::DRIVER).is_some());
        registry.validate_profile(super::DRIVER, "test", "").unwrap();
    }}
}}
"#
        ),
    )?;
    write_new(
        &path.join("README.md"),
        &format!(
            "# {crate_name}\n\nThis crate contributes the `{driver}` adapter through `lab_adapter_api::AdapterRegistration`. Its only Lab dependency is `lab-adapter-api`: that crate re-exports the canonical Procedure, planning-task, and allocated-task types used by named callbacks. Extend `descriptor()`, `validate_profile`, `check_program_feasibility`, and `lower_invocation`, and keep device-specific code inside this crate. A host application composes it with its `AdapterRegistry::with_registration({crate_name}::registration())`; no biological operation allowlist or compiler match is required. Runtime document loading and hardware execution are a separate optional integration layer.\n\nRun `cargo test` as the registration conformance test. The generated callbacks fail closed until you declare and implement an exact Procedure contract.\n"
        ),
    )?;
    output.success(
        "created",
        ContributionCreated {
            kind: "adapter",
            name: crate_name.clone(),
            root: path.clone(),
            conformance: "cargo test",
        },
        format!(
            "Created adapter {crate_name} ({driver}) in {}\n  Conformance: cd {} && cargo test",
            path.display(),
            path.display()
        ),
    )
}

pub(crate) fn python_bindings(
    path: PathBuf,
    out_dir: Option<PathBuf>,
    output: &Output,
) -> Result<()> {
    if path == Path::new("std") {
        let generated = lab_python_bindings::generate_standard_library(
            &lab_language::standard_library_manifest(),
        )?;
        let output_root = out_dir.unwrap_or_else(|| PathBuf::from("bindings/python/lab"));
        lab_python_bindings::write_generated_files(&output_root, &generated)?;
        return output.success(
            "generated",
            BindingsGenerated {
                package: "std".to_owned(),
                language: "python",
                output: output_root.clone(),
                files: generated.len(),
            },
            format!(
                "Generated {} Python binding files for std in {}",
                generated.len(),
                output_root.display()
            ),
        );
    }
    let application = ProjectCompilation::load(&path)?;
    let package = application.project().default_package();
    let package_name = package.manifest.package.name.clone();
    let modules = application
        .compiled()
        .modules
        .iter()
        .map(|module| PackageModule {
            package: module.package.clone(),
            interface: module.module.interface.clone(),
            imports: module.module.imports.clone(),
        })
        .collect::<Vec<_>>();
    let generated = lab_python_bindings::generate_package(&package_name, &modules)?;
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => package.root.join(path),
        None => package.root.join("bindings/python"),
    };
    lab_python_bindings::write_generated_files(&output_root, &generated)?;
    output.success(
        "generated",
        BindingsGenerated {
            package: package_name.clone(),
            language: "python",
            output: output_root.clone(),
            files: generated.len(),
        },
        format!(
            "Generated {} Python binding files for {package_name} in {}",
            generated.len(),
            output_root.display()
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

    let application = ProjectCompilation::load(&path)?;
    let project = application.project();
    let compiled = application.compiled();
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
    program: Option<String>,
    output: &Output,
) -> Result<()> {
    let application = ProjectCompilation::load(&path)?;
    let project_root = application.project().root().to_path_buf();
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => project_root.join(path),
        None => project_root.join(".lab").join("build"),
    };
    let built = application.write_build_artifacts(ProjectBuildRequest {
        program: program
            .as_deref()
            .map_or(ProjectProgram::Default, ProjectProgram::Named),
        methods: None,
        output_root: &output_root,
        renderer: Some(&CliDocumentRenderer),
    })?;

    let mut human = format!(
        "Built {} {} ({} modules)",
        built.package, built.version, built.modules
    );
    if built.products.is_empty() {
        human.push_str("\n\nBuild products: none");
    } else {
        human.push_str("\n\nBuild products:");
        for product in &built.products {
            human.push_str(&format!("\n  {} {}", product.kind, product.name));
        }
    }
    human.push_str(&format!(
        "\n\nCompiler output: {}",
        human_path(&output_root)
    ));
    if let Some(facility) = &built.facility {
        human.push_str(&format!(
            "\n\nFacility outputs:\n  Facility: {}\n  Methods selected: {}\n  Requirements allocated: {}\n  Adapter invocations lowered: {}\n  Planning problem: {}\n  Facility solution: {}\n  Allocated LAIR: {}\n  Adapter invocations: {}\n  Lowering manifest: {}\n  Reviewed plan: {}",
            facility.facility,
            facility.selected_methods,
            facility.allocated_requirements,
            facility.adapter_lowerings,
            human_path(&facility.planning_problem),
            human_path(&facility.facility_solution),
            human_path(&facility.allocated_lair),
            human_path(&facility.adapter_invocations),
            human_path(&facility.lowering),
            human_path(&facility.execution_plan)
        ));
        append_facility_artifacts(&mut human, facility);
        append_unlowered_warning(&mut human, facility);
    }
    output.success("built", built, human)
}

/// Says plainly when a plan allocated work to instruments but emitted nothing to run on them.
///
/// A build that reports success while lowering zero invocations looks finished. The requirements
/// are still bound to Assets, so the plan claims the work happens on a robot, and only `lab run`
/// would discover that no document exists.
fn append_unlowered_warning(human: &mut String, facility: &FacilityArtifactBuild) {
    if facility.adapter_lowerings > 0 || facility.allocated_requirements == 0 {
        return;
    }
    human.push_str(
        "\n\nNo device documents were emitted. Requirements are allocated to Assets, but no \
configured adapter claimed them, so this plan has nothing to execute. Add an \
`[[execution.adapters]]` entry for the bound Asset, or set `adapter-requirement = \"non-manual\"` \
under `[planning]` to make this an error instead of a warning.",
    );
}

pub(crate) fn plan(
    path: PathBuf,
    out_dir: Option<PathBuf>,
    program: Option<String>,
    output: &Output,
) -> Result<()> {
    let application = ProjectCompilation::load(&path)?;
    let project = application.project();
    let project_root = project.root();
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => project_root.join(path),
        // Each program's plan is its own reviewable artifact, so it gets its
        // own directory rather than overwriting the last program planned.
        None => match &program {
            Some(program) => project_root.join(".lab").join("plan").join(program),
            None => project_root.join(".lab").join("plan"),
        },
    };
    let planned = write_facility_plan(&application, &output_root, program.as_deref())?;
    let mut human = format!(
        "Planned {} {} against {}\n  Methods selected: {}\n  Requirements allocated: {}\n  Adapter invocations lowered: {}\n  Plan output: {}\n  Planning problem: {}\n  Facility solution: {}\n  Allocated LAIR: {}\n  Adapter invocations: {}\n  Reviewed plan: {}",
        planned.package,
        planned.version,
        planned.facility,
        planned.selected_methods,
        planned.allocated_requirements,
        planned.adapter_lowerings,
        human_path(&planned.output),
        human_path(&planned.planning_problem),
        human_path(&planned.facility_solution),
        human_path(&planned.allocated_lair),
        human_path(&planned.adapter_invocations),
        human_path(&planned.execution_plan)
    );
    append_facility_artifacts(&mut human, &planned);
    output.success("planned", planned, human)
}

struct CliDocumentRenderer;

impl FacilityDocumentRenderer for CliDocumentRenderer {
    fn render_typst(&self, root: &Path, document: &str) -> std::result::Result<Vec<u8>, String> {
        crate::typeset::Typesetter::new()
            .compile_pdf(root, document)
            .map_err(|error| format!("{error:#}"))
    }
}

fn write_facility_plan(
    application: &ProjectCompilation,
    output_root: &Path,
    program: Option<&str>,
) -> Result<FacilityArtifactBuild> {
    application
        .write_facility_artifacts(ProjectArtifactRequest {
            program: program.map_or(ProjectProgram::Default, ProjectProgram::Named),
            methods: None,
            output_root,
            renderer: Some(&CliDocumentRenderer),
        })
        .map_err(Into::into)
}

fn append_facility_artifacts(human: &mut String, planned: &FacilityArtifactBuild) {
    if !planned.bundles.is_empty() {
        human.push_str("\n\nAsset bundles:");
        for bundle in &planned.bundles {
            human.push_str(&format!("\n  {}", human_path(bundle)));
        }
    }
    if !planned.protocols.is_empty() {
        human.push_str("\n\nAutomation protocols:");
        for protocol in &planned.protocols {
            human.push_str(&format!("\n  {}", human_path(protocol)));
        }
    }
    if !planned.documents.is_empty() {
        human.push_str("\n\nDocuments:");
        for document in &planned.documents {
            human.push_str(&format!("\n  {}", human_path(document)));
        }
    }
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

fn contribution_name(path: &Path, name: Option<String>) -> Result<String> {
    let name = name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("lab-contribution")
            .to_owned()
    });
    validate_package_name(&name)?;
    Ok(name)
}

fn prepare_empty_directory(path: &Path) -> Result<()> {
    if path.exists() && fs::read_dir(path)?.next().is_some() {
        bail!("{} already exists and is not empty", path.display());
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

fn write_new(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        bail!("refusing to overwrite {}", path.display());
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

#[derive(Serialize)]
struct ProjectCreated {
    package: String,
    root: PathBuf,
    entry: PathBuf,
}

#[derive(Serialize)]
struct ContributionCreated {
    kind: &'static str,
    name: String,
    root: PathBuf,
    conformance: &'static str,
}

#[derive(Serialize)]
struct BindingsGenerated {
    package: String,
    language: &'static str,
    output: PathBuf,
    files: usize,
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
