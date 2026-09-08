use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(arguments)
        .output()
        .unwrap()
}

fn temporary_project() -> PathBuf {
    std::env::temp_dir().join(format!(
        "lab-cli-project-{}-{}",
        std::process::id(),
        NEXT_TEST.fetch_add(1, Ordering::Relaxed)
    ))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lab" {
            continue;
        }
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files.extend(walk_files(&entry.path()));
        } else {
            files.push(entry.path());
        }
    }
    files
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn read_text(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Read the review files emitted by the canonical per-Procedure adapter boundary.
fn task_manifests(root: &Path) -> Vec<(PathBuf, Value)> {
    let mut paths = walk_files(root)
        .into_iter()
        .filter(|p| p.file_name().unwrap() == "invocation_manifest.json")
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let v = read_json(&p);
            (p, v)
        })
        .collect()
}

fn operation<'a>(manifests: &'a [(PathBuf, Value)], name: &str) -> Vec<&'a Value> {
    manifests
        .iter()
        .filter(|(_, m)| {
            m["task"]["operation"]
                .as_str()
                .unwrap()
                .ends_with(&format!("#{name}"))
        })
        .map(|(_, m)| m)
        .collect()
}

fn quantity(value: &Value) -> f64 {
    value["value"]["value"].as_str().unwrap().parse().unwrap()
}

fn with_portable_manual_method_pins(manifest: String) -> String {
    manifest
        .replace(
            "https://www.lab-compiler.org/ns/method#temperature-staged-golden-gate",
            "https://www.lab-compiler.org/ns/method#automated-golden-gate",
        )
        .replace(
            "https://www.lab-compiler.org/ns/method#automated-chemical-transformation",
            "https://www.lab-compiler.org/ns/method#manual-chemical-transformation",
        )
        .replace(
            "https://www.lab-compiler.org/ns/method#automated-recovery",
            "https://www.lab-compiler.org/ns/method#manual-recovery",
        )
        .replace(
            "https://www.lab-compiler.org/ns/method#automated-antibiotic-selection",
            "https://www.lab-compiler.org/ns/method#manual-antibiotic-selection",
        )
}

