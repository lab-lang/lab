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
    assert_eq!(problem["schema_version"], "lab.planning-problem.v4");
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
        "lab.facility-planning-solution.v2"
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
    assert_eq!(bindings["schema_version"], "lab.adapter-bindings.v2");
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
fn facility_lowering_emits_one_protocol_for_each_exact_ot2_requirement() {
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
    assert_eq!(protocols.len(), 8);
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
    assert_eq!(lowering["routes"][0]["scope"], "invocation");
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
            .join("tasks/005-serial-dilution/automation_protocol.py")
            .is_file()
    );
    assert!(
        !walk_files(&target_root)
            .iter()
            .any(|path| path.to_string_lossy().contains("transformation_protocol")),
        "the OT-2 must not absorb transformation allocated to the manual workstation"
    );
    assert!(
        !walk_files(&target_root)
            .iter()
            .any(|path| path.to_string_lossy().contains("plating_protocol")),
        "the dilution requirement must not absorb downstream plating"
    );

    let manifest = read_json(
        target_root.join("tasks/001-setup-golden-gate-reaction/invocation_manifest.json"),
    );
    assert_eq!(manifest["schema_version"], "lab.opentrons-ot2-task.v1");
    assert_eq!(
        manifest["task"]["operation"],
        "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction"
    );
    assert_eq!(manifest["execution"]["kind"], "setup_golden_gate_reaction");
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
    let dilution =
        read_json(target_root.join("tasks/005-serial-dilution/invocation_manifest.json"));
    assert_eq!(dilution["execution"]["kind"], "serial_dilution");
    assert_eq!(
        dilution["execution"]["medium"]["source"]["material_lot"],
        "https://example.org/golden-gate/lots/recovery_medium_lot"
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
    assert_eq!(facility["protocols"].as_array().unwrap().len(), 8);
    assert_eq!(facility["documents"].as_array().unwrap().len(), 8);
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
    assert_eq!(index["facility"]["protocols"].as_array().unwrap().len(), 8);
    assert!(
        index["facility"]["protocols"][0]
            .as_str()
            .unwrap()
            .starts_with("assets/opentrons_ot2/")
    );
    let invocations = read_json(out_dir.join("compiler/adapter-invocations.json"));
    assert_eq!(invocations["schema_version"], "lab.adapter-invocations.v5");
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
fn the_golden_gate_facility_plan_binds_liquid_handling_to_the_ot2() {
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
    assert_eq!(requirements.len(), 24);
    let liquid_handling = requirements
        .iter()
        .copied()
        .filter(|binding| {
            binding["capability_kind"] == "https://sbol.io/ns/capability#LiquidHandling"
        })
        .collect::<Vec<_>>();
    assert!(!liquid_handling.is_empty());
    assert!(liquid_handling.iter().all(|binding| {
        binding["asset"] == "https://example.org/golden-gate/opentrons_ot2"
            && binding["offering"]
                == "https://example.org/golden-gate/opentrons_ot2_liquid_handling"
            && binding["adapter"]["driver"] == "opentrons.ot2"
    }));

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
    assert_eq!(route["scope"], "invocation");
    assert_eq!(route["id"], "opentrons-ot2-5dbf2ae84b40");
    assert_eq!(route["output"], "assets/opentrons_ot2");
    assert_eq!(route["requirements"].as_array().unwrap().len(), 8);
    let protocols = route["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| artifact["role"] == "automation_protocol")
        .collect::<Vec<_>>();
    assert_eq!(protocols.len(), 8);
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
    let node_id = |requirement_fragment: &str| {
        execution_nodes
            .iter()
            .find(|node| {
                node["requirement"]
                    .as_str()
                    .is_some_and(|requirement| requirement.contains(requirement_fragment))
            })
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
        .find(|node| {
            node["requirement"]
                .as_str()
                .is_some_and(|requirement| requirement.contains("std-lab-plasmid-transform-0::"))
        })
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
    assert!(
        execution_plan["lowerings"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "an exact requirement document belongs on its Execute node, not in a whole-program lowering"
    );
    let reviewed_protocols = execution_nodes
        .iter()
        .filter_map(|node| node.get("document"))
        .collect::<Vec<_>>();
    assert_eq!(reviewed_protocols.len(), 8);
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
        String::from_utf8_lossy(&dry_run.stdout).contains("Opentrons OT-2 LiquidHandling protocol")
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
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    let configured_ot2 = r#"[[execution.adapters]]
asset = "https://example.org/golden-gate/opentrons_ot2"
driver = "opentrons.ot2"
profile = "adapters/opentrons-ot2.toml""#;
    assert!(manifest.contains(configured_ot2));
    let star_and_simulator_bindings = r#"[[execution.adapters]]
asset = "https://example.org/golden-gate/hamilton_star"
driver = "hamilton.star"
profile = "adapters/hamilton-star.toml"

[[execution.adapters]]
asset = "https://example.org/golden-gate/thermal_simulator"
driver = "lab.simulator"
profile = "adapters/simulator.toml""#;
    std::fs::write(
        &manifest_path,
        manifest.replace(configured_ot2, star_and_simulator_bindings),
    )
    .unwrap();
    std::fs::write(project.join("adapters/hamilton-star.toml"), "").unwrap();
    std::fs::write(project.join("adapters/simulator.toml"), "").unwrap();

    let inventory_path = project.join("inventory/facility.ttl");
    let inventory = std::fs::read_to_string(&inventory_path)
        .unwrap()
        .replace("opentrons_ot2", "hamilton_star")
        .replace(
            "Opentrons OT-2 with Thermocycler Module",
            "Hamilton STAR liquid handler",
        )
        .replace("OT-2 with Thermocycler Module Gen2", "STAR");
    let combined_offerings =
        "    fac:capability ex:hamilton_star_liquid_handling, ex:hamilton_star_thermal_cycling .";
    assert!(inventory.contains(combined_offerings));
    let split_assets = r#"    fac:capability ex:hamilton_star_liquid_handling .

ex:thermal_simulator
    a sbol:TopLevel, fac:Asset ;
    sbol:displayId "thermal_simulator" ;
    sbol:hasNamespace <https://example.org/golden-gate> ;
    sbol:name "Thermal cycling semantic simulator" ;
    fac:facility ex:facility ;
    fac:assetKind fac:Instrument ;
    fac:locatedIn ex:automation_bench ;
    fac:isActive true ;
    fac:capability ex:hamilton_star_thermal_cycling ."#;
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
    assert!(routes.iter().all(|route| route["scope"] == "invocation"));
    let drivers = routes
        .iter()
        .map(|route| route["driver"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        drivers,
        std::collections::BTreeSet::from(["hamilton.star", "lab.simulator"])
    );
    let assets = routes
        .iter()
        .map(|route| route["asset"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        assets,
        std::collections::BTreeSet::from([
            "https://example.org/golden-gate/hamilton_star",
            "https://example.org/golden-gate/thermal_simulator",
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
    assert_eq!(automation_artifacts.len(), lowered_requirements);
    assert!(automation_artifacts.iter().all(|artifact| {
        matches!(
            artifact["format"].as_str(),
            Some("lab.star-run.v0" | "lab.simulation-run.v1")
        ) && artifact["sha256"].as_str().unwrap().len() == 64
    }));

    let plan = read_json(out_dir.join("plan.execution.json"));
    assert!(plan["lowerings"].as_array().is_none_or(Vec::is_empty));
    let reviewed_documents = plan["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node.get("document"))
        .collect::<Vec<_>>();
    assert_eq!(reviewed_documents.len(), lowered_requirements);
    let mut star_documents = 0;
    let mut simulation_documents = 0;
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
            "lab.simulation-run.v1" => {
                simulation_documents += 1;
                assert!(path.starts_with("assets/thermal_simulator/"));
            }
            format => panic!("unexpected invocation document format: {format}"),
        }
        assert!(out_dir.join(path).is_file());
    }
    assert!(star_documents > 0);
    assert!(simulation_documents > 0);

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

    let result: Value = serde_json::from_slice(&built.stdout).unwrap();
    assert_eq!(
        result["result"]["facility"]["protocols"]
            .as_array()
            .unwrap()
            .len(),
        lowered_requirements
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
    assert_eq!(route["scope"], "invocation");
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
    assert_eq!(requirements.len(), 25);
    let liquid_handling = requirements
        .iter()
        .copied()
        .filter(|binding| {
            binding["capability_kind"] == "https://sbol.io/ns/capability#LiquidHandling"
        })
        .collect::<Vec<_>>();
    assert!(!liquid_handling.is_empty());
    assert!(liquid_handling.iter().all(|binding| {
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
            "OT-2 with Thermocycler Module Gen2",
            "Flex with Thermocycler Module Gen2",
        );
    std::fs::write(inventory_path, inventory).unwrap();
    let manifest_path = project.join("lab.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("opentrons_ot2", "opentrons_flex")
        .replace("opentrons.ot2", "opentrons.flex")
        .replace("opentrons-ot2.toml", "opentrons-flex.toml");
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
        "the allocated Flex adapter emits one JSON protocol per exact requirement: {protocols:?}"
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
    assert_eq!(route["scope"], "invocation");
    assert_eq!(route["requirements"].as_array().unwrap().len(), 8);
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
    assert!(execution["lowerings"].as_array().is_none_or(Vec::is_empty));
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
