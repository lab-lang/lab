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
    assert_eq!(tasks.len(), 4);
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
    assert_eq!(tasks.len(), 2);
    for task in tasks {
        assert_eq!(
            task["program"]["contract"],
            "https://www.lab-compiler.org/ns/procedure-contract#ThermalProgramV1"
        );
        let program = &task["program"]["body"];
        assert_eq!(program["load"]["input"], 0);
        assert_eq!(program["load"]["outputs"], serde_json::json!(["product"]));
        assert_eq!(program["load"]["volume_each"]["value"]["value"], "20");
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

    let created = run(&["new", &project_text, "--name", "test-project"]);
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
    assert_eq!(index["schema_version"], 7);
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
    assert_eq!(problem["schema_version"], "lab.planning-problem.v6");
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
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
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
    assert_eq!(plan["format"], "lab.execution-plan.v6");
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
        plan["nodes"][0]["requirement"],
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
    let created = run(&["new", &project_text]);
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
    let created = run(&["new", &project_text]);
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
    let created = run(&["new", &project_text]);
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
        include_str!("fixtures/minimal-inventory.ttl"),
        r#"ex:operator a sbol:TopLevel, fac:Asset ;
    sbol:displayId "operator" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ;
    fac:facility ex:facility ;
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
    assert_eq!(index["schema_version"], 7);
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
    assert_eq!(bindings["schema_version"], "lab.adapter-bindings.v3");
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

    // The emitted protocol must draw the aliquot from the labware the temperature module holds,
    // not from an ambient rack standing beside it.
    let protocol =
        read_text(project.join(".lab/build/assets/opentrons_ot2/transformation_protocol.py"));
    assert!(protocol.contains("cell_rack = temperature.load_labware("));
    assert!(protocol.contains("source = cell_rack[source_name]"));
    assert!(
        protocol.contains("temperature.set_temperature(execution[\"cell_staging_temperature_c\"])"),
        "the setpoint comes from the reviewed plan rather than a literal in the template"
    );

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
        message.contains("composite_plasmid_1")
            && message.contains("`restriction_enzyme`")
            && message.contains("build it by another method"),
        "the diagnostic names the artifact, the missing property, and the alternative: {message}"
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
    assert!(!out_dir.join("lowerings").exists());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "planned");
    // Planning names the three reviewed device runs produced by the allocated OT-2 schedule.
    let protocols = result["result"]["protocols"].as_array().unwrap();
    assert_eq!(protocols.len(), 3);
    assert_eq!(
        protocols
            .iter()
            .map(|path| Path::new(path.as_str().unwrap()).file_name().unwrap())
            .collect::<Vec<_>>(),
        [
            "assembly_protocol.py",
            "plating_protocol.py",
            "transformation_protocol.py",
        ]
    );
    let human = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "plan",
            example.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let printed = String::from_utf8(human.stdout).unwrap();
    assert!(printed.contains("Automation protocols:"), "{printed}");
    assert!(printed.contains("assembly_protocol.py"), "{printed}");
    assert!(printed.contains("transformation_protocol.py"), "{printed}");
    assert!(printed.contains("plating_protocol.py"), "{printed}");

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    let target_root = out_dir.join(lowering["routes"][0]["output"].as_str().unwrap());
    assert!(lowering["routes"][0].get("scope").is_none());
    for path in [
        "assembly_protocol.py",
        "transformation_protocol.py",
        "plating_protocol.py",
        "execution_schedule.json",
        "assembly_manifest.json",
        "transformation_manifest.json",
        "plating_manifest.json",
        "plate_map.json",
        "plate_map.pdf",
    ] {
        assert!(target_root.join(path).is_file(), "missing {path}");
    }

    let schedule = read_json(target_root.join("execution_schedule.json"));
    assert_eq!(
        schedule["schema_version"],
        "lab.allocated-procedure-schedule.v1"
    );
    assert_eq!(
        schedule["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["assembly", "transformation", "plating"]
    );
    assert_eq!(
        schedule["groups"][1]["after"],
        serde_json::json!(["assembly"])
    );
    assert_eq!(
        schedule["groups"][2]["after"],
        serde_json::json!(["transformation"])
    );

    let manifest = read_json(target_root.join("assembly_manifest.json"));
    assert_eq!(manifest["schema_version"], "lab.opentrons-ot2-run.v2");
    assert_eq!(manifest["group"]["id"], "assembly");
    assert_eq!(manifest["execution"]["kind"], "assembly");
    assert_eq!(manifest["execution"]["setups"].as_array().unwrap().len(), 2);
    assert_eq!(
        manifest["deck"]["deck"]["thermocycler"]["model"], "thermocycler module",
        "the example's exact OT-2 Asset has a Thermocycler Module GEN1"
    );
    assert_eq!(
        manifest["deck"]["deck"]["thermocycler"]["model"],
        regression["hardware"]["thermocycler_load_name"]
    );
    assert!(
        manifest["execution"]["setups"][0]["execution"]["additions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|addition| addition["source"]["kind"] == "material_lot")
    );
    let setup = &manifest["execution"]["setups"][0]["execution"];
    assert_eq!(
        setup["reaction_wells"][0],
        regression["lineage_exemplar"]["assembly_product_well"]
    );
    assert_eq!(
        number(&setup["source_temperature_c"]),
        number(&regression["assembly"]["source_temperature_c"])
    );
    assert_eq!(
        number(&manifest["deck"]["techniques"]["aspiration_rate"]),
        number(&regression["assembly"]["transfer"]["aspiration_rate"])
    );
    assert_eq!(
        number(&manifest["deck"]["techniques"]["dispense_rate"]),
        number(&regression["assembly"]["transfer"]["dispense_rate"])
    );
    let additions = setup["additions"].as_array().unwrap();
    let mut source_order = Vec::new();
    for role in additions
        .iter()
        .map(|addition| addition["role"].as_str().unwrap())
    {
        if source_order.last().copied() != Some(role) {
            source_order.push(role);
        }
    }
    assert_eq!(
        source_order,
        regression["assembly"]["source_order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|role| role.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(additions[0]["role"], "water");
    assert!(additions[0].get("source_mix").is_none());
    assert!(additions.iter().all(|addition| {
        addition["transfer_technique"]["blow_out"] == regression["assembly"]["transfer"]["blow_out"]
            && addition["transfer_technique"]["touch_tip"]
                == regression["assembly"]["transfer"]["touch_tip"]
    }));
    assert!(additions[1..].iter().all(|addition| {
        addition["source_mix"]["cycles"] == regression["assembly"]["source_mix"]["cycles"]
            && addition["source_mix"]["volume_ul"] == addition["volume_ul"]
    }));
    assert!(
        additions[..additions.len() - 1]
            .iter()
            .all(|addition| addition["reuse_tip_for_final_mix"] == false)
    );
    assert_eq!(
        additions.last().unwrap()["reuse_tip_for_final_mix"],
        regression["assembly"]["bubble_clear"]["reuse_final_addition_tip"]
    );
    assert_eq!(
        setup["final_mix"]["cycles"],
        regression["assembly"]["bubble_clear"]["cycles"]
    );
    assert_eq!(
        setup["final_mix"]["volume_ul"],
        regression["assembly"]["bubble_clear"]["volume_ul"]
    );
    assert_eq!(
        setup["final_mix"]["technique"]["aspiration"]["offset"]["value"]["value"],
        regression["assembly"]["bubble_clear"]["aspiration_bottom_offset_mm"]
            .as_i64()
            .unwrap()
            .to_string()
    );
    assert_eq!(
        setup["final_mix"]["technique"]["dispense"]["offset"]["value"]["value"],
        regression["assembly"]["bubble_clear"]["dispense_bottom_offset_mm"]
            .as_i64()
            .unwrap()
            .to_string()
    );
    assert_eq!(
        setup["final_mix"]["technique"]["blow_out"],
        regression["assembly"]["bubble_clear"]["blow_out_each_cycle"]
    );
    assert_eq!(
        setup["final_mix"]["technique"]["touch_tip"],
        regression["assembly"]["bubble_clear"]["touch_tip_each_cycle"]
    );
    let thermal = &manifest["execution"]["thermal_programs"][0]["execution"];
    assert_eq!(
        number(&thermal["lid_temperature_c"]),
        number(&regression["assembly"]["thermal"]["lid_temperature_c"])
    );
    assert_eq!(
        thermal["profile"]["stages"][0]["repeats"],
        regression["assembly"]["thermal"]["stages"][0]["repeats"]
    );
    for stage in 0..2 {
        for step in 0..2 {
            assert_eq!(
                number(&thermal["profile"]["stages"][stage]["steps"][step]["celsius"]),
                number(
                    &regression["assembly"]["thermal"]["stages"][stage]["steps"][step]["temperature_c"]
                )
            );
            assert_eq!(
                number(&thermal["profile"]["stages"][stage]["steps"][step]["hold_seconds"]),
                number(
                    &regression["assembly"]["thermal"]["stages"][stage]["steps"][step]["hold_seconds"]
                )
            );
        }
    }
    assert_eq!(
        number(&thermal["final_hold_celsius"]),
        number(&regression["assembly"]["thermal"]["final_hold_c"])
    );
    let assembly_protocol = read_text(target_root.join("assembly_protocol.py"));
    assert!(
        assembly_protocol
            .contains("temperature.set_temperature(execution[\"source_temperature_c\"]"),
        "{assembly_protocol}"
    );
    assert_eq!(
        assembly_protocol
            .matches("temperature.set_temperature(execution[\"source_temperature_c\"]")
            .count(),
        1,
        "the shared staging setpoint is programmed once per batch"
    );
    assert!(
        assembly_protocol.contains("_execute_mix(pipette, source, source_mix, techniques)"),
        "{assembly_protocol}"
    );
    let deactivate_sources = assembly_protocol
        .find("temperature.deactivate()")
        .expect("source staging ends before cycling");
    let execute_thermal = assembly_protocol
        .rfind("_execute_thermal_program(thermocycler, thermal)")
        .expect("the authored thermal program is rendered");
    assert!(deactivate_sources < execute_thermal);
    assert_eq!(
        manifest["deck"]["stages"]["plating"]["agar_plate"]["slots"],
        serde_json::json!(["5"]),
        "the allocated adapter emits the concrete deck plan"
    );
    let transformation = read_json(target_root.join("transformation_manifest.json"));
    assert_eq!(transformation["execution"]["kind"], "transformation");
    assert_eq!(
        transformation["execution"]["preparations"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let first_preparation = &transformation["execution"]["preparations"][0]["execution"];
    assert_eq!(
        first_preparation["cell_volume_ul"],
        regression["transformation"]["competent_cells"]["volume_ul"]
    );
    assert_eq!(
        first_preparation["cell_mix_cycles"],
        regression["transformation"]["competent_cells"]["source_mix_cycles"]
    );
    assert_eq!(
        first_preparation["cell_mix_volume_ul"],
        regression["transformation"]["competent_cells"]["source_mix_volume_ul"]
    );
    assert_eq!(first_preparation["cell_source_volume_ul"], 80);
    assert_eq!(
        first_preparation["dna_volume_ul"],
        regression["transformation"]["dna"]["volume_ul"]
    );
    assert_eq!(
        first_preparation["dna_mix_cycles"],
        regression["transformation"]["dna"]["source_mix_cycles"]
    );
    assert_eq!(
        first_preparation["dna_mix_volume_ul"],
        regression["transformation"]["dna"]["source_mix_volume_ul"]
    );
    assert_eq!(
        first_preparation["dna_transfer_technique"]["blow_out"],
        regression["transformation"]["dna"]["transfer_blow_out"]
    );
    assert_eq!(
        first_preparation["bubble_clear_cycles"],
        regression["transformation"]["dna"]["bubble_clear_cycles"]
    );
    assert_eq!(
        first_preparation["bubble_clear_volume_ul"],
        regression["transformation"]["dna"]["bubble_clear_volume_ul"]
    );
    assert_eq!(
        first_preparation["bubble_clear_technique"]["dispense"]["offset"]["value"]["value"],
        "8"
    );
    assert_eq!(
        first_preparation["bubble_clear_technique"]["touch_tip"],
        true
    );
    assert_eq!(
        first_preparation["bubble_clear_technique"]["blow_out"],
        false
    );
    assert_eq!(
        first_preparation["reaction_wells"],
        regression["lineage_exemplar"]["transformation_wells"]
    );
    assert_eq!(
        first_preparation["dna"][0]["source_well"],
        manifest["execution"]["setups"][0]["execution"]["reaction_wells"][0],
        "the assembly product well becomes the exact transformation DNA source"
    );
    let transformation_protocol = read_text(target_root.join("transformation_protocol.py"));
    assert!(
        transformation_protocol.contains("disposal_volume=0"),
        "{transformation_protocol}"
    );
    assert!(
        transformation_protocol.contains("for _ in range(preparation[\"bubble_clear_cycles\"]):"),
        "{transformation_protocol}"
    );
    assert!(
        transformation_protocol.contains("radius=techniques[\"touch_tip_radius\"]"),
        "{transformation_protocol}"
    );
    let transfer_finish = transformation_protocol
        .find("preparation[\"dna_transfer_technique\"]")
        .expect("DNA transfer technique is rendered");
    let bubble_loop = transformation_protocol
        .find("for _ in range(preparation[\"bubble_clear_cycles\"]):")
        .expect("bubble clearing is rendered");
    let final_touch = transformation_protocol
        .rfind("preparation[\"bubble_clear_technique\"]")
        .expect("the post-bubble touch is rendered");
    assert!(transfer_finish < bubble_loop && bubble_loop < final_touch);
    let heat_shock = &transformation["execution"]["heat_shocks"][0]["execution"];
    assert_eq!(
        number(&heat_shock["volume_each_ul"]),
        number(&regression["transformation"]["heat_shock"]["volume_ul"])
    );
    assert_eq!(
        number(&heat_shock["profile"]["stages"][0]["steps"][1]["celsius"]),
        number(&regression["transformation"]["heat_shock"]["steps"][1]["temperature_c"])
    );
    assert!(
        transformation_protocol.contains("block_max_volume=execution[\"volume_each_ul\"]"),
        "{transformation_protocol}"
    );
    assert!(
        !transformation_protocol.contains("is_simulating"),
        "thermal work may not disappear during simulation"
    );
    let recovery_medium = &transformation["execution"]["recovery_additions"][0]["execution"];
    assert_eq!(
        recovery_medium["technique"]["dispense"]["kind"],
        "above_liquid"
    );
    assert_eq!(
        recovery_medium["technique"]["air_gap"]["value"]["value"],
        regression["transformation"]["recovery"]["air_gap_ul"]
            .as_i64()
            .unwrap()
            .to_string()
    );
    let recovery = &transformation["execution"]["recovery_incubations"][0]["execution"];
    assert_eq!(
        number(&recovery["volume_each_ul"]),
        number(&regression["transformation"]["recovery"]["incubation_volume_ul"])
    );

    let plating = read_json(target_root.join("plating_manifest.json"));
    assert_eq!(plating["execution"]["kind"], "plating");
    let dilution = &plating["execution"]["dilutions"][0]["execution"];
    assert_eq!(dilution["kind"], "serial_dilution");
    assert_eq!(
        dilution["medium"]["source"]["material_lot"],
        "https://example.org/golden-gate/lots/recovery_medium_lot"
    );
    assert_eq!(
        dilution["dilution_wells"],
        serde_json::json!([
            {"plate": 0, "well": "A1"},
            {"plate": 0, "well": "B1"},
            {"plate": 0, "well": "A7"},
            {"plate": 0, "well": "B7"}
        ])
    );
    assert_eq!(
        dilution["dilution_wells"],
        serde_json::json!([
            {"plate": 0, "well": regression["lineage_exemplar"]["dilution_1_wells"][0]},
            {"plate": 0, "well": regression["lineage_exemplar"]["dilution_1_wells"][1]},
            {"plate": 0, "well": regression["lineage_exemplar"]["dilution_2_wells"][0]},
            {"plate": 0, "well": regression["lineage_exemplar"]["dilution_2_wells"][1]}
        ])
    );
    assert_eq!(
        dilution["medium_volume_ul"],
        regression["plating"]["medium_volume_ul"]
    );
    assert_eq!(
        dilution["culture_volume_ul"],
        regression["plating"]["culture_volume_ul"]
    );
    assert_eq!(dilution["mix_cycles"], regression["plating"]["mix_cycles"]);
    assert_eq!(
        dilution["mix_volume_ul"],
        regression["plating"]["mix_volume_ul"]
    );
    assert_eq!(
        plating["deck"]["techniques"]["distribution_disposal_volume_ul"],
        regression["plating"]["medium_distribution_disposal_volume_ul"]
    );
    assert_eq!(
        plating["deck"]["techniques"]["tracked_chunk_size"],
        regression["plating"]["medium_distribution_chunk_size"]
    );
    let plating_protocol = read_text(target_root.join("plating_protocol.py"));
    assert!(
        plating_protocol.contains("remaining_ul / source.max_volume"),
        "{plating_protocol}"
    );
    assert!(
        plating_protocol.contains("tracked_low_volume_fraction"),
        "{plating_protocol}"
    );
    // The reviewed plan states the source load, so the run follows that number down. Reading the
    // instrument's own liquid state could put the tip somewhere the reviewer never saw.
    assert!(
        !plating_protocol.contains("current_liquid_volume"),
        "the emitted protocol must not consult live instrument liquid state"
    );
    assert!(
        plating_protocol.contains("remaining_ul -="),
        "the emitted protocol carries the planned volume forward itself"
    );
    assert!(
        plating_protocol.contains("tracked_chunk_size"),
        "{plating_protocol}"
    );
    let seed_second = plating_protocol
        .find("p20.mix(dilution[\"mix_cycles\"], dilution[\"mix_volume_ul\"], dilution_2)")
        .expect("dilution two is seeded and mixed");
    let plate_first = plating_protocol
        .find("for entry in _plate_entries(plating, 1")
        .expect("dilution one is plated");
    let plate_second = plating_protocol
        .find("for entry in _plate_entries(plating, 2")
        .expect("dilution two is plated");
    assert!(seed_second < plate_first && plate_first < plate_second);
    let first_plating = &plating["execution"]["platings"][0]["execution"];
    assert_eq!(first_plating["kind"], "plate_diluted_culture");
    assert_eq!(first_plating["plate_map"].as_array().unwrap().len(), 4);
    assert_eq!(
        first_plating["colony_volume_ul"],
        regression["plating"]["colony_volume_ul"]
    );
    assert_eq!(
        first_plating["technique"]["dispense"]["kind"],
        "material_surface"
    );
    let plate_map = read_json(target_root.join("plate_map.json"));
    assert_eq!(plate_map["schema_version"], "lab.plate-map.v2");
    assert_eq!(plate_map["entries"].as_array().unwrap().len(), 16);
    assert_eq!(plate_map["entries"][0]["subject"], "composite_strain_1");
    assert_eq!(plate_map["entries"][0]["dilution_ratio"], "1/10");
    assert_eq!(plate_map["entries"][2]["dilution_ratio"], "1/100");
    let expected_agar_wells = regression["lineage_exemplar"]["agar_dilution_1_wells"]
        .as_array()
        .unwrap()
        .iter()
        .chain(
            regression["lineage_exemplar"]["agar_dilution_2_wells"]
                .as_array()
                .unwrap(),
        )
        .map(|well| well.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        plate_map["entries"].as_array().unwrap()[..4]
            .iter()
            .map(|entry| entry["destination"]["well"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_agar_wells
    );
    assert!(
        plating_protocol.contains("destination.top(techniques[\"material_surface_offset_mm\"]"),
        "{plating_protocol}"
    );
    assert!(
        plating_protocol.contains("p20.blow_out()"),
        "{plating_protocol}"
    );

    std::fs::remove_dir_all(out_dir).unwrap();
}

#[test]
fn build_emits_facility_selected_protocol_bundles_and_documents() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
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
    assert_eq!(result["result"]["products"].as_array().unwrap().len(), 6);
    assert_eq!(
        result["result"]["products"]
            .as_array()
            .unwrap()
            .iter()
            .map(|product| product["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "composite_plasmid_1",
            "composite_plasmid_2",
            "composite_strain_1",
            "composite_strain_2",
            "composite_strain_3",
            "composite_strain_4",
        ]
    );
    let facility = &result["result"]["facility"];
    assert_eq!(
        facility["facility"],
        "https://example.org/golden-gate/facility"
    );
    assert_eq!(facility["bundles"].as_array().unwrap().len(), 1);
    assert_eq!(facility["protocols"].as_array().unwrap().len(), 3);
    assert_eq!(facility["documents"].as_array().unwrap().len(), 4);
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
    assert_eq!(index["schema_version"], 7);
    assert_eq!(
        index["compiler"]["planning_problem"],
        "compiler/planning-problem.json"
    );
    assert_eq!(
        index["facility"]["facility_solution"],
        "compiler/facility-solution.json"
    );
    assert_eq!(index["facility"]["protocols"].as_array().unwrap().len(), 3);
    assert!(
        index["facility"]["protocols"][0]
            .as_str()
            .unwrap()
            .starts_with("assets/opentrons_ot2/")
    );
    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_eq!(invocations["schema_version"], "lab.adapter-invocations.v8");
    assert_eq!(
        invocations["material_inventory"]["facility"],
        "https://example.org/golden-gate/facility"
    );
    assert_eq!(
        invocations["material_inventory"]["source_sha256"],
        invocations["inventory_sha256"]
    );
    assert_eq!(
        invocations["material_inventory"]["materials"]["J23101"]["component"],
        "https://sbolcanvas.org/J23101"
    );
    assert_eq!(
        invocations["material_inventory"]["materials"]["J23101"]["material_lots"]
            .as_array()
            .unwrap()
            .len(),
        1
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
                .is_some_and(|path| path.ends_with("/assembly_protocol.py"))
        })
        .cloned()
        .expect("the allocated assembly group has one reviewed execute node");
    assert_eq!(
        setup_execute_node["document"]["path"],
        "assets/opentrons_ot2/assembly_protocol.py"
    );
    assert_eq!(
        setup_execute_node["requirements"].as_array().unwrap().len(),
        16
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
            "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2ThermalV1",
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
    assert!(printed.contains("assembly_protocol.py"), "{printed}");
    assert!(printed.contains("transformation_protocol.py"), "{printed}");
    assert!(printed.contains("plating_protocol.py"), "{printed}");
    assert!(printed.contains("Documents:"), "{printed}");
    assert!(
        printed.contains("assembly_manual_protocol.pdf"),
        "{printed}"
    );

    std::fs::remove_dir_all(out_dir).unwrap();
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
    assert_eq!(solution["selections"].as_array().unwrap().len(), 22);
    let requirements = solution_requirements(&solution);
    assert_eq!(requirements.len(), 88);
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
    assert_eq!(lowering["schema_version"], "lab.facility-lowering.v4");
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
    assert_eq!(route["requirements"].as_array().unwrap().len(), 84);
    let protocols = route["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| artifact["role"] == "automation_protocol")
        .collect::<Vec<_>>();
    assert_eq!(protocols.len(), 3);
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
        }) || node["requirement"]
            .as_str()
            .is_some_and(|requirement| requirement.contains(requirement_fragment))
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
    assert_eq!(assembly_setup, assembly_cycle);
    let cell_provisions = (0..4)
        .map(|index| node_id(&format!("std-lab-plasmid-provision-{index}::")))
        .collect::<Vec<_>>();
    for cell_provision in &cell_provisions {
        let provision = execution_nodes
            .iter()
            .find(|node| node["id"] == *cell_provision)
            .expect("the competent-cell provisioning task is executable");
        assert!(
            provision["after"].as_array().is_none_or(Vec::is_empty),
            "cell provisioning is independent of plasmid assembly"
        );
    }
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
    assert_eq!(
        transform_dependencies,
        std::iter::once(assembly_setup.as_str())
            .chain(cell_provisions.iter().map(String::as_str))
            .collect::<std::collections::BTreeSet<_>>(),
        "the fused transformation run must wait for every realized plasmid and provisioned cell input"
    );
    assert!(execution_plan.get("lowerings").is_none());
    let reviewed_protocols = execution_nodes
        .iter()
        .filter_map(|node| node.get("document"))
        .collect::<Vec<_>>();
    assert_eq!(reviewed_protocols.len(), 3);
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
    std::fs::write(project.join("adapters/hamilton-star.toml"), "").unwrap();
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
    fac:facility ex:facility ;
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
    assert_eq!(lowering["schema_version"], "lab.facility-lowering.v4");
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
    assert_eq!(lowered_requirements, 20);
    assert_eq!(automation_artifacts.len(), 8);
    assert!(automation_artifacts.iter().all(|artifact| {
        matches!(
            artifact["format"].as_str(),
            Some("lab.star-run.v0" | "lab.thermocycle-run.v0")
        ) && artifact["sha256"].as_str().unwrap().len() == 64
    }));

    let plan = read_json(out_dir.join("plan.execution.json"));
    assert!(plan.get("lowerings").is_none());
    assert_eq!(
        plan["nodes"]
            .as_array()
            .unwrap()
            .iter()
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
            }
            "lab.thermocycle-run.v0" => {
                thermocycle_documents += 1;
                assert!(path.starts_with("assets/inheco_odtc/"));
                let run = read_json(out_dir.join(path));
                assert_eq!(run["format"], "lab.thermocycle-run.v0");
                assert_eq!(run["profile"]["stages"][0]["repeats"], 75);
                assert_eq!(run["fill_volume_ul"], 20.0);
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
    // Two biological replicates through two dilutions each. Reading only the dilution count would
    // silently emit half the experiment and report n=1.
    let star_dilution =
        read_json(star_output.join("tasks/003-serial-dilution/invocation_manifest.json"));
    let star_execution = &star_dilution["execution"];
    assert_eq!(
        star_execution["culture_wells"].as_array().unwrap().len(),
        2,
        "the STAR adapter stages one culture per biological replicate"
    );
    assert_eq!(
        star_execution["dilution_wells"].as_array().unwrap().len(),
        4,
        "two dilutions for each of two replicates"
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
    assert_eq!(result["result"]["protocols"].as_array().unwrap().len(), 3);

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
    assert_eq!(
        invocations["material_inventory"]["materials"]["reference_gfp"]["component"],
        "https://example.org/golden-gate/materials/reference_gfp"
    );
    assert_eq!(
        invocations["material_inventory"]["materials"]["reference_gfp"]["material_lots"],
        serde_json::json!(["https://example.org/golden-gate/lots/reference_gfp_lot"])
    );
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
        .find(|binding| binding["symbol"] == "reference_gfp")
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
    assert_eq!(solution["selections"].as_array().unwrap().len(), 23);
    let materials = solution_materials(&solution);
    let reference_input = materials
        .iter()
        .copied()
        .find(|binding| binding["symbol"] == "reference_gfp")
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
    assert_eq!(requirements.len(), 89);
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
    assert_eq!(execution["format"], "lab.execution-plan.v6");
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

    let panel_path = project.join("src/programs/panel.lab");
    let panel = std::fs::read_to_string(&panel_path).unwrap().replace(
        "  composite_plasmid_1 <- assemble_composite_plasmid_1\n  composite_plasmid_2 <- assemble_composite_plasmid_2\n\n  // One plasmid goes into two chassis. Material is affine, so a transformation\n  // consumes an aliquot of its own rather than the same value twice.\n  for_cloning, for_expression <- split composite_plasmid_1",
        "  composite_plasmid_1 <- assemble_composite_plasmid_1\n\n  // One plasmid supplies a later assembly and two transformations.\n  for_assembly, for_transformations <- split composite_plasmid_1\n  for_cloning, for_expression <- split for_transformations\n  composite_plasmid_2 <- assemble_composite_plasmid_2 for_assembly",
    );
    std::fs::write(panel_path, panel).unwrap();

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
    let manifest = read_json(target_root.join("assembly_manifest.json"));
    let dependency_transfer = manifest["execution"]["setups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|setup| setup["execution"]["additions"].as_array().unwrap())
        .any(|addition| {
            addition["role"] == "dependency"
                && addition["symbol"] == "composite_plasmid_1"
                && addition["source"]["kind"] == "choice_output"
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
    assert_eq!(protocols.len(), 8);
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
    assert_eq!(route["requirements"].as_array().unwrap().len(), 20);
    let target_root = out_dir.join(route["output"].as_str().unwrap());
    assert!(
        target_root
            .join("tasks/001-setup-golden-gate-reaction/automation_protocol.json")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/002-thermal-cycle-golden-gate-reaction/automation_protocol.json")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/008-serial-dilution/automation_protocol.json")
            .is_file()
    );
    assert!(
        walk_files(&target_root).iter().all(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            !name.contains("transformation_protocol") && !name.contains("plating_protocol")
        }),
        "an exact Flex dilution must not absorb transformation or plating"
    );

    // The example runs two biological replicates through two dilutions each. An adapter that
    // reads only the dilution count would emit half the experiment and silently report n=1.
    let dilution: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("tasks/008-serial-dilution/invocation_manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let execution = &dilution["execution"];
    assert_eq!(
        execution["culture_replicates"], 2,
        "the Flex adapter preserves every biological replicate"
    );
    assert_eq!(execution["serial_dilutions"], 2);
    assert_eq!(
        execution["culture_wells"].as_array().unwrap().len(),
        2,
        "one staged culture per replicate"
    );
    assert_eq!(
        execution["dilution_wells"].as_array().unwrap().len(),
        4,
        "two dilutions for each of two replicates"
    );

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("tasks/001-setup-golden-gate-reaction/invocation_manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["adapter"], "opentrons.flex");
    assert_eq!(
        manifest["deck"]["stages"]["plating"]["agar_plate"]["slots"],
        serde_json::json!(["B2", "B3"]),
        "the emitted plan carries the allocated adapter's deck configuration"
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
            target_root.join("tasks/001-setup-golden-gate-reaction/automation_protocol.json"),
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
    assert_eq!(flex_documents.len(), 8);
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
