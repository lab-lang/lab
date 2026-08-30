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
fn facility_lowering_emits_the_complete_golden_gate_ot2_slice() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
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
    // Planning names every requirement-scoped protocol, so a path can go straight into the
    // device application without asking the backend to rediscover the experiment.
    let protocols = result["result"]["protocols"].as_array().unwrap();
    assert_eq!(protocols.len(), 28);
    assert!(
        protocols
            .iter()
            .all(|path| path.as_str().unwrap().contains("/tasks/")
                && path.as_str().unwrap().ends_with("/automation_protocol.py")),
        "{protocols:?}"
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
    assert!(
        printed.contains("tasks/001-setup-golden-gate-reaction/automation_protocol.py"),
        "{printed}"
    );

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    let target_root = out_dir.join(lowering["routes"][0]["output"].as_str().unwrap());
    assert!(lowering["routes"][0].get("scope").is_none());
    assert!(
        target_root
            .join("tasks/001-setup-golden-gate-reaction/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/002-thermal-cycle-golden-gate-reaction/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/005-prepare-chemical-transformation/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/006-heat-shock-transformation/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/007-add-recovery-medium/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/008-incubate-recovery-culture/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/009-serial-dilution/automation_protocol.py")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/010-plate-diluted-culture/automation_protocol.py")
            .is_file()
    );

    let manifest = read_json(
        target_root.join("tasks/001-setup-golden-gate-reaction/invocation_manifest.json"),
    );
    assert_eq!(manifest["schema_version"], "lab.opentrons-ot2-task.v3");
    assert_eq!(
        manifest["task"]["operation"],
        "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction"
    );
    assert_eq!(manifest["execution"]["kind"], "setup_golden_gate_reaction");
    assert_eq!(
        manifest["deck"]["deck"]["thermocycler"]["model"], "thermocycler module",
        "the example's exact OT-2 Asset has a Thermocycler Module GEN1"
    );
    assert!(
        manifest["execution"]["additions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|addition| addition["source"]["kind"] == "material_lot")
    );
    assert_eq!(
        manifest["deck"]["stages"]["plating"]["agar_plate"]["slots"],
        serde_json::json!(["5", "6"]),
        "the allocated adapter emits the concrete deck plan"
    );
    let transformation = read_json(
        target_root.join("tasks/005-prepare-chemical-transformation/invocation_manifest.json"),
    );
    assert_eq!(
        transformation["execution"]["kind"],
        "prepare_chemical_transformation"
    );
    assert_eq!(transformation["execution"]["cell_volume_ul"], 20);
    assert_eq!(transformation["execution"]["dna_volume_ul"], 2);
    assert_eq!(
        transformation["execution"]["bubble_clear_technique"]["dispense"]["offset"]["value"]["value"],
        "8"
    );
    assert_eq!(
        transformation["execution"]["bubble_clear_technique"]["touch_tip"],
        true
    );
    let transformation_protocol = read_text(
        target_root.join("tasks/005-prepare-chemical-transformation/automation_protocol.py"),
    );
    assert!(
        transformation_protocol.contains("disposal_volume=0"),
        "{transformation_protocol}"
    );
    assert!(
        transformation_protocol.contains("for _ in range(execution[\"bubble_clear_cycles\"]):"),
        "{transformation_protocol}"
    );
    assert!(
        transformation_protocol.contains("radius=techniques[\"touch_tip_radius\"]"),
        "{transformation_protocol}"
    );
    let heat_shock =
        read_json(target_root.join("tasks/006-heat-shock-transformation/invocation_manifest.json"));
    assert_eq!(heat_shock["execution"]["volume_each_ul"], 22.0);
    assert_eq!(
        heat_shock["execution"]["profile"]["stages"][0]["steps"][1]["celsius"],
        42.0
    );
    let heat_shock_protocol =
        read_text(target_root.join("tasks/006-heat-shock-transformation/automation_protocol.py"));
    assert!(
        heat_shock_protocol.contains("block_max_volume=execution[\"volume_each_ul\"]"),
        "{heat_shock_protocol}"
    );
    assert!(
        !heat_shock_protocol.contains("is_simulating"),
        "thermal work may not disappear during simulation"
    );
    let recovery_medium =
        read_json(target_root.join("tasks/007-add-recovery-medium/invocation_manifest.json"));
    assert_eq!(
        recovery_medium["execution"]["technique"]["dispense"]["kind"],
        "above_liquid"
    );
    assert_eq!(
        recovery_medium["execution"]["technique"]["air_gap"]["value"]["value"],
        "10"
    );
    let recovery =
        read_json(target_root.join("tasks/008-incubate-recovery-culture/invocation_manifest.json"));
    assert_eq!(recovery["execution"]["volume_each_ul"], 82.0);
    let dilution =
        read_json(target_root.join("tasks/009-serial-dilution/invocation_manifest.json"));
    assert_eq!(dilution["execution"]["kind"], "serial_dilution");
    assert_eq!(
        dilution["execution"]["medium"]["source"]["material_lot"],
        "https://example.org/golden-gate/lots/recovery_medium_lot"
    );
    assert_eq!(
        dilution["execution"]["dilution_wells"],
        serde_json::json!([
            {"plate": 0, "well": "A1"},
            {"plate": 0, "well": "B1"},
            {"plate": 0, "well": "A7"},
            {"plate": 0, "well": "B7"}
        ])
    );
    assert_eq!(
        dilution["deck"]["techniques"]["distribution_disposal_volume_ul"],
        4
    );
    let dilution_protocol =
        read_text(target_root.join("tasks/009-serial-dilution/automation_protocol.py"));
    assert!(
        dilution_protocol.contains("current_volume / source.max_volume"),
        "{dilution_protocol}"
    );
    assert!(
        dilution_protocol.contains("tracked_low_volume_fraction"),
        "{dilution_protocol}"
    );
    assert!(
        dilution_protocol.contains("tracked_chunk_size"),
        "{dilution_protocol}"
    );
    let plating =
        read_json(target_root.join("tasks/010-plate-diluted-culture/invocation_manifest.json"));
    assert_eq!(plating["execution"]["kind"], "plate_diluted_culture");
    assert_eq!(
        plating["execution"]["plate_map"].as_array().unwrap().len(),
        4
    );
    assert_eq!(
        plating["execution"]["technique"]["dispense"]["kind"],
        "material_surface"
    );
    assert!(
        target_root
            .join("tasks/010-plate-diluted-culture/plate_map.json")
            .is_file()
    );
    assert!(
        target_root
            .join("tasks/010-plate-diluted-culture/plate_map.pdf")
            .is_file()
    );
    let plate_map = read_json(target_root.join("tasks/010-plate-diluted-culture/plate_map.json"));
    assert_eq!(plate_map["artifact"], "composite_strain_1");
    assert_eq!(plate_map["entries"][0]["dilution_ratio"], "1/10");
    assert_eq!(plate_map["entries"][2]["dilution_ratio"], "1/100");
    assert_eq!(plate_map["entries"], plating["execution"]["plate_map"]);
    let plating_protocol =
        read_text(target_root.join("tasks/010-plate-diluted-culture/automation_protocol.py"));
    assert!(
        plating_protocol.contains("destination.top(techniques[\"material_surface_offset_mm\"]"),
        "{plating_protocol}"
    );
    assert!(
        plating_protocol.contains("pipette.blow_out()"),
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
    assert_eq!(facility["protocols"].as_array().unwrap().len(), 28);
    assert_eq!(facility["documents"].as_array().unwrap().len(), 32);
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
    assert_eq!(index["facility"]["protocols"].as_array().unwrap().len(), 28);
    assert!(
        index["facility"]["protocols"][0]
            .as_str()
            .unwrap()
            .starts_with("assets/opentrons_ot2/")
    );
    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_eq!(invocations["schema_version"], "lab.adapter-invocations.v7");
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
        "https://synbiohub.org/public/igem/J23101"
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
            node["requirements"]
                .as_array()
                .is_some_and(|requirements| requirements.len() == 2)
        })
        .cloned()
        .expect("the atomic pipetting program has one multi-requirement execute node");
    assert_eq!(
        setup_execute_node["document"]["path"],
        "assets/opentrons_ot2/tasks/001-setup-golden-gate-reaction/automation_protocol.py"
    );
    assert!(
        setup_execute_node["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|requirement| {
                execution_plan["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|binding| {
                binding["requirement_instance"].as_str() == requirement.as_str()
            })
            .is_some_and(|binding| binding["procedure_implementation"]
                == "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1")
            })
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
    assert_eq!(requirements.len(), 76);
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
    assert_eq!(route["requirements"].as_array().unwrap().len(), 72);
    let protocols = route["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| artifact["role"] == "automation_protocol")
        .collect::<Vec<_>>();
    assert_eq!(protocols.len(), 28);
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
        "https://www.lab-compiler.org/ns/method#automated-golden-gate"
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
        "std-bio-build-realize-0::https://www.lab-compiler.org/ns/method#automated-golden-gate::setup-reaction",
    );
    let assembly_cycle = node_id(
        "std-bio-build-realize-0::https://www.lab-compiler.org/ns/method#automated-golden-gate::cycle-reaction",
    );
    let cell_provision = node_id("std-lab-plasmid-provision-0::");
    let provision = execution_nodes
        .iter()
        .find(|node| node["id"] == cell_provision)
        .expect("the competent-cell provisioning task is executable");
    assert!(
        provision["after"].as_array().is_none_or(Vec::is_empty),
        "cell provisioning is independent of plasmid assembly"
    );
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
        std::collections::BTreeSet::from([
            assembly_setup.as_str(),
            assembly_cycle.as_str(),
            cell_provision.as_str(),
        ]),
        "transformation must wait for both its realized plasmid and provisioned cells"
    );
    assert!(execution_plan.get("lowerings").is_none());
    let reviewed_protocols = execution_nodes
        .iter()
        .filter_map(|node| node.get("document"))
        .collect::<Vec<_>>();
    assert_eq!(reviewed_protocols.len(), 28);
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
    assert_eq!(result["result"]["protocols"].as_array().unwrap().len(), 8);

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
    assert_eq!(requirements.len(), 37);
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
    let mut dependency_transfer = false;
    for artifact in route["artifacts"]
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
        let manifest = read_json(target_root.join(artifact["path"].as_str().unwrap()));
        dependency_transfer |=
            manifest["execution"]["additions"]
                .as_array()
                .is_some_and(|additions| {
                    additions.iter().any(|addition| {
                        addition["role"] == "dependency"
                            && addition["symbol"] == "composite_plasmid_1"
                            && addition["source"]["kind"] == "choice_output"
                    })
                });
    }
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
