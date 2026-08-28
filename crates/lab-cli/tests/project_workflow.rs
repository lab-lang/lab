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
    let index: Value = serde_json::from_slice(&std::fs::read(index_path).unwrap()).unwrap();
    assert_eq!(index["schema_version"], 6);
    assert_eq!(index["package"], "test-project");
    assert_eq!(index["modules"][0]["module"], "test_project.programs.main");
    assert_eq!(
        index["capability_requirements"],
        "capability_requirements.json"
    );
    assert_eq!(index["capability_instances"], "capability_instances.json");
    assert!(index.get("facility").is_none());
    let requirements: Value = serde_json::from_slice(
        &std::fs::read(project.join(".lab/build/capability_requirements.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        requirements["schema_version"],
        "lab.capability-requirements.v2"
    );
    assert_eq!(requirements["requirements"].as_array().unwrap().len(), 1);
    assert_eq!(
        requirements["requirements"][0]["capability_kind"],
        "https://draggon.org/ns/capability#ArtifactRealization"
    );
    assert_eq!(
        requirements["requirements"][0]["minimum_qualification"],
        "https://draggon.org/ns/facility#Plannable"
    );
    assert_eq!(
        requirements["requirements"][0]["value_inputs"][0]["argument"],
        "design"
    );
    assert!(
        requirements["requirements"][0]
            .get("parameter_constraints")
            .is_none()
    );
    let instances: Value = serde_json::from_slice(
        &std::fs::read(project.join(".lab/build/capability_instances.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        instances["schema_version"],
        "lab.capability-requirement-instances.v2"
    );
    assert_eq!(
        instances["requirements_schema_version"],
        "lab.capability-requirements.v2"
    );
    assert_eq!(instances["entry"]["module"], "test_project.programs.main");
    assert_eq!(instances["entry"]["workflow"], "main");
    assert_eq!(instances["instances"].as_array().unwrap().len(), 1);
    assert_eq!(
        instances["instances"][0]["template"],
        "test_project.programs.main::main::body[0]"
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
        r#"@prefix cap: <https://draggon.org/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://draggon.org/ns/facility#> .
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
    let allocation: Value = serde_json::from_slice(
        &std::fs::read(project.join(".lab/plan/facility_allocation.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(allocation["schema_version"], "lab.facility-allocation.v1");
    assert_eq!(
        allocation["allocations"][0]["offering"],
        "https://example.org/facility/operator/realization"
    );
    assert_eq!(
        allocation["allocations"][0]["asset"],
        "https://example.org/facility/operator"
    );
    assert!(allocation["allocations"][0].get("adapter").is_none());
    let plan: Value = serde_json::from_slice(
        &std::fs::read(project.join(".lab/plan/plan.execution.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(plan["format"], "lab.execution-plan.v1");
    assert_eq!(plan["inventory"]["document"], "inventory-source.ttl");
    assert_eq!(
        std::fs::read(project.join(".lab/plan/inventory-source.ttl")).unwrap(),
        std::fs::read(project.join("inventory/catalog.ttl")).unwrap()
    );
    assert_eq!(plan["requirements"].as_array().unwrap().len(), 1);
    assert_eq!(plan["nodes"][0]["action"], "execute");
    assert_eq!(
        plan["nodes"][0]["requirement"],
        plan["requirements"][0]["requirement_instance"]
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

    let index: Value =
        serde_json::from_slice(&std::fs::read(project.join(".lab/build/package.json")).unwrap())
            .unwrap();
    assert_eq!(index["schema_version"], 6);
    assert_eq!(index["adapter_bindings"], "adapter_bindings.json");
    assert_eq!(
        index["facility"]["facility"],
        "https://example.org/sbolinventory/facility"
    );
    let bindings: Value = serde_json::from_slice(
        &std::fs::read(project.join(".lab/build/adapter_bindings.json")).unwrap(),
    )
    .unwrap();
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
fn facility_lowering_emits_automation_protocols_for_every_wave() {
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
    // Planning names every runnable protocol, so a path can go straight into
    // a device application.
    let protocols = result["result"]["protocols"].as_array().unwrap();
    assert_eq!(protocols.len(), 3);
    assert!(
        protocols
            .iter()
            .all(|path| path.as_str().unwrap().ends_with("_protocol.py")),
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
        printed.contains("wave-001/assembly_protocol.py"),
        "{printed}"
    );

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    let target_root = out_dir.join(lowering["routes"][0]["output"].as_str().unwrap());
    // Assembly precedes transformation, and every artifact in a wave shares
    // one robot run.
    assert!(target_root.join("wave-001/assembly_protocol.py").is_file());
    assert!(
        target_root
            .join("wave-002/transformation_protocol.py")
            .is_file()
    );
    assert!(target_root.join("wave-002/plating_protocol.py").is_file());
    assert!(!target_root.join("wave-001/plating_protocol.py").exists());

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(target_root.join("wave-002/automation_manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["strains"].as_array().unwrap().len(), 4);
    assert_eq!(
        manifest["deck"]["stages"]["plating"]["agar_plate"]["slots"],
        serde_json::json!(["5", "6"]),
        "the allocated adapter emits the concrete deck plan"
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
    let index: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("package.json")).unwrap()).unwrap();
    assert_eq!(index["adapter_bindings"], "adapter_bindings.json");
    assert_eq!(index["schema_version"], 6);
    assert_eq!(index["facility"]["protocols"].as_array().unwrap().len(), 3);
    assert!(
        index["facility"]["protocols"][0]
            .as_str()
            .unwrap()
            .starts_with("assets/opentrons_ot2/")
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
            "Allocation: {}",
            out_dir.join("facility_allocation.json").display()
        )),
        "{printed}"
    );
    assert!(printed.contains("Automation protocols:"), "{printed}");
    assert!(printed.contains("assembly_protocol.py"), "{printed}");
    assert!(printed.contains("Documents:"), "{printed}");
    assert!(printed.contains("dependency_report.pdf"), "{printed}");

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

    let allocation: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("facility_allocation.json")).unwrap())
            .unwrap();
    assert_eq!(
        allocation["facility"],
        "https://example.org/golden-gate/facility"
    );
    assert_eq!(allocation["allocations"].as_array().unwrap().len(), 28);
    let liquid_handling = allocation["allocations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|binding| {
            binding["capability_kind"] == "https://draggon.org/ns/capability#LiquidHandling"
        })
        .collect::<Vec<_>>();
    assert!(!liquid_handling.is_empty());
    assert!(liquid_handling.iter().all(|binding| {
        binding["asset"] == "https://example.org/golden-gate/opentrons_ot2"
            && binding["offering"]
                == "https://example.org/golden-gate/opentrons_ot2_liquid_handling"
            && binding["adapter"]["driver"] == "opentrons.ot2"
    }));

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    assert_eq!(lowering["schema_version"], "lab.facility-lowering.v1");
    assert_eq!(lowering["inventory_sha256"], allocation["inventory_sha256"]);
    assert_eq!(lowering["routes"].as_array().unwrap().len(), 1);
    let route = &lowering["routes"][0];
    assert_eq!(
        route["asset"],
        "https://example.org/golden-gate/opentrons_ot2"
    );
    assert_eq!(route["driver"], "opentrons.ot2");
    assert_eq!(route["id"], "opentrons-ot2-5dbf2ae84b40");
    assert_eq!(route["output"], "assets/opentrons_ot2");
    assert_eq!(route["requirements"].as_array().unwrap().len(), 6);
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

    let execution_plan: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("plan.execution.json")).unwrap())
            .unwrap();
    let reviewed_lowering = &execution_plan["lowerings"][0];
    assert_eq!(reviewed_lowering["id"], route["id"]);
    assert_eq!(reviewed_lowering["asset"], route["asset"]);
    assert_eq!(reviewed_lowering["adapter"]["driver"], "opentrons.ot2");
    assert_eq!(
        reviewed_lowering["requirements"].as_array().unwrap().len(),
        6
    );
    let reviewed_protocols = reviewed_lowering["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| artifact["role"] == "device_protocol")
        .collect::<Vec<_>>();
    assert_eq!(reviewed_protocols.len(), 3);
    assert!(reviewed_protocols.iter().all(|artifact| {
        artifact["format"] == "opentrons.python-protocol"
            && artifact["sha256"].as_str().unwrap().len() == 64
            && out_dir.join(artifact["path"].as_str().unwrap()).is_file()
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
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("reviewed adapter lowerings"));

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
    assert_eq!(result["result"]["protocols"].as_array().unwrap().len(), 5);

    let lowering: Value =
        serde_json::from_slice(&std::fs::read(plan_dir.join("facility_lowering.json")).unwrap())
            .unwrap();
    assert_eq!(lowering["routes"].as_array().unwrap().len(), 1);
    let route = &lowering["routes"][0];
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(
            plan_dir
                .join(route["output"].as_str().unwrap())
                .join("dependency_manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["inventory"]["kind"], "sbol_inventory");
    assert_eq!(
        manifest["inventory"]["facility"],
        "https://example.org/golden-gate/facility"
    );
    let reference_binding = manifest["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|node| node["material_lot_bindings"].as_array().unwrap())
        .find(|binding| binding["symbol"] == "reference_gfp")
        .unwrap();
    assert_eq!(
        reference_binding["component"],
        "https://example.org/golden-gate/materials/reference_gfp"
    );
    assert_eq!(
        reference_binding["material_lot"],
        "https://example.org/golden-gate/lots/reference_gfp_lot"
    );

    let allocation: Value =
        serde_json::from_slice(&std::fs::read(plan_dir.join("facility_allocation.json")).unwrap())
            .unwrap();
    assert_eq!(allocation["allocations"].as_array().unwrap().len(), 30);
    let liquid_handling = allocation["allocations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|binding| {
            binding["capability_kind"] == "https://draggon.org/ns/capability#LiquidHandling"
        })
        .collect::<Vec<_>>();
    assert!(!liquid_handling.is_empty());
    assert!(liquid_handling.iter().all(|binding| {
        binding["asset"] == "https://example.org/golden-gate/opentrons_ot2"
            && binding["adapter"]["driver"] == "opentrons.ot2"
    }));

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
    assert_eq!(protocols.len(), 3);
    assert!(
        protocols
            .iter()
            .all(|path| path.as_str().unwrap().ends_with("_protocol.json")),
        "the allocated Flex adapter emits JSON protocols: {protocols:?}"
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
    let target_root = out_dir.join(route["output"].as_str().unwrap());
    assert!(
        target_root
            .join("wave-001/assembly_protocol.json")
            .is_file()
    );
    assert!(
        target_root
            .join("wave-002/transformation_protocol.json")
            .is_file()
    );
    assert!(!target_root.join("wave-001/assembly_protocol.py").exists());

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(target_root.join("wave-002/automation_manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["adapter"], "opentrons.flex");
    assert_eq!(
        manifest["deck"]["stages"]["plating"]["agar_plate"]["slots"],
        serde_json::json!(["B2", "B3"]),
        "the emitted plan carries the allocated adapter's deck configuration"
    );

    let protocol: Value = serde_json::from_str(
        &std::fs::read_to_string(target_root.join("wave-001/assembly_protocol.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(protocol["schemaVersion"], 8);
    assert_eq!(protocol["robot"]["model"], "OT-3 Standard");

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