fn solution_requirements(solution: &Value) -> Vec<&Value> {
    solution["selections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|selection| selection["tasks"].as_array().unwrap())
        .flat_map(|task| task["requirements"].as_array().unwrap())
        .collect()
}

fn solution_materials(solution: &Value) -> Vec<&Value> {
    solution["selections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|selection| selection["tasks"].as_array().unwrap())
        .flat_map(|task| {
            task.get("materials")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect()
}

fn assert_serial_dilutions_use_pipetting(
    invocations: &Value,
    expected_asset: &str,
    expected_driver: &str,
    expected_implementation: &str,
) {
    let tasks = invocations["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|method| method["tasks"].as_array().unwrap())
        .filter(|task| {
            task["operation"] == "https://www.lab-compiler.org/ns/procedure#SeriallyDiluteCulture"
        })
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 1);
    for task in tasks {
        assert_eq!(
            task["program"]["contract"],
            "https://www.lab-compiler.org/ns/procedure-contract#PipettingProgramV1"
        );
        assert!(
            task["program"]["body"]["vessels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|vessel| {
                    vessel["role"]["kind"] == "procedure_input" && vessel["role"]["input"] == 0
                })
        );
        let step_kinds = task["program"]["body"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step["kind"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            step_kinds,
            std::collections::BTreeSet::from(["distribute", "mix", "transfer"])
        );
        let requirements = task["requirements"].as_array().unwrap();
        assert_eq!(requirements.len(), 3);
        assert_eq!(
            requirements
                .iter()
                .map(|requirement| requirement["capability_kind"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                "https://sbol.io/ns/capability#InWellMixing",
                "https://sbol.io/ns/capability#LiquidLevelAwareAspiration",
                "https://sbol.io/ns/capability#MeteredLiquidTransfer",
            ])
        );
        assert!(requirements.iter().all(|requirement| {
            requirement["asset"] == expected_asset
                && requirement["adapter"]["driver"] == expected_driver
                && requirement["procedure_implementation"] == expected_implementation
        }));
    }
}

fn assert_golden_gate_uses_thermal_program(
    invocations: &Value,
    expected_asset: &str,
    expected_driver: &str,
    expected_implementation: &str,
) {
    let tasks = invocations["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|method| method["tasks"].as_array().unwrap())
        .filter(|task| {
            task["operation"]
                == "https://www.lab-compiler.org/ns/procedure#ThermalCycleGoldenGateReaction"
        })
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 3);
    for task in tasks {
        assert_eq!(
            task["program"]["contract"],
            "https://www.lab-compiler.org/ns/procedure-contract#ThermalProgramV1"
        );
        let program = &task["program"]["body"];
        assert_eq!(program["load"]["input"], 0);
        assert_eq!(program["load"]["outputs"], serde_json::json!(["product"]));
        assert_eq!(program["load"]["volume_each"]["value"]["value"], "25");
        assert_eq!(program["stages"][0]["id"], "digest-ligate-cycle");
        assert_eq!(program["stages"][0]["steps"][0]["id"], "digest");
        assert_eq!(program["stages"][0]["steps"][1]["id"], "ligate");
        assert_eq!(program["stages"][1]["steps"][0]["id"], "final-digest");
        assert_eq!(program["stages"][1]["steps"][1]["id"], "heat-inactivation");
        assert_eq!(program["final_hold"]["value"]["value"], "4");
        let requirements = task["requirements"].as_array().unwrap();
        assert_eq!(requirements.len(), 2);
        assert_eq!(
            requirements
                .iter()
                .map(|requirement| requirement["capability_kind"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                "https://sbol.io/ns/capability#HeatedLidTemperatureControl",
                "https://sbol.io/ns/capability#ProgrammedBlockTemperatureControl",
            ])
        );
        assert!(requirements.iter().all(|requirement| {
            requirement["asset"] == expected_asset
                && requirement["adapter"]["driver"] == expected_driver
                && requirement["procedure_implementation"] == expected_implementation
        }));
    }
}

#[test]
fn new_check_build_and_metadata_form_one_project_loop() {
    let project = temporary_project();
    let project_text = project.to_string_lossy().into_owned();

    let created = run(&["new", "project", &project_text, "--name", "test-project"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(project.join("lab.toml").is_file());
    assert!(project.join("src/programs/main.lab").is_file());

    let checked = run(&["check", &project_text]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8_lossy(&checked.stdout).contains("Checked test-project 0.1.0"));

    let built = run(&["build", &project_text]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let build_output = String::from_utf8_lossy(&built.stdout);
    assert!(
        build_output.contains("Build products:\n  plasmid starter"),
        "{build_output}"
    );
    assert!(
        !build_output.contains("Facility outputs:"),
        "{build_output}"
    );
    let index_path = project.join(".lab/build/package.json");
    let index = read_json(index_path);
    assert_eq!(index["schema_version"], 8);
    assert_eq!(index["package"], "test-project");
    assert_eq!(index["modules"][0]["module"], "test_project.programs.main");
    assert_eq!(index["compiler"]["refined_lair"], "compiler/refined.lair");
    assert_eq!(
        index["compiler"]["planning_problem"],
        "compiler/planning-problem.json"
    );
    assert!(index["compiler"].get("facility_solution").is_none());
    assert!(index.get("capability_requirements").is_none());
    assert!(index.get("capability_instances").is_none());
    assert!(index.get("facility").is_none());
    assert!(project.join(".lab/build/compiler/refined.lair").is_file());
    let problem = read_json(project.join(".lab/build/compiler/planning-problem.json"));
    assert_eq!(problem["schema_version"], "lab.planning-problem.v2");
    assert_eq!(problem["choices"].as_array().unwrap().len(), 1);
    assert_eq!(
        problem["choices"][0]["source_operation"],
        "std.bio.build.realize"
    );
    assert_eq!(
        problem["choices"][0]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        problem["choices"][0]["candidates"][0]["method"],
        "https://www.lab-compiler.org/ns/method#manual-artifact-realization"
    );
    assert_eq!(
        problem["choices"][0]["candidates"][0]["tasks"][0]["requirements"][0]["capability_kind"],
        "https://sbol.io/ns/capability#ArtifactRealization"
    );
    assert_eq!(
        problem["choices"][0]["candidates"][0]["tasks"][0]["requirements"][0]["minimum_qualification"],
        "https://sbol.io/ns/facility#Plannable"
    );
    assert!(project.join("lab.lock").is_file());

    let metadata = run(&["metadata", &project_text, "--json"]);
    assert!(metadata.status.success());
    let metadata: Value = serde_json::from_slice(&metadata.stdout).unwrap();
    assert_eq!(metadata["status"], "metadata");
    assert_eq!(
        metadata["result"]["modules"][0]["module"],
        "test_project.programs.main"
    );

    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn the_contribution_example_bindings_are_generated_from_its_checked_package() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/contributing/scientific-package");
    let output = tempfile::tempdir().unwrap();
    let generated = run(&[
        "bindings",
        "python",
        example.to_str().unwrap(),
        "--out-dir",
        output.path().to_str().unwrap(),
    ]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let mut count = 0;
    for path in walk_files(output.path()) {
        let relative = path.strip_prefix(output.path()).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            fs::read(example.join("bindings/python").join(relative)).unwrap(),
            "stale binding {}",
            relative.display()
        );
        count += 1;
    }
    assert!(count >= 4);
}

#[test]
fn focused_scaffolds_expose_their_conformance_paths() {
    let parent = temporary_project();
    std::fs::create_dir_all(&parent).unwrap();

    let methods = parent.join("thermal-methods");
    let methods_text = methods.to_string_lossy().into_owned();
    let created = run(&[
        "new",
        "method-pack",
        &methods_text,
        "--name",
        "thermal-methods",
    ]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(methods.join("methods/methods.json").is_file());
    assert!(
        std::fs::read_to_string(methods.join("methods/methods.json"))
            .unwrap()
            .contains("lab.method-catalog.v2")
    );
    assert!(
        std::fs::read_to_string(methods.join("methods/methods.json"))
            .unwrap()
            .contains("thermal_methods.vocabulary.prepare")
    );
    assert!(
        std::fs::read_to_string(methods.join("src/vocabulary.lab"))
            .unwrap()
            .contains("action prepare <sample> -> prepared:")
    );
    assert!(
        std::fs::read_to_string(methods.join("README.md"))
            .unwrap()
            .contains("declarative `template`")
    );
    let conformed = run(&["check", &methods_text]);
    assert!(
        conformed.status.success(),
        "{}",
        String::from_utf8_lossy(&conformed.stderr)
    );

    let adapter = parent.join("acme-cycler");
    let adapter_text = adapter.to_string_lossy().into_owned();
    let created = run(&["new", "adapter", &adapter_text, "--driver", "acme.cycler"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let source = std::fs::read_to_string(adapter.join("src/lib.rs")).unwrap();
    let manifest_path = adapter.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest.contains(&format!(
        "lab-adapter-api = {:?}",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(!manifest.contains("lab-adapters ="));
    assert!(!manifest.contains("lab-compiler ="));
    assert!(!manifest.contains("lab-capability ="));
    assert!(source.contains("AdapterRegistration::new("));
    assert!(source.contains("fn check_program_feasibility("));
    assert!(source.contains("_task: &PlanningProcedureTask"));
    assert!(source.contains("fn lower_invocation("));
    assert!(source.contains("_plan: &AdapterInvocationPlan"));
    assert!(source.contains("_contracts: &ProcedureContractRegistry"));
    assert!(source.contains("check_program_feasibility,"));
    assert!(source.contains("lower_invocation,"));
    assert!(source.contains("AdapterRegistry::new([super::registration()])"));
    assert!(source.contains("canonical_adapter_profile(DRIVER, name"));
    assert!(source.contains("registration_conforms_to_the_adapter_api"));

    // Compile and run the scaffold exactly as an independent crate. Point its sole Lab
    // dependency at this checkout so the test exercises the generated source against the API
    // that emitted it instead of requiring a publication first.
    let adapter_api = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lab-adapter-api")
        .canonicalize()
        .unwrap();
    std::fs::write(
        &manifest_path,
        manifest.replace(
            &format!("lab-adapter-api = {:?}", env!("CARGO_PKG_VERSION")),
            &format!("lab-adapter-api = {{ path = {adapter_api:?} }}"),
        ),
    )
    .unwrap();
    // Reuse the tested dependency versions, including pinned versions that may have been
    // yanked since this checkout was locked. Cargo adds the scaffold's own package entry.
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        adapter.join("Cargo.lock"),
    )
    .unwrap();
    let conformed = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--quiet", "--manifest-path"])
        .arg(&manifest_path)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/adapter-scaffold-conformance"),
        )
        .output()
        .unwrap();
    assert!(
        conformed.status.success(),
        "generated adapter did not compile and pass its conformance test:\n{}\n{}",
        String::from_utf8_lossy(&conformed.stdout),
        String::from_utf8_lossy(&conformed.stderr),
    );

    std::fs::remove_dir_all(parent).unwrap();
}

#[test]
fn python_bindings_are_generated_from_the_checked_package_interface() {
    let project = temporary_project();
    let project_text = project.to_string_lossy().into_owned();
    let created = run(&["new", "project", &project_text, "--name", "binding-fixture"]);
    assert!(created.status.success());

    let generated = run(&["bindings", "python", &project_text]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let runtime = project.join("bindings/python/binding_fixture/programs/main.py");
    let stub = project.join("bindings/python/binding_fixture/programs/main.pyi");
    assert!(runtime.is_file());
    assert!(stub.is_file());
    assert!(
        std::fs::read_to_string(runtime)
            .unwrap()
            .contains("ImportedWorkflow(")
    );
    assert!(
        std::fs::read_to_string(stub)
            .unwrap()
            .contains("-> WorkflowCall[")
    );

    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn plan_binds_reachable_requirements_to_an_exact_facility_offering() {
    let project = temporary_project();
    std::fs::create_dir_all(project.join("src/programs")).unwrap();
    std::fs::create_dir_all(project.join("inventory")).unwrap();
    std::fs::write(
        project.join("lab.toml"),
        r#"[package]
name = "facility-plan"
version = "0.1.0"
edition = "2026"

[build]
entry = "src/programs/main.lab"

[inventory]
document = "inventory/catalog.ttl"
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("src/programs/main.lab"),
        r#"use std.bio.build
use std.bio.designs

plasmid starter:
  sequence = dna("ATGC")
  require topology == circular
  accept sequence == design.sequence

workflow main() -> Material<Plasmid>:
  product <- realize starter
  return product
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("inventory/catalog.ttl"),
        r#"@prefix cap: <https://sbol.io/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix sbol: <http://sbols.org/v3#> .

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/facility> .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:operator a sbol:TopLevel, fac:Asset ; sbol:displayId "operator" ;
    sbol:hasNamespace <https://example.org/facility> ;
    fac:assetKind fac:Workstation ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/facility/operator/realization> .
<https://example.org/facility/operator/realization>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "realization" ;
    fac:capabilityKind cap:ArtifactRealization ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ManualControl ; fac:isActive true .
"#,
    )
    .unwrap();
    let project_text = project.to_string_lossy().into_owned();

    let planned = run(&["plan", &project_text]);

    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let solution = read_json(project.join(".lab/plan/compiler/facility-solution.json"));
    assert_eq!(
        solution["schema_version"],
        "lab.facility-planning-solution.v3"
    );
    assert_eq!(
        solution["selections"][0]["method"],
        "https://www.lab-compiler.org/ns/method#manual-artifact-realization"
    );
    let requirements = solution_requirements(&solution);
    assert_eq!(requirements.len(), 1);
    assert_eq!(
        requirements[0]["offering"],
        "https://example.org/facility/operator/realization"
    );
    assert_eq!(
        requirements[0]["asset"],
        "https://example.org/facility/operator"
    );
    assert!(requirements[0].get("adapter").is_none());
    let plan = read_json(project.join(".lab/plan/plan.execution.json"));
    assert_eq!(plan["format"], "lab.execution-plan.v4");
    assert_eq!(
        plan["planning"]["facility_solution"]["path"],
        "compiler/facility-solution.json"
    );
    assert_eq!(
        plan["planning"]["methods"][0]["method"],
        "https://www.lab-compiler.org/ns/method#manual-artifact-realization"
    );
    assert_eq!(plan["inventory"]["document"], "inventory-source.ttl");
    assert_eq!(
        std::fs::read(project.join(".lab/plan/inventory-source.ttl")).unwrap(),
        std::fs::read(project.join("inventory/catalog.ttl")).unwrap()
    );
    assert_eq!(plan["requirements"].as_array().unwrap().len(), 1);
    assert_eq!(plan["nodes"][0]["action"], "manual");
    assert_eq!(
        plan["nodes"][0]["requirements"][0],
        plan["requirements"][0]["requirement_instance"]
    );
    assert!(
        plan["nodes"][0]["instructions"]
            .as_str()
            .unwrap()
            .contains("https://example.org/facility/operator/realization")
    );

    let plan_directory = project.join(".lab/plan");
    let reviewed = run(&["run", plan_directory.to_str().unwrap(), "--dry-run"]);
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    assert!(String::from_utf8_lossy(&reviewed.stdout).contains("all frozen inputs validated"));
    assert!(String::from_utf8_lossy(&reviewed.stdout).contains("planning-only bindings"));

    let live = run(&["run", plan_directory.to_str().unwrap(), "--yes"]);
    assert!(!live.status.success());
    assert!(String::from_utf8_lossy(&live.stderr).contains("reviewed plan is not ready for live"));
    assert!(!plan_directory.join("run-ledger.jsonl").exists());
}

#[test]
fn run_requires_a_reviewed_facility_plan() {
    let directory = temporary_project();
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("automation_manifest.json"),
        r#"{"schema_version":"lab.automation.v1","adapter":"hamilton.star"}"#,
    )
    .unwrap();

    let attempted = run(&["run", directory.to_str().unwrap(), "--dry-run"]);

    assert!(!attempted.status.success());
    let stderr = String::from_utf8_lossy(&attempted.stderr);
    assert!(stderr.contains("failed to read reviewed plan"), "{stderr}");
    assert!(stderr.contains("plan.execution.json"), "{stderr}");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn registry_dependencies_fail_closed_without_being_ignored() {
    let project = temporary_project();
    let project_text = project.to_string_lossy().into_owned();
    let created = run(&["new", "project", &project_text]);
    assert!(created.status.success());
    let manifest = project.join("lab.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[dependencies]\nparts = \"1.0\"\n");
    std::fs::write(&manifest, text).unwrap();

    let checked = run(&["check", &project_text]);
    assert!(!checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stderr).contains("not a path dependency"));

    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn check_validates_a_configured_sbol_inventory() {
    let project = temporary_project();
    let project_text = project.to_string_lossy().into_owned();
    let created = run(&["new", "project", &project_text]);
    assert!(created.status.success());

    let manifest = project.join("lab.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[inventory]\ndocument = \"inventory/catalog.ttl\"\n");
    std::fs::write(&manifest, text).unwrap();
    std::fs::create_dir(project.join("inventory")).unwrap();
    let valid = include_str!("fixtures/minimal-inventory.ttl");
    std::fs::write(project.join("inventory/catalog.ttl"), valid).unwrap();

    let checked = run(&["check", &project_text]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    let invalid = valid.replace("fac:isActive true", "fac:isActive \"yes\"");
    std::fs::write(project.join("inventory/catalog.ttl"), invalid).unwrap();
    let rejected = run(&["check", &project_text]);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("does not conform to SBOLInventory"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );

    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn build_freezes_exact_asset_offering_and_adapter_profile_bindings() {
    let project = temporary_project();
    let project_text = project.to_string_lossy().into_owned();
    let created = run(&["new", "project", &project_text]);
    assert!(created.status.success());

    let manifest = project.join("lab.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str(
        "\n[inventory]\ndocument = \"inventory/catalog.ttl\"\n\n[[execution.adapters]]\nasset = \"https://example.org/sbolinventory/cycler\"\ndriver = \"opentrons.ot2\"\nprofile = \"adapters/cycler.toml\"\n",
    );
    std::fs::write(&manifest, text).unwrap();
    std::fs::create_dir(project.join("inventory")).unwrap();
    let inventory = format!(
        "{}\n{}",
        include_str!("fixtures/minimal-inventory.ttl").replace(
            "cap:ThermalCycling",
            "cap:ProgrammedBlockTemperatureControl"
        ),
        r#"ex:operator a sbol:TopLevel, fac:Asset ;
    sbol:displayId "operator" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ;
    fac:assetKind fac:Workstation ;
    fac:locatedIn ex:room ;
    fac:isActive true ;
    fac:capability <https://example.org/sbolinventory/operator/artifact_realization> .

<https://example.org/sbolinventory/operator/artifact_realization>
    a sbol:Identified, fac:CapabilityOffering ;
    sbol:displayId "artifact_realization" ;
    fac:capabilityKind cap:ArtifactRealization ;
    fac:qualification fac:Plannable ;
    fac:controlMode fac:ManualControl ;
    fac:isActive true ."#,
    );
    std::fs::write(project.join("inventory/catalog.ttl"), inventory).unwrap();
    std::fs::create_dir(project.join("adapters")).unwrap();
    std::fs::write(project.join("adapters/cycler.toml"), "").unwrap();

    let checked = run(&["check", &project_text]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let built = run(&["build", &project_text]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let index = read_json(project.join(".lab/build/package.json"));
    assert_eq!(index["schema_version"], 8);
    assert_eq!(index["adapter_bindings"], "adapter_bindings.json");
    assert_eq!(
        index["compiler"]["facility_solution"],
        "compiler/facility-solution.json"
    );
    assert_eq!(
        index["compiler"]["adapter_invocations"],
        "compiler/adapter-invocations.json"
    );
    assert_eq!(
        index["facility"]["facility"],
        "https://example.org/sbolinventory/facility"
    );
    let bindings = read_json(project.join(".lab/build/adapter_bindings.json"));
    assert_eq!(bindings["schema_version"], "lab.adapter-bindings.v7");
    assert_eq!(
        bindings["facility"],
        "https://example.org/sbolinventory/facility"
    );
    assert_eq!(bindings["bindings"][0]["driver"], "opentrons.ot2");
    assert_eq!(
        bindings["bindings"][0]["asset"],
        "https://example.org/sbolinventory/cycler"
    );
    assert_eq!(
        bindings["bindings"][0]["offerings"][0]["offering"],
        "https://example.org/sbolinventory/cycler/thermal_cycling"
    );
    assert_eq!(
        bindings["bindings"][0]["offerings"][0]["planning_eligible"],
        true
    );
    assert_eq!(
        bindings["bindings"][0]["offerings"][0]["execution_eligible"],
        false
    );
    assert_eq!(
        bindings["bindings"][0]["profile_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn a_second_instrument_is_planned_once_the_package_names_which_to_use() {
    let project = temporary_project();
    let _ = fs::remove_dir_all(&project);
    copy_dir(Path::new("../../examples/golden-gate"), &project);

    // A laboratory with two of the same instrument is ordinary. They are not interchangeable, so
    // Lab asks which to use rather than choosing, but it must say so in a way that can be acted on.
    let inventory_path = project.join("inventory/facility.ttl");
    let inventory = fs::read_to_string(&inventory_path).unwrap();
    let duplicate = inventory
        .lines()
        .skip_while(|line| !line.starts_with("ex:opentrons_ot2"))
        .take_while(|line| !line.trim().is_empty() || false)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!duplicate.is_empty(), "the example declares an OT-2 Asset");

    let second = inventory
        .replace("ex:opentrons_ot2", "ex:otwo_b")
        .replace("\"opentrons_ot2", "\"otwo_b");
    let second_assets = second
        .lines()
        .filter(|line| line.starts_with("ex:otwo_b"))
        .count();
    assert!(second_assets > 0);
    fs::write(
        &inventory_path,
        format!(
            "{inventory}\n{}",
            second
                .split("\n\n")
                .filter(|block| block.trim_start().starts_with("ex:otwo_b"))
                .collect::<Vec<_>>()
                .join("\n\n")
        ),
    )
    .unwrap();

    let manifest_path = project.join("lab.toml");
    let mut manifest = read_text(&manifest_path);
    manifest.push_str("\n[[execution.adapters]]\nasset = \"https://example.org/golden-gate/otwo_b\"\ndriver = \"opentrons.ot2\"\nprofile = \"adapters/opentrons-ot2.toml\"\n");
    fs::write(&manifest_path, manifest).unwrap();

    let ambiguous = run(&["build", &project.to_string_lossy()]);
    assert!(!ambiguous.status.success());
    let message = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(
        message.contains("more than one complete plan")
            && message.contains("[[planning.assets]]")
            && message.contains("asset = "),
        "the ambiguity names a pin the user can paste: {message}"
    );

    let manifest_path = project.join("lab.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        format!(
            "{manifest}\n[[planning.assets]]\nasset = \"https://example.org/golden-gate/opentrons_ot2\"\n"
        ),
    )
    .unwrap();

    let pinned = run(&["build", &project.to_string_lossy()]);
    assert!(
        pinned.status.success(),
        "the suggested pin resolves the ambiguity: {}",
        String::from_utf8_lossy(&pinned.stderr)
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn every_shared_source_states_the_volume_the_batch_draws() {
    let project = temporary_project();
    let _ = fs::remove_dir_all(&project);
    copy_dir(Path::new("../../examples/golden-gate"), &project);

    let built = run(&["build", &project.to_string_lossy()]);
    assert!(
        built.status.success(),
        "golden-gate build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    let schedule_root = project.join(".lab/build/assets/opentrons_ot2");

    let manifests = task_manifests(&schedule_root);
    let mut source_count = 0;
    for (_, manifest) in &manifests {
        if manifest["execution"]["kind"] != "pipetting_program" {
            continue;
        }
        let execution = &manifest["execution"];
        for source in execution["sources"].as_array().unwrap() {
            let id = source["vessel"].as_str().unwrap();
            let volumes = execution["initial_volumes_ul"][id].as_array().unwrap();
            assert_eq!(volumes.len(), source["wells"].as_array().unwrap().len());
            assert!(volumes.iter().all(|v| v.as_f64().is_some()));
            source_count += 1;
        }
    }
    assert!(
        source_count > 20,
        "all reagent and upstream input loads are reviewed"
    );
    let dilution = operation(&manifests, "SeriallyDiluteCulture")[0];
    let medium = dilution["execution"]["program"]["vessels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["role"]["kind"] == "material_source")
        .unwrap();
    assert_eq!(
        dilution["execution"]["initial_volumes_ul"][medium["id"].as_str().unwrap()][0],
        10000.0
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn competent_cells_are_staged_on_a_temperature_controlled_position() {
    let project = temporary_project();
    let _ = fs::remove_dir_all(&project);
    copy_dir(Path::new("../../examples/golden-gate"), &project);

    let built = run(&["build", &project.to_string_lossy()]);
    assert!(
        built.status.success(),
        "golden-gate build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    let invocations = read_json(project.join(".lab/build/compiler/adapter-invocations.json"));
    let prepare = invocations["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|method| method["tasks"].as_array().unwrap())
        .find(|task| {
            task["id"]
                .as_str()
                .unwrap()
                .ends_with("::prepare-transformation")
        })
        .expect("the plan contains a transformation preparation");
    assert!(
        prepare["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|requirement| {
                requirement["capability_kind"]
                    == "https://sbol.io/ns/capability#TemperatureControlledStaging"
            }),
        "chemically competent cells lose efficiency at bench temperature, so staging is a \
         requirement the facility must satisfy rather than an adapter default"
    );

    let manifests = task_manifests(&project.join(".lab/build/assets/opentrons_ot2"));
    let prepare = operation(&manifests, "PrepareChemicalTransformation")[0];
    let cells = prepare["execution"]["program"]["vessels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["temperature"].is_object())
        .unwrap();
    let resource = prepare["execution"]["locations"][cells["id"].as_str().unwrap()][0]["resource"]
        ["kind"]
        .as_str()
        .unwrap();
    assert!(matches!(resource, "sources" | "work"));
    assert_eq!(prepare["execution"]["staging_temperatures"][resource], 4.0);
    let path = manifests
        .iter()
        .find(|(_, m)| m == prepare)
        .unwrap()
        .0
        .parent()
        .unwrap();
    let protocol = read_text(path.join("automation_protocol.py"));
    assert!(protocol.contains("temperature.set_temperature(staging[\"sources\"])"));
    assert!(protocol.contains("thermocycler.set_block_temperature(staging[\"work\"])"));

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn a_partly_stated_assembly_recipe_is_a_diagnostic_rather_than_a_manual_fallback() {
    let project = temporary_project();
    let _ = fs::remove_dir_all(&project);
    copy_dir(Path::new("../../examples/golden-gate"), &project);

    let plasmids = project.join("src/designs/plasmids.lab");
    let source = fs::read_to_string(&plasmids).unwrap();
    fs::write(
        &plasmids,
        source.replacen("  restriction_enzyme = BsaI\n", "", 1),
    )
    .unwrap();

    let output = run(&["build", &project.to_string_lossy()]);
    assert!(
        !output.status.success(),
        "an incomplete recipe must not build"
    );
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains("temperature-staged-golden-gate")
            && message.contains("not one of its candidates"),
        "the pinned incomplete Method cannot silently fall back: {message}"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn facility_lowering_emits_the_complete_golden_gate_ot2_slice() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let regression = read_json(example.join("reference/ot2-regression.json"));
    let number = |value: &Value| value.as_f64().expect("regression values are numeric");
    assert_eq!(
        regression["schema_version"],
        "lab.golden-gate-ot2-regression.v1"
    );
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-facility-lowering-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }
    std::fs::create_dir_all(out_dir.join("lowerings/stale")).unwrap();
    std::fs::write(out_dir.join("lowerings/stale/protocol.py"), "stale").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "plan",
            example.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "facility plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        read_text(out_dir.join("lowerings/stale/protocol.py")),
        "stale",
        "planning preserves files outside its owned artifact index"
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "planned");
    let protocols = result["result"]["protocols"].as_array().unwrap();
    assert_eq!(
        protocols.len(),
        12,
        "every automated Procedure has a file, including recovery and plating"
    );
    for path in protocols {
        assert!(Path::new(path.as_str().unwrap()).is_file());
    }
    let manifests = task_manifests(&out_dir.join("assets/opentrons_ot2"));
    assert_eq!(manifests.len(), 12);
    for (_, m) in &manifests {
        assert_eq!(m["schema_version"], "lab.opentrons-ot2-task.v1");
        assert!(!m["requirements"].as_array().unwrap().is_empty());
        assert_eq!(
            m["deck"]["instruments"]["small"]["model"],
            regression["hardware"]["p20_model"]
        );
        assert_eq!(
            m["deck"]["instruments"]["large"]["model"],
            regression["hardware"]["p300_model"]
        );
        assert_eq!(
            m["deck"]["resources"]["sources"]["model"],
            regression["hardware"]["temperature_module_load_name"]
        );
        assert_eq!(
            m["deck"]["resources"]["work"]["model"],
            regression["hardware"]["thermocycler_load_name"]
        );
    }
    let setups = operation(&manifests, "SetupGoldenGateReaction");
    assert_eq!(setups.len(), 3);
    for setup in setups {
        assert_eq!(
            setup["execution"]["staging_temperatures"]["sources"]
                .as_f64()
                .unwrap(),
            number(&regression["assembly"]["source_temperature_c"])
        );
        let steps = setup["execution"]["program"]["steps"].as_array().unwrap();
        let transfers = steps
            .iter()
            .filter(|s| s["kind"] == "transfer")
            .collect::<Vec<_>>();
        assert_eq!(
            transfers
                .iter()
                .map(|s| quantity(&s["volume"]))
                .sum::<f64>(),
            number(&regression["assembly"]["reaction_volume_ul"])
        );
        assert!(
            transfers
                .iter()
                .all(|s| s["technique"]["blow_out"] == true && s["technique"]["touch_tip"] == true)
        );
        let clears = steps
            .iter()
            .filter(|s| s["id"].as_str().unwrap().starts_with("clear-bubbles"))
            .collect::<Vec<_>>();
        assert_eq!(
            clears.len() as u64,
            regression["assembly"]["bubble_clear"]["cycles"]
                .as_u64()
                .unwrap()
        );
        for clear in clears {
            assert_eq!(clear["cycles"], 1);
            assert_eq!(
                quantity(&clear["volume"]),
                number(&regression["assembly"]["bubble_clear"]["volume_ul"])
            );
            assert_eq!(
                clear["fluid_path_group"],
                transfers.last().unwrap()["fluid_path_group"]
            );
            assert_eq!(clear["technique"]["blow_out"], true);
            assert_eq!(clear["technique"]["touch_tip"], true);
        }
    }
    let cycles = operation(&manifests, "ThermalCycleGoldenGateReaction");
    assert_eq!(cycles.len(), 3);
    for cycle in cycles {
        let e = &cycle["execution"];
        assert_eq!(
            number(&e["lid_temperature_c"]),
            number(&regression["assembly"]["thermal"]["lid_temperature_c"])
        );
        assert_eq!(
            number(&e["final_hold_celsius"]),
            number(&regression["assembly"]["thermal"]["final_hold_c"])
        );
        let stages = e["profile"]["stages"].as_array().unwrap();
        let expected = regression["assembly"]["thermal"]["stages"]
            .as_array()
            .unwrap();
        assert_eq!(stages.len(), expected.len());
        for (stage, expected) in stages.iter().zip(expected) {
            assert_eq!(stage["repeats"], expected["repeats"]);
            let steps = stage["steps"].as_array().unwrap();
            assert_eq!(steps.len(), expected["steps"].as_array().unwrap().len());
            for (step, expected) in steps.iter().zip(expected["steps"].as_array().unwrap()) {
                assert_eq!(number(&step["celsius"]), number(&expected["temperature_c"]));
                assert_eq!(
                    number(&step["hold_seconds"]),
                    number(&expected["hold_seconds"])
                );
            }
        }
    }
    let prepare = operation(&manifests, "PrepareChemicalTransformation")[0];
    let steps = prepare["execution"]["program"]["steps"].as_array().unwrap();
    let cells = steps
        .iter()
        .find(|s| s["id"] == "add-competent-cells")
        .unwrap();
    assert_eq!(
        quantity(&cells["volume_each"]),
        number(&regression["transformation"]["competent_cells"]["volume_ul"])
    );
    assert_eq!(cells["destinations"].as_array().unwrap().len(), 3);
    assert_eq!(cells["fluid_path_group"], steps[0]["fluid_path_group"]);
    let dna = steps
        .iter()
        .filter(|s| s["kind"] == "transfer")
        .collect::<Vec<_>>();
    assert_eq!(
        dna.len(),
        9,
        "three DNA sources for each of three replicates"
    );
    assert!(dna.iter().all(
        |s| quantity(&s["volume"]) == number(&regression["transformation"]["dna"]["volume_ul"])
    ));
    let recovery = operation(&manifests, "AddRecoveryMedium")[0];
    let add = &recovery["execution"]["program"]["steps"][0];
    assert_eq!(
        quantity(&add["volume_each"]),
        number(&regression["transformation"]["recovery"]["medium_volume_ul"])
    );
    assert_eq!(
        quantity(&add["technique"]["air_gap"]),
        number(&regression["transformation"]["recovery"]["air_gap_ul"])
    );
    assert_eq!(add["destinations"].as_array().unwrap().len(), 3);
    let heat = operation(&manifests, "HeatShockTransformation")[0];
    assert_eq!(
        heat["execution"]["volume_each_ul"].as_f64().unwrap(),
        number(&regression["transformation"]["heat_shock"]["volume_ul"])
    );
    let incubation = operation(&manifests, "IncubateRecoveryCulture")[0];
    assert_eq!(
        incubation["execution"]["volume_each_ul"].as_f64().unwrap(),
        number(&regression["transformation"]["recovery"]["incubation_volume_ul"])
    );
    let dilution = operation(&manifests, "SeriallyDiluteCulture")[0];
    let dilution_steps = dilution["execution"]["program"]["steps"]
        .as_array()
        .unwrap();
    let mixing = dilution_steps
        .iter()
        .filter(|s| s["kind"] == "mix")
        .collect::<Vec<_>>();
    assert_eq!(mixing.len(), 6);
    assert!(
        mixing
            .iter()
            .all(|s| s["cycles"] == regression["plating"]["mix_cycles"]
                && quantity(&s["volume"]) == number(&regression["plating"]["mix_volume_ul"]))
    );
    let plating = operation(&manifests, "PlateDilutedCulture")[0];
    let spots = plating["execution"]["program"]["steps"].as_array().unwrap();
    assert!(!spots.is_empty());
    assert!(spots.iter().all(|s| s["kind"] == "distribute"
        && quantity(&s["volume_each"]) == number(&regression["plating"]["colony_volume_ul"])
        && s["technique"]["dispense"]["kind"] == "material_surface"));
    let allocated = read_json(out_dir.join("compiler/adapter-invocations.json"));
    let emitted = manifests
        .iter()
        .flat_map(|(_, m)| m["requirements"].as_array().unwrap())
        .map(|r| r["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = allocated["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["tasks"].as_array().unwrap())
        .flat_map(|t| t["requirements"].as_array().unwrap())
        .filter(|r| r["adapter"].is_object())
        .map(|r| r["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        emitted, expected,
        "lowering covers every allocated requirement exactly"
    );

    std::fs::remove_dir_all(out_dir).unwrap();
}

#[test]
fn build_emits_facility_selected_protocol_bundles_and_documents() {
    // `build` updates the package lockfile, so run it against an isolated project.
    let example = temporary_project();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/golden-gate"),
        &example,
    );
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-facility-build-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let built = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "facility build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let result: Value = serde_json::from_slice(&built.stdout).unwrap();
    assert!(result["result"].get("target").is_none());
    assert!(result["result"].get("protocols").is_none());
    assert!(result["result"].get("documents").is_none());
    assert_eq!(result["result"]["products"].as_array().unwrap().len(), 4);
    assert_eq!(
        result["result"]["products"]
            .as_array()
            .unwrap()
            .iter()
            .map(|product| product["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["GVD0011", "GVD0013", "GVD0015", "GVD_strain",]
    );
    let facility = &result["result"]["facility"];
    assert_eq!(
        facility["facility"],
        "https://example.org/golden-gate/facility"
    );
    assert_eq!(facility["bundles"].as_array().unwrap().len(), 1);
    assert_eq!(facility["protocols"].as_array().unwrap().len(), 12);
    // Twelve adapter operator documents plus the run sheet for the plan's
    // manual-control steps.
    assert_eq!(facility["documents"].as_array().unwrap().len(), 13);
    for path in facility["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .chain(facility["documents"].as_array().unwrap())
    {
        assert!(Path::new(path.as_str().unwrap()).is_file(), "{path}");
    }
    assert!(out_dir.join("plan.execution.json").is_file());
    assert!(out_dir.join("assets/opentrons_ot2").is_dir());
    assert!(!out_dir.join("lowerings").exists());
    assert!(out_dir.join("package.json").is_file());
    let index = read_json(out_dir.join("package.json"));
    assert_eq!(index["adapter_bindings"], "adapter_bindings.json");
    assert_eq!(index["schema_version"], 8);
    assert_eq!(
        index["compiler"]["planning_problem"],
        "compiler/planning-problem.json"
    );
    assert_eq!(
        index["facility"]["facility_solution"],
        "compiler/facility-solution.json"
    );
    assert_eq!(index["facility"]["protocols"].as_array().unwrap().len(), 12);
    assert!(
        index["facility"]["protocols"][0]
            .as_str()
            .unwrap()
            .starts_with("assets/opentrons_ot2/")
    );
    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_eq!(invocations["schema_version"], "lab.adapter-invocations.v3");
    assert!(invocations.get("material_inventory").is_none());
    let j23101_binding = invocations["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|method| method["tasks"].as_array().unwrap())
        .flat_map(|task| {
            task.get("materials")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|binding| binding["symbol"] == "J23101")
        .expect("the adapter plan retains the exact selected J23101 binding");
    assert_eq!(
        j23101_binding["source"]["component"],
        "https://sbolcanvas.org/J23101"
    );
    assert_eq!(
        j23101_binding["source"]["material_lot"],
        "https://example.org/golden-gate/lots/J23101_lot"
    );
    let normalized_setup = invocations["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|method| method["tasks"].as_array().unwrap())
        .find(|task| {
            task["operation"] == "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction"
        })
        .unwrap();
    assert_eq!(
        normalized_setup["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|requirement| requirement["capability_kind"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "https://sbol.io/ns/capability#InWellMixing",
            "https://sbol.io/ns/capability#MeteredLiquidTransfer",
            "https://sbol.io/ns/capability#PostDispenseBlowout",
            "https://sbol.io/ns/capability#TemperatureControlledStaging",
            "https://sbol.io/ns/capability#TouchTip",
            "https://sbol.io/ns/capability#VesselRelativeLiquidAccess",
        ])
    );
    assert!(normalized_setup["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .all(|requirement| requirement["procedure_implementation"]
            == "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1"));
    let execution_plan = read_json(out_dir.join("plan.execution.json"));
    let setup_execute_node = execution_plan["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| {
            node["document"]["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/001-pipetting-program/automation_protocol.py"))
        })
        .cloned()
        .expect("the allocated assembly group has one reviewed execute node");
    assert_eq!(
        setup_execute_node["document"]["path"],
        "assets/opentrons_ot2/tasks/001-pipetting-program/automation_protocol.py"
    );
    assert_eq!(
        setup_execute_node["requirements"].as_array().unwrap().len(),
        6
    );
    assert_eq!(
        setup_execute_node["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|requirement| {
                execution_plan["requirements"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|binding| {
                        binding["requirement_instance"].as_str() == requirement.as_str()
                    })
                    .unwrap()["procedure_implementation"]
                    .as_str()
                    .unwrap()
            })
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1",
        ])
    );

    let human = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        human.status.success(),
        "facility build failed: {}",
        String::from_utf8_lossy(&human.stderr)
    );
    let printed = String::from_utf8(human.stdout).unwrap();
    assert!(printed.contains("Asset bundles:"), "{printed}");
    assert!(
        printed.contains(&format!(
            "\n  {}",
            out_dir.join("assets/opentrons_ot2").display()
        )),
        "{printed}"
    );
    assert!(
        printed.contains(&format!(
            "Facility solution: {}",
            out_dir.join("compiler/facility-solution.json").display()
        )),
        "{printed}"
    );
    assert!(
        printed.contains(&format!(
            "Adapter invocations: {}",
            out_dir.join("compiler/adapter-invocations.json").display()
        )),
        "{printed}"
    );
    assert!(printed.contains("Automation protocols:"), "{printed}");
    assert!(printed.contains("automation_protocol.py"), "{printed}");
    assert!(printed.contains("Documents:"), "{printed}");
    assert!(printed.contains("manual_protocol.pdf"), "{printed}");

    std::fs::remove_dir_all(out_dir).unwrap();
    std::fs::remove_dir_all(example).unwrap();
}

#[test]
fn the_golden_gate_facility_plan_binds_canonical_pipetting_to_the_ot2() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-plan-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "plan",
            example.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "facility plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let solution = read_json(out_dir.join("compiler/facility-solution.json"));
    assert_eq!(
        solution["facility"],
        "https://example.org/golden-gate/facility"
    );
    assert_eq!(solution["selections"].as_array().unwrap().len(), 10);
    let requirements = solution_requirements(&solution);
    assert_eq!(requirements.len(), 44);
    assert!(requirements.iter().all(|binding| {
        binding["capability_kind"] != "https://sbol.io/ns/capability#LiquidHandling"
    }));
    let pipetting = requirements
        .iter()
        .copied()
        .filter(|binding| {
            matches!(
                binding["capability_kind"].as_str(),
                Some("https://sbol.io/ns/capability#MeteredLiquidTransfer")
                    | Some("https://sbol.io/ns/capability#InWellMixing")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(pipetting.len(), 12);
    assert!(pipetting.iter().all(|binding| {
        binding["asset"] == "https://example.org/golden-gate/opentrons_ot2"
            && binding["adapter"]["driver"] == "opentrons.ot2"
            && binding["adapter"]["procedure_implementation"]
                == "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1"
    }));
    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_serial_dilutions_use_pipetting(
        &invocations,
        "https://example.org/golden-gate/opentrons_ot2",
        "opentrons.ot2",
        "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1",
    );
    assert_golden_gate_uses_thermal_program(
        &invocations,
        "https://example.org/golden-gate/opentrons_ot2",
        "opentrons.ot2",
        "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2ThermalV1",
    );

    let lowering = read_json(out_dir.join("facility_lowering.json"));
    assert_eq!(lowering["schema_version"], "lab.facility-lowering.v2");
    assert_eq!(lowering["inventory_sha256"], solution["inventory_sha256"]);
    assert_eq!(lowering["routes"].as_array().unwrap().len(), 1);
    let route = &lowering["routes"][0];
    assert_eq!(
        route["asset"],
        "https://example.org/golden-gate/opentrons_ot2"
    );
    assert_eq!(route["driver"], "opentrons.ot2");
    assert_eq!(
        route["procedure_implementations"],
        serde_json::json!([
            "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1",
            "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2ThermalV1",
        ])
    );
    assert!(route.get("scope").is_none());
    assert_eq!(route["id"], "opentrons-ot2-5dbf2ae84b40");
    assert_eq!(route["output"], "assets/opentrons_ot2");
    assert_eq!(route["requirements"].as_array().unwrap().len(), 41);
    let protocols = route["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| artifact["role"] == "automation_protocol")
        .collect::<Vec<_>>();
    assert_eq!(protocols.len(), 12);
    assert!(protocols.iter().all(|artifact| {
        artifact["format"] == "opentrons.python-protocol"
            && artifact["sha256"].as_str().unwrap().len() == 64
            && out_dir
                .join(route["output"].as_str().unwrap())
                .join(artifact["path"].as_str().unwrap())
                .is_file()
    }));

    let execution_plan = read_json(out_dir.join("plan.execution.json"));
    assert_eq!(
        execution_plan["planning"]["methods"][0]["method"],
        "https://www.lab-compiler.org/ns/method#temperature-staged-golden-gate"
    );
    assert_eq!(
        execution_plan["planning"]["allocated_lair"]["path"],
        "compiler/allocated.lair"
    );
    let execution_nodes = execution_plan["nodes"].as_array().unwrap();
    assert!(
        execution_nodes
            .iter()
            .filter(|node| node["after"].as_array().is_none_or(Vec::is_empty))
            .count()
            > 1,
        "independent Procedure branches should remain parallel"
    );
    let node_has_requirement = |node: &Value, requirement_fragment: &str| {
        node["requirements"].as_array().is_some_and(|requirements| {
            requirements.iter().any(|requirement| {
                requirement
                    .as_str()
                    .is_some_and(|requirement| requirement.contains(requirement_fragment))
            })
        })
    };
    let node_id = |requirement_fragment: &str| {
        execution_nodes
            .iter()
            .find(|node| node_has_requirement(node, requirement_fragment))
            .unwrap_or_else(|| panic!("missing execution node for {requirement_fragment}"))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let assembly_setup = node_id(
        "std-bio-build-realize-0::https://www.lab-compiler.org/ns/method#temperature-staged-golden-gate::setup-reaction",
    );
    let assembly_cycle = node_id(
        "std-bio-build-realize-0::https://www.lab-compiler.org/ns/method#temperature-staged-golden-gate::cycle-reaction",
    );
    assert_ne!(assembly_setup, assembly_cycle);
    assert!(
        execution_nodes
            .iter()
            .find(|n| n["id"] == assembly_cycle)
            .unwrap()["after"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == &assembly_setup)
    );
    let cell_provisions = (0..1)
        .map(|index| node_id(&format!("std-lab-plasmid-provision-{index}::")))
        .collect::<Vec<_>>();
    let transform = execution_nodes
        .iter()
        .find(|node| node_has_requirement(node, "std-lab-plasmid-transform-0::"))
        .unwrap();
    let transform_dependencies = transform["after"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dependency| dependency.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(transform_dependencies.contains(assembly_cycle.as_str()));
    assert!(
        cell_provisions
            .iter()
            .all(|id| transform_dependencies.contains(id.as_str()))
    );
    for index in [2, 4, 6] {
        assert!(
            transform_dependencies.contains(format!("execute-{index:04}").as_str()),
            "transformation waits for all three completed assemblies"
        );
    }
    assert!(execution_plan.get("lowerings").is_none());
    let reviewed_protocols = execution_nodes
        .iter()
        .filter_map(|node| node.get("document"))
        .collect::<Vec<_>>();
    assert_eq!(reviewed_protocols.len(), 12);
    assert!(reviewed_protocols.iter().all(|document| {
        document["format"] == "opentrons.python-protocol"
            && document["sha256"].as_str().unwrap().len() == 64
            && out_dir.join(document["path"].as_str().unwrap()).is_file()
    }));

    let dry_run = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", out_dir.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "reviewed plan failed preflight: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&dry_run.stdout)
            .contains("Opentrons OT-2 MeteredLiquidTransfer protocol")
    );

    let allocated_lair_path = out_dir.join("compiler/allocated.lair");
    let allocated_lair = std::fs::read(&allocated_lair_path).unwrap();
    std::fs::write(&allocated_lair_path, b"changed after review\n").unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", out_dir.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("SHA-256"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    std::fs::write(&allocated_lair_path, allocated_lair).unwrap();

    let tampered = reviewed_protocols[0]["path"].as_str().unwrap();
    std::fs::write(out_dir.join(tampered), "# changed after review\n").unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", out_dir.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("SHA-256"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );

    std::fs::remove_dir_all(out_dir).unwrap();
}

#[test]
fn a_facility_can_lower_exact_requirements_through_several_assets() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let project = temporary_project();
    copy_dir(&example, &project);

    let manifest_path = project.join("lab.toml");
    let manifest =
        with_portable_manual_method_pins(std::fs::read_to_string(&manifest_path).unwrap());
    let configured_ot2 = r#"[[execution.adapters]]
asset = "https://example.org/golden-gate/opentrons_ot2"
driver = "opentrons.ot2"
profile = "adapters/opentrons-ot2.toml""#;
    assert!(manifest.contains(configured_ot2));
    let star_and_odtc_bindings = r#"[[execution.adapters]]
asset = "https://example.org/golden-gate/hamilton_star"
driver = "hamilton.star"
profile = "adapters/hamilton-star.toml"

[[execution.adapters]]
asset = "https://example.org/golden-gate/inheco_odtc"
driver = "inheco.odtc"
profile = "adapters/inheco-odtc.toml""#;
    std::fs::write(
        &manifest_path,
        manifest.replace(configured_ot2, star_and_odtc_bindings),
    )
    .unwrap();
    std::fs::write(
        project.join("adapters/hamilton-star.toml"),
        include_str!("../../../examples/contributing/hamilton-profile.toml"),
    )
    .unwrap();
    std::fs::write(project.join("adapters/inheco-odtc.toml"), "").unwrap();

    let inventory_path = project.join("inventory/facility.ttl");
    let inventory = std::fs::read_to_string(&inventory_path)
        .unwrap()
        .replace("opentrons_ot2", "hamilton_star")
        .replace(
            "Opentrons OT-2 with Thermocycler Module",
            "Hamilton STAR liquid handler",
        )
        .replace("OT-2 with Thermocycler Module GEN1", "STAR")
        .replace(
            "hamilton_star_programmed_block_temperature_control",
            "inheco_odtc_programmed_block_temperature_control",
        )
        .replace(
            "hamilton_star_heated_lid_temperature_control",
            "inheco_odtc_heated_lid_temperature_control",
        )
        .replace("hamilton_star_minimum_block_temperature", "inheco_odtc_minimum_block_temperature")
        .replace("hamilton_star_maximum_block_temperature", "inheco_odtc_maximum_block_temperature")
        .replace("hamilton_star_maximum_sample_count", "inheco_odtc_maximum_sample_count")
        .replace("hamilton_star_minimum_thermal_sample_volume", "inheco_odtc_minimum_thermal_sample_volume")
        .replace("hamilton_star_maximum_thermal_sample_volume", "inheco_odtc_maximum_thermal_sample_volume")
        .replace("hamilton_star_minimum_lid_temperature", "inheco_odtc_minimum_lid_temperature")
        .replace("hamilton_star_maximum_lid_temperature", "inheco_odtc_maximum_lid_temperature")
        .replace(
            "fac:capabilityKind cap:ProgrammedBlockTemperatureControl ;\n    fac:qualification fac:Plannable ;\n    fac:controlMode fac:ReviewedFileControl ;",
            "fac:capabilityKind cap:ProgrammedBlockTemperatureControl ;\n    fac:qualification fac:Plannable ;\n    fac:controlMode fac:SiLA2Control ;",
        )
        .replace(
            "fac:capabilityKind cap:HeatedLidTemperatureControl ;\n    fac:qualification fac:Plannable ;\n    fac:controlMode fac:ReviewedFileControl ;",
            "fac:capabilityKind cap:HeatedLidTemperatureControl ;\n    fac:qualification fac:Plannable ;\n    fac:controlMode fac:SiLA2Control ;",
        );
    let combined_offerings = r#"    fac:capability ex:hamilton_star_metered_liquid_transfer,
        ex:hamilton_star_in_well_mixing,
        ex:hamilton_star_temperature_controlled_staging,
        ex:hamilton_star_liquid_level_aware_aspiration,
        ex:hamilton_star_vessel_relative_liquid_access,
        ex:hamilton_star_air_gap_handling,
        ex:hamilton_star_post_dispense_blowout,
        ex:hamilton_star_touch_tip,
        ex:inheco_odtc_programmed_block_temperature_control,
        ex:inheco_odtc_heated_lid_temperature_control ."#;
    assert!(inventory.contains(combined_offerings));
    let split_assets = r#"    fac:capability ex:hamilton_star_metered_liquid_transfer,
        ex:hamilton_star_in_well_mixing,
        ex:hamilton_star_temperature_controlled_staging,
        ex:hamilton_star_liquid_level_aware_aspiration,
        ex:hamilton_star_vessel_relative_liquid_access,
        ex:hamilton_star_air_gap_handling,
        ex:hamilton_star_post_dispense_blowout,
        ex:hamilton_star_touch_tip .

ex:inheco_odtc
    a sbol:TopLevel, fac:Asset ;
    sbol:displayId "inheco_odtc" ;
    sbol:hasNamespace <https://example.org/golden-gate> ;
    sbol:name "Inheco ODTC thermocycler" ;
    fac:assetKind fac:Instrument ;
    fac:locatedIn ex:automation_bench ;
    fac:isActive true ;
    fac:capability ex:inheco_odtc_programmed_block_temperature_control,
        ex:inheco_odtc_heated_lid_temperature_control ."#;
    std::fs::write(
        &inventory_path,
        inventory.replace(combined_offerings, split_assets),
    )
    .unwrap();

    let out_dir = project.join("review");
    let built = run(&[
        "build",
        project.to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        built.status.success(),
        "multi-Asset facility build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    let lowering = read_json(out_dir.join("facility_lowering.json"));
    assert_eq!(lowering["schema_version"], "lab.facility-lowering.v2");
    let routes = lowering["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 2);
    assert!(routes.iter().all(|route| route.get("scope").is_none()));
    let drivers = routes
        .iter()
        .map(|route| route["driver"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        drivers,
        std::collections::BTreeSet::from(["hamilton.star", "inheco.odtc"])
    );
    let assets = routes
        .iter()
        .map(|route| route["asset"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        assets,
        std::collections::BTreeSet::from([
            "https://example.org/golden-gate/hamilton_star",
            "https://example.org/golden-gate/inheco_odtc",
        ])
    );
    let lowered_requirements = routes
        .iter()
        .map(|route| route["requirements"].as_array().unwrap().len())
        .sum::<usize>();
    let automation_artifacts = routes
        .iter()
        .flat_map(|route| route["artifacts"].as_array().unwrap())
        .filter(|artifact| artifact["role"] == "automation_protocol")
        .collect::<Vec<_>>();
    assert_eq!(lowered_requirements, 15);
    assert_eq!(automation_artifacts.len(), 7);
    assert!(automation_artifacts.iter().all(|artifact| {
        matches!(
            artifact["format"].as_str(),
            Some("lab.star-run.v0" | "lab.thermocycle-run.v1")
        ) && artifact["sha256"].as_str().unwrap().len() == 64
    }));

    let plan = read_json(out_dir.join("plan.execution.json"));
    assert!(plan.get("lowerings").is_none());
    assert_eq!(
        plan["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["action"] == "execute")
            .filter_map(|node| node["requirements"].as_array())
            .map(Vec::len)
            .sum::<usize>(),
        lowered_requirements
    );
    let reviewed_documents = plan["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node.get("document"))
        .collect::<Vec<_>>();
    assert_eq!(reviewed_documents.len(), automation_artifacts.len());
    let mut star_documents = 0;
    let mut thermocycle_documents = 0;
    for document in reviewed_documents {
        let path = document["path"].as_str().unwrap();
        match document["format"].as_str().unwrap() {
            "lab.star-run.v0" => {
                star_documents += 1;
                assert!(path.starts_with("assets/hamilton_star/"));
                let run = read_json(out_dir.join(path));
                assert_eq!(run["format"], "lab.star-run.v0");
                assert!(!run["steps"].as_array().unwrap().is_empty());
                let manifest = read_json(
                    out_dir
                        .join(path)
                        .with_file_name("invocation_manifest.json"),
                );
                assert_eq!(
                    manifest["liquid_class_library"]["id"],
                    "org.example.hamilton.liquid-classes"
                );
                assert_eq!(manifest["liquid_class_library"]["version"], "1.0.0");
                let classes = manifest["liquid_classes"].as_array().unwrap();
                assert!(!classes.is_empty());
                let run_text = serde_json::to_string(&run).unwrap();
                for class in classes {
                    assert!(
                        class["identity"]["id"]
                            .as_str()
                            .unwrap()
                            .starts_with("org.example.hamilton.")
                    );
                    assert_eq!(class["speeds"]["aspirate_ul_s"], serde_json::json!(17.0));
                    assert!(
                        run_text.contains(class["identity"]["content_sha256"].as_str().unwrap())
                    );
                }
            }
            "lab.thermocycle-run.v1" => {
                thermocycle_documents += 1;
                assert!(path.starts_with("assets/inheco_odtc/"));
                let document = read_json(out_dir.join(path));
                assert_eq!(document["format"], "lab.thermocycle-run.v1");
                let run = &document["run"];
                match run["profile"]["stages"][0]["repeats"].as_u64() {
                    Some(75) => {
                        assert_eq!(run["sample_count"], 1);
                        assert_eq!(run["fill_volume_ul"], 25.0);
                    }
                    Some(1) => {
                        assert_eq!(run["sample_count"], 3);
                        assert!(matches!(run["fill_volume_ul"].as_f64(), Some(35.0 | 95.0)));
                    }
                    repeats => panic!("unexpected thermocycler repeat count: {repeats:?}"),
                }
            }
            format => panic!("unexpected invocation document format: {format}"),
        }
        assert!(out_dir.join(path).is_file());
    }
    assert!(star_documents > 0);
    assert!(thermocycle_documents > 0);

    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_serial_dilutions_use_pipetting(
        &invocations,
        "https://example.org/golden-gate/hamilton_star",
        "hamilton.star",
        "https://www.lab-compiler.org/ns/adapter-implementation#HamiltonStarPipettingV1",
    );
    assert_golden_gate_uses_thermal_program(
        &invocations,
        "https://example.org/golden-gate/inheco_odtc",
        "inheco.odtc",
        "https://www.lab-compiler.org/ns/adapter-implementation#InhecoOdtcThermalV1",
    );

    let star_route = routes
        .iter()
        .find(|route| route["driver"] == "hamilton.star")
        .unwrap();
    let star_output = out_dir.join(star_route["output"].as_str().unwrap());
    // Three biological replicates through two dilutions each. Reading only the dilution count would
    // silently emit half the experiment and report n=1.
    let star_manifests = task_manifests(&star_output);
    let star_dilution = operation(&star_manifests, "SeriallyDiluteCulture")[0];
    let vessels = star_dilution["execution"]["program"]["vessels"]
        .as_array()
        .unwrap();
    assert_eq!(
        vessels
            .iter()
            .find(|v| v["role"]["kind"] == "procedure_input")
            .unwrap()["positions"],
        3
    );
    assert_eq!(
        vessels
            .iter()
            .find(|v| v["role"]["kind"] == "product")
            .unwrap()["positions"],
        6
    );
    for manifest in star_route["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| {
            artifact["path"]
                .as_str()
                .unwrap()
                .ends_with("invocation_manifest.json")
        })
    {
        let contents =
            std::fs::read_to_string(star_output.join(manifest["path"].as_str().unwrap())).unwrap();
        assert!(!contents.contains("transformation"));
        assert!(!contents.contains("agar_plate"));
    }

    let dry_run = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", out_dir.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "multi-Asset reviewed plan failed preflight: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("through hamilton.star"));
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("through inheco.odtc"));

    let result: Value = serde_json::from_slice(&built.stdout).unwrap();
    assert_eq!(
        result["result"]["facility"]["protocols"]
            .as_array()
            .unwrap()
            .len(),
        automation_artifacts.len()
    );
    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn the_extended_golden_gate_example_uses_exact_material_lots_and_the_ot2() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate-extended")
        .canonicalize()
        .unwrap();
    let output_root = temporary_project();
    let plan_dir = output_root.join("plan");

    let planned = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "plan",
            example.to_str().unwrap(),
            "--out-dir",
            plan_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "extended Golden Gate facility plan failed: {}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let result: Value = serde_json::from_slice(&planned.stdout).unwrap();
    assert_eq!(result["result"]["protocols"].as_array().unwrap().len(), 28);

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(plan_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    assert_eq!(lowering["routes"].as_array().unwrap().len(), 1);
    let route = &lowering["routes"][0];
    assert!(route.get("scope").is_none());
    let invocations = read_json(plan_dir.join("compiler/adapter-invocations.json"));
    assert_eq!(
        invocations["facility"],
        "https://example.org/golden-gate/facility"
    );
    assert!(invocations.get("material_inventory").is_none());
    let reference_binding = invocations["methods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|method| method["tasks"].as_array().unwrap())
        .flat_map(|task| {
            task.get("materials")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|binding| binding["symbol"] == "Addgene-#134516")
        .unwrap();
    assert_eq!(
        reference_binding["source"]["component"],
        "https://example.org/golden-gate/materials/reference_gfp"
    );
    assert_eq!(
        reference_binding["source"]["material_lot"],
        "https://example.org/golden-gate/lots/reference_gfp_lot"
    );

    let solution = read_json(plan_dir.join("compiler/facility-solution.json"));
    assert_eq!(solution["selections"].as_array().unwrap().len(), 32);
    let materials = solution_materials(&solution);
    let reference_input = materials
        .iter()
        .copied()
        .find(|binding| binding["symbol"] == "Addgene-#134516")
        .expect("the global facility solution allocates the external reference plasmid");
    assert_eq!(
        reference_input["source"]["component"],
        "https://example.org/golden-gate/materials/reference_gfp"
    );
    assert_eq!(
        reference_input["source"]["material_lot"],
        "https://example.org/golden-gate/lots/reference_gfp_lot"
    );
    assert!(materials.iter().any(|binding| {
        binding["symbol"] == "composite_plasmid_1" && binding["source"]["kind"] == "choice_output"
    }));
    let requirements = solution_requirements(&solution);
    assert_eq!(requirements.len(), 98);
    assert!(requirements.iter().all(|binding| {
        binding["capability_kind"] != "https://sbol.io/ns/capability#LiquidHandling"
    }));
    let pipetting = requirements
        .iter()
        .copied()
        .filter(|binding| {
            matches!(
                binding["capability_kind"].as_str(),
                Some("https://sbol.io/ns/capability#MeteredLiquidTransfer")
                    | Some("https://sbol.io/ns/capability#InWellMixing")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(pipetting.len(), 28);
    assert!(pipetting.iter().all(|binding| {
        binding["asset"] == "https://example.org/golden-gate/opentrons_ot2"
            && binding["adapter"]["driver"] == "opentrons.ot2"
    }));

    let execution = read_json(plan_dir.join("plan.execution.json"));
    assert_eq!(execution["format"], "lab.execution-plan.v4");
    assert!(
        execution["materials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|binding| {
                binding["component"] == "https://example.org/golden-gate/materials/reference_gfp"
                    && binding["material_lot"]
                        == "https://example.org/golden-gate/lots/reference_gfp_lot"
            })
    );

    let dry_run = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", plan_dir.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "extended Golden Gate plan failed preflight: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );

    std::fs::remove_dir_all(output_root).unwrap();
}

#[test]
fn an_ot2_setup_transfers_dependency_dna_from_an_earlier_choice() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate-extended")
        .canonicalize()
        .unwrap();
    let project = temporary_project();
    copy_dir(&example, &project);

    let assemble_path = project.join("src/workflows/assemble.lab");
    let assemble = std::fs::read_to_string(&assemble_path).unwrap().replace(
        "workflow assemble_composite_plasmid_2() -> Material<Plasmid>:\n  product <- realize composite_plasmid_2",
        "workflow assemble_composite_plasmid_2(\n  composite_plasmid_1: Material<Plasmid>,\n) -> Material<Plasmid>:\n  dependencies = [composite_plasmid_1]\n  product <- realize composite_plasmid_2 from dependencies",
    );
    std::fs::write(assemble_path, assemble).unwrap();

    for entry in ["build_panel.lab", "panel.lab"] {
        let panel_path = project.join("src/programs").join(entry);
        let panel = std::fs::read_to_string(&panel_path).unwrap().replace(
        "  composite_plasmid_1 <- assemble_composite_plasmid_1\n  composite_plasmid_2 <- assemble_composite_plasmid_2\n\n  // One plasmid goes into two chassis. Material is affine, so a transformation\n  // consumes an aliquot of its own rather than the same value twice.\n  for_cloning, for_expression <- split composite_plasmid_1",
        "  composite_plasmid_1 <- assemble_composite_plasmid_1\n\n  // One plasmid supplies a later assembly and two transformations.\n  for_assembly, for_transformations <- split composite_plasmid_1\n  for_cloning, for_expression <- split for_transformations\n  composite_plasmid_2 <- assemble_composite_plasmid_2 for_assembly",
    );
        std::fs::write(panel_path, panel).unwrap();
    }

    let out_dir = project.join("review");
    let planned = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "plan",
            project.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "dependent Golden Gate facility plan failed: {}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let lowering = read_json(out_dir.join("facility_lowering.json"));
    let route = &lowering["routes"][0];
    let target_root = out_dir.join(route["output"].as_str().unwrap());
    let manifests = task_manifests(&target_root);
    let dependency_transfer = operation(&manifests, "SetupGoldenGateReaction")
        .iter()
        .any(|m| {
            m["execution"]["sources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|source| {
                    source["binding"]["symbol"] == "composite_plasmid_1"
                        && source["binding"]["source"]["kind"] == "choice_output"
                })
        });
    assert!(
        dependency_transfer,
        "an OT-2 setup invocation must transfer DNA produced by an earlier choice"
    );

    std::fs::remove_dir_all(project).unwrap();
}

/// A different facility Asset and exact adapter binding lower the same experiment for a Flex
/// without a source edit or an independent device selector.
#[test]
fn a_facility_binding_selects_the_flex_adapter_and_protocol_format() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let project = temporary_project();
    copy_dir(&example, &project);
    let inventory_path = project.join("inventory/facility.ttl");
    let inventory = std::fs::read_to_string(&inventory_path)
        .unwrap()
        .replace("opentrons_ot2", "opentrons_flex")
        .replace("Opentrons OT-2", "Opentrons Flex")
        .replace(
            "OT-2 with Thermocycler Module GEN1",
            "Flex with Thermocycler Module Gen2",
        );
    std::fs::write(inventory_path, inventory).unwrap();
    let manifest_path = project.join("lab.toml");
    let manifest = with_portable_manual_method_pins(
        std::fs::read_to_string(&manifest_path)
            .unwrap()
            .replace("opentrons_ot2", "opentrons_flex")
            .replace("opentrons.ot2", "opentrons.flex")
            .replace("opentrons-ot2.toml", "opentrons-flex.toml"),
    );
    std::fs::write(manifest_path, manifest).unwrap();
    std::fs::write(project.join("adapters/opentrons-flex.toml"), "").unwrap();
    let out_dir = project.join("review");

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "plan",
            project.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Flex facility plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let protocols = result["result"]["protocols"].as_array().unwrap();
    assert_eq!(protocols.len(), 7);
    assert!(
        protocols
            .iter()
            .all(|path| path.as_str().unwrap().ends_with("automation_protocol.json")),
        "the allocated Flex adapter emits one JSON protocol per exact Procedure task: {protocols:?}"
    );

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    let route = &lowering["routes"][0];
    assert_eq!(
        route["asset"],
        "https://example.org/golden-gate/opentrons_flex"
    );
    assert_eq!(route["driver"], "opentrons.flex");
    assert!(route.get("scope").is_none());
    assert_eq!(route["requirements"].as_array().unwrap().len(), 15);
    let target_root = out_dir.join(route["output"].as_str().unwrap());
    assert!(
        target_root
            .join("tasks/001-pipetting-program/automation_protocol.json")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/002-thermal-program/automation_protocol.json")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/007-pipetting-program/automation_protocol.json")
            .is_file()
    );
    assert!(
        walk_files(&target_root).iter().all(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            !name.contains("transformation_protocol") && !name.contains("plating_protocol")
        }),
        "an exact Flex dilution must not absorb transformation or plating"
    );

    // The example runs three biological replicates through two dilutions each. An adapter that
    // reads only the dilution count would emit half the experiment and silently report n=1.
    let dilution: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("tasks/007-pipetting-program/invocation_manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let execution = &dilution["execution"];
    let vessels = execution["program"]["vessels"].as_array().unwrap();
    let culture = vessels
        .iter()
        .find(|v| v["role"]["kind"] == "procedure_input")
        .unwrap();
    assert_eq!(
        culture["positions"], 3,
        "every biological replicate is staged"
    );
    assert_eq!(
        vessels
            .iter()
            .filter(|v| v["role"]["kind"] == "product")
            .map(|v| v["positions"].as_u64().unwrap())
            .sum::<u64>(),
        6
    );
    assert_eq!(
        execution["program"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "mix")
            .count(),
        6
    );

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("tasks/001-pipetting-program/invocation_manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["adapter"], "opentrons.flex");
    assert_eq!(
        manifest["deck"]["resources"]["small_tips"]["slots"],
        serde_json::json!(["C2"]),
        "the emitted plan carries the allocated adapter's physical resources"
    );

    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_serial_dilutions_use_pipetting(
        &invocations,
        "https://example.org/golden-gate/opentrons_flex",
        "opentrons.flex",
        "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsFlexPipettingV1",
    );
    assert_golden_gate_uses_thermal_program(
        &invocations,
        "https://example.org/golden-gate/opentrons_flex",
        "opentrons.flex",
        "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsFlexThermalV1",
    );

    let protocol: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("tasks/001-pipetting-program/automation_protocol.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(protocol["schemaVersion"], 8);
    assert_eq!(protocol["robot"]["model"], "OT-3 Standard");
    assert!(!protocol["commands"].as_array().unwrap().is_empty());

    let execution = read_json(out_dir.join("plan.execution.json"));
    assert!(execution.get("lowerings").is_none());
    let flex_documents = execution["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["document"]["format"] == "opentrons.protocol-designer-json")
        .map(|node| &node["document"])
        .collect::<Vec<_>>();
    assert_eq!(flex_documents.len(), 7);
    assert!(flex_documents.iter().all(|document| {
        document["format"] == "opentrons.protocol-designer-json"
            && out_dir.join(document["path"].as_str().unwrap()).is_file()
    }));

    let dry_run = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", out_dir.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "the exact Flex documents must pass generic runtime preflight: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );

    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn checking_one_file_underlines_the_source_rather_than_naming_byte_offsets() {
    let source = temporary_project().with_extension("lab");
    std::fs::write(
        &source,
        "workflow grow() -> Integer:\n  return 1\n\nworkflow grow() -> Integer:\n  return 2\n",
    )
    .unwrap();

    let output = run(&["check", source.to_str().unwrap()]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("duplicate declaration 'grow'"),
        "the headline names the mistake:\n{stderr}"
    );
    assert!(
        stderr.contains("4 | workflow grow() -> Integer:") && stderr.contains("  |          ^^^^"),
        "the offending line is excerpted and underlined:\n{stderr}"
    );
    assert!(
        stderr.contains("'grow' is already declared here"),
        "the first declaration is shown as well:\n{stderr}"
    );
    assert!(
        !stderr.contains("at bytes"),
        "byte offsets are not a source location:\n{stderr}"
    );

    std::fs::remove_file(&source).unwrap();
}
