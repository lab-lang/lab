use std::path::PathBuf;
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
    let index_path = project.join(".lab/build/package.json");
    let index: Value = serde_json::from_slice(&std::fs::read(index_path).unwrap()).unwrap();
    assert_eq!(index["package"], "test-project");
    assert_eq!(index["modules"][0]["module"], "test_project.programs.main");
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
fn a_target_build_emits_robot_protocols_for_every_wave() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-target-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "opentrons-ot2",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "target build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "built");
    assert_eq!(result["result"]["target"], "opentrons-ot2");
    assert_eq!(
        result["result"]["modules"], 6,
        "designs, workflows, and the program lower as one program"
    );
    // The build names every runnable protocol, so a path can go straight into
    // a robot application.
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
            "build",
            example.to_str().unwrap(),
            "--target",
            "opentrons-ot2",
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let printed = String::from_utf8(human.stdout).unwrap();
    assert!(printed.contains("Robot protocols:"), "{printed}");
    assert!(
        printed.contains("wave-001/assembly_protocol.py"),
        "{printed}"
    );

    let target_root = out_dir.join("opentrons-ot2");
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
        "the emitted plan carries the deck the target profile declared"
    );

    std::fs::remove_dir_all(out_dir).unwrap();
}

#[test]
fn the_manifest_target_builds_robot_protocols_without_naming_one() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-default-target-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let default_target = Command::new(env!("CARGO_BIN_EXE_lab"))
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
        default_target.status.success(),
        "default target build failed: {}",
        String::from_utf8_lossy(&default_target.stderr)
    );
    let result: Value = serde_json::from_slice(&default_target.stdout).unwrap();
    assert_eq!(result["result"]["target"], "workcell-star");
    assert!(
        out_dir
            .join("workcell-star/wave-001/plan.workcell.json")
            .is_file()
    );

    // The default is reversible: a build can still stop at portable module IR.
    std::fs::remove_dir_all(&out_dir).unwrap();
    let ir_only = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--no-target",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        ir_only.status.success(),
        "{}",
        String::from_utf8_lossy(&ir_only.stderr)
    );
    let result: Value = serde_json::from_slice(&ir_only.stdout).unwrap();
    assert_eq!(result["result"]["target"], Value::Null);
    assert!(result["result"]["protocols"].as_array().unwrap().is_empty());
    assert!(!out_dir.join("workcell-star").exists());
    assert!(out_dir.join("package.json").is_file());

    std::fs::remove_dir_all(out_dir).unwrap();
}

/// The `backend` key a profile declares selects the backend, so the same
/// program builds for a Flex without a source edit.
#[test]
fn a_profile_selects_its_backend_and_that_backends_protocol_format() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-flex-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "opentrons-flex",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Flex target build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["result"]["target"], "opentrons-flex");
    let protocols = result["result"]["protocols"].as_array().unwrap();
    assert_eq!(protocols.len(), 3);
    assert!(
        protocols
            .iter()
            .all(|path| path.as_str().unwrap().ends_with("_protocol.json")),
        "a Flex build emits JSON protocols: {protocols:?}"
    );

    let target_root = out_dir.join("opentrons-flex");
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
    assert_eq!(manifest["target"], "opentrons.flex");
    assert_eq!(
        manifest["deck"]["stages"]["plating"]["agar_plate"]["slots"],
        serde_json::json!(["B2", "B3"]),
        "the emitted plan carries the deck the target profile declared"
    );

    let protocol: Value = serde_json::from_str(
        &std::fs::read_to_string(target_root.join("wave-001/assembly_protocol.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(protocol["schemaVersion"], 8);
    assert_eq!(protocol["robot"]["model"], "OT-3 Standard");

    std::fs::remove_dir_all(out_dir).unwrap();
}

#[test]
fn a_target_build_rejects_a_backend_this_toolchain_does_not_provide() {
    let project = temporary_project();
    std::fs::create_dir_all(project.join("src/programs")).unwrap();
    std::fs::write(
        project.join("lab.toml"),
        "[package]\nname = \"unknown-backend\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[build]\nentry = \"src/programs/main.lab\"\n",
    )
    .unwrap();
    std::fs::write(
        project.join("src/programs/main.lab"),
        "use std.bio.build\nuse std.bio.designs\n\nplasmid starter:\n  sequence = dna(\"ATGC\")\n  require topology == circular\n  accept sequence == design.sequence\n\nworkflow main() -> Material<Plasmid>:\n  product <- realize starter\n  return product\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.join("targets")).unwrap();
    std::fs::write(
        project.join("targets/evo.toml"),
        "[target]\nbackend = \"tecan.evo\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["build", project.to_str().unwrap(), "--target", "evo"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("tecan.evo"), "{stderr}");
    assert!(stderr.contains("opentrons.flex"), "{stderr}");
    assert!(stderr.contains("hamilton.star"), "{stderr}");

    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn a_target_build_rejects_a_profile_that_does_not_exist() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["build", example.to_str().unwrap(), "--target", "no-such"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no target profile at"), "{stderr}");
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

#[test]
fn a_workcell_target_lifts_thermal_work_onto_its_cycler_station() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-workcell-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "workcell target build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let target_root = out_dir.join("workcell-star");
    let coordination: Value = serde_json::from_str(
        &std::fs::read_to_string(target_root.join("wave-001/plan.workcell.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(coordination["format"], "lab.workcell-run.v0");
    let stations = coordination["stations"].as_array().unwrap();
    assert_eq!(stations.len(), 2, "the profile declares two stations");

    let nodes = coordination["nodes"].as_array().unwrap();
    assert_eq!(
        nodes[0]["action"], "station-program",
        "the wave opens with the liquid handler's run"
    );
    assert_eq!(nodes[0]["station"], "star-1");
    let actions: Vec<&str> = nodes
        .iter()
        .map(|node| node["action"].as_str().unwrap())
        .collect();
    assert!(
        actions.contains(&"handoff"),
        "plate movements are explicit nodes: {actions:?}"
    );

    // The thermal program left the operator prose and became a station
    // document the cycler executes.
    let cycler_doc: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("wave-001/stations/odtc-1/assembly_thermocycle.odtc.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cycler_doc["format"], "lab.thermocycle-run.v0");
    assert_eq!(cycler_doc["plate"], "reaction_plate");
    assert_eq!(cycler_doc["final_hold_celsius"], 4.0);
    let stages = cycler_doc["profile"]["stages"].as_array().unwrap();
    assert_eq!(
        stages[0]["steps"].as_array().unwrap().len(),
        2,
        "digest and ligate alternate inside the cycled stage"
    );

    // The handler's run document is the same reviewed program it would be
    // on a bare STAR target, minus the operator prose the coordination
    // plan now owns.
    let star_doc: Value = serde_json::from_str(
        &std::fs::read_to_string(
            target_root.join("wave-001/stations/star-1/assembly_run.star.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(star_doc["format"], "lab.star-run.v0");
    assert_eq!(
        star_doc["manual_after"].as_array().unwrap().len(),
        0,
        "sequencing lives in plan.workcell.json, not in the station package"
    );
    assert!(
        target_root
            .join("wave-001/stations/star-1/manual_protocol.typ")
            .is_file(),
        "deck and source loading stay with the handler's own manual"
    );
    let station_manual =
        std::fs::read(target_root.join("wave-001/stations/star-1/manual_protocol.pdf")).unwrap();
    assert!(
        station_manual.starts_with(b"%PDF-"),
        "every emitted document is typeset beside its source"
    );

    // Later waves carry the transformation thermal programs.
    assert!(
        target_root
            .join("wave-002/stations/odtc-1/transformation_heat_shock.odtc.json")
            .is_file()
    );
    assert!(
        target_root
            .join("wave-002/stations/odtc-1/transformation_recovery.odtc.json")
            .is_file()
    );

    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn a_workcell_wave_dry_runs_through_the_coordination_plan() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-workcell-run-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }
    let build = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "workcell build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let wave = out_dir.join("workcell-star/wave-002");
    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", wave.to_str().unwrap(), "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "workcell dry run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "dry-run");
    assert!(
        result["result"]["nodes"].as_u64().unwrap() >= 8,
        "the transformation wave coordinates runs, thermal programs, and handoffs: {result}"
    );

    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn a_workcell_wave_simulates_with_attended_and_walkaway_time() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-simulate-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "workcell target build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wave = out_dir.join("workcell-star/wave-001");

    let simulated = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["simulate", wave.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        simulated.status.success(),
        "simulate failed: {}",
        String::from_utf8_lossy(&simulated.stderr)
    );
    let report: Value = serde_json::from_slice(&simulated.stdout).unwrap();
    assert_eq!(report["status"], "simulate");
    let trace = &report["result"];
    assert_eq!(trace["format"], "lab.sim-trace.v0");

    let summary = &trace["summary"];
    let total = summary["total_seconds"].as_f64().unwrap();
    let attended = summary["attended_seconds"].as_f64().unwrap();
    assert!(total > 0.0, "simulated work takes time");
    assert!(
        attended > 0.0 && attended < total,
        "handoffs are attended, machine time is not: attended {attended} of {total}"
    );
    assert!(
        summary["stations"].get("odtc-1").is_some(),
        "the cycler reports busy time"
    );

    // Simulation interprets the same plan the dry run narrates: one
    // started node per coordination node, in the same order.
    let dry = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", wave.to_str().unwrap(), "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(dry.status.success());
    let dry_report: Value = serde_json::from_slice(&dry.stdout).unwrap();
    let plan: Value =
        serde_json::from_str(&std::fs::read_to_string(wave.join("plan.workcell.json")).unwrap())
            .unwrap();
    let plan_ids: Vec<&str> = plan["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        dry_report["result"]["nodes"].as_u64().unwrap() as usize,
        plan_ids.len()
    );
    let started: Vec<&str> = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["event"] == "node-started")
        .map(|event| event["id"].as_str().unwrap())
        .collect();
    assert_eq!(started, plan_ids, "simulate walks the plan in plan order");

    // The trace document landed beside the plan, and no ledger did.
    assert!(wave.join("sim-trace.json").is_file());
    assert!(
        !wave.join("run-ledger.jsonl").exists(),
        "simulation writes a trace, never a ledger"
    );

    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn a_built_wave_renders_as_a_scene_with_exact_well_positions() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-scene-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let wave = out_dir.join("workcell-star/wave-001");

    let rendered = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["scene", wave.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "scene failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let report: Value = serde_json::from_slice(&rendered.stdout).unwrap();
    assert_eq!(report["status"], "scene");
    assert!(report["result"]["nodes"].as_u64().unwrap() > 100);

    let scene: Value =
        serde_json::from_str(&std::fs::read_to_string(wave.join("scene.json")).unwrap()).unwrap();
    assert_eq!(scene["format"], "lab.scene.v0");
    // Both plan stations render, and the reaction plate keeps its plan
    // resource name so trace events bind to it.
    let text = std::fs::read_to_string(wave.join("scene.json")).unwrap();
    assert!(text.contains("\"reaction_plate\""));
    assert!(text.contains("odtc-1"));

    let gltf: Value =
        serde_json::from_str(&std::fs::read_to_string(wave.join("scene.gltf")).unwrap()).unwrap();
    assert_eq!(gltf["asset"]["version"], "2.0");
    let usda = std::fs::read_to_string(wave.join("scene.usda")).unwrap();
    assert!(usda.starts_with("#usda 1.0"));

    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn a_facility_lays_out_the_scene_with_room_and_assets() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-facility-scene-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let wave = out_dir.join("workcell-star/wave-001");

    // A facility with a room, placed stations, and one stub asset.
    let facility_dir = out_dir.join("facility");
    std::fs::create_dir_all(facility_dir.join("assets")).unwrap();
    std::fs::write(
        facility_dir.join("assets/inheco.odtc.usda"),
        "#usda 1.0\ndef Xform \"odtc\" {}\n",
    )
    .unwrap();
    std::fs::write(
        facility_dir.join("assets/inheco.odtc.glb"),
        b"glTF-stub".as_slice(),
    )
    .unwrap();
    let source = std::fs::read_to_string(example.join("facility.toml")).unwrap();
    std::fs::write(facility_dir.join("main-bench.toml"), source).unwrap();

    let rendered = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "scene",
            wave.to_str().unwrap(),
            "--facility",
            facility_dir.join("main-bench.toml").to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "scene --facility failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );

    let scene_text = std::fs::read_to_string(wave.join("scene.json")).unwrap();
    assert!(
        scene_text.contains("room:floor") && scene_text.contains("room:wall-back"),
        "the kit room renders from [room]"
    );
    assert!(
        scene_text.contains("rotation_z_deg"),
        "a placed station's rotation lands in the scene"
    );
    assert!(
        scene_text.contains("assets/inheco.odtc.glb"),
        "asset paths are bundled relative to the scene"
    );
    assert!(
        wave.join("assets/inheco.odtc.glb").is_file()
            && wave.join("assets/inheco.odtc.usda").is_file(),
        "referenced assets are copied beside the scene"
    );
    let usda = std::fs::read_to_string(wave.join("scene.usda")).unwrap();
    assert!(
        usda.contains("prepend references = @assets/inheco.odtc.usda@"),
        "the USD layer composes the asset by reference:\n{}",
        &usda[..usda.len().min(400)]
    );

    // Without a facility, the schematic scene renders exactly as before.
    let bare = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["scene", wave.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(bare.status.success());
    let bare_text = std::fs::read_to_string(wave.join("scene.json")).unwrap();
    assert!(
        !bare_text.contains("room:floor") && !bare_text.contains("asset_gltf"),
        "no facility means no room shell and no meshes"
    );

    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn an_animated_scene_plays_the_simulated_run_on_the_usd_timeline() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-animated-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let wave = out_dir.join("workcell-star/wave-001");

    // Animation requires a trace; the error names the missing step.
    let premature = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["scene", wave.to_str().unwrap(), "--animated"])
        .output()
        .unwrap();
    assert!(!premature.status.success());
    assert!(
        String::from_utf8_lossy(&premature.stderr).contains("lab simulate"),
        "the error points at the missing step"
    );

    let simulated = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["simulate", wave.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(simulated.status.success());

    let rendered = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["scene", wave.to_str().unwrap(), "--animated", "--json"])
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "scene --animated failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let usda = std::fs::read_to_string(wave.join("scene.usda")).unwrap();
    let trace: Value =
        serde_json::from_str(&std::fs::read_to_string(wave.join("sim-trace.json")).unwrap())
            .unwrap();
    let total = trace["summary"]["total_seconds"].as_f64().unwrap();
    assert!(
        usda.contains(&format!("endTimeCode = {total}")),
        "the stage timeline spans the simulated run"
    );
    assert!(usda.contains("xformOp:translate.timeSamples"));
    assert!(
        usda.contains("def Xform \"pipetting_head\""),
        "the liquid handler grows a head that follows its frames"
    );
    assert!(
        usda.contains("token visibility.timeSamples"),
        "the head hides between programs"
    );
    assert!(
        usda.contains("def Material \"lab_plate\""),
        "preview-surface materials ride along"
    );

    std::fs::remove_dir_all(&out_dir).unwrap();
}

/// Gated on a local Blender, the same pattern as the Opentrons simulator
/// checks: `LAB_BLENDER=/path/to/blender cargo test -p lab-cli`.
#[test]
fn a_wave_renders_one_photographic_frame_through_blender() {
    let Ok(blender) = std::env::var("LAB_BLENDER") else {
        eprintln!("skipping: set LAB_BLENDER to run the render check");
        return;
    };
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let out_dir = std::env::temp_dir().join(format!(
        "lab-golden-gate-render-{}-{}",
        std::process::id(),
        line!()
    ));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }

    for args in [
        vec![
            "build",
            example.to_str().unwrap(),
            "--target",
            "workcell-star",
            "--out-dir",
            out_dir.to_str().unwrap(),
        ],
        vec![
            "simulate",
            out_dir.join("workcell-star/wave-001").to_str().unwrap(),
        ],
        vec![
            "scene",
            out_dir.join("workcell-star/wave-001").to_str().unwrap(),
        ],
    ] {
        let step = Command::new(env!("CARGO_BIN_EXE_lab"))
            .args(&args)
            .output()
            .unwrap();
        assert!(step.status.success(), "step {args:?} failed");
    }

    let wave = out_dir.join("workcell-star/wave-001");
    let rendered = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args([
            "render",
            wave.to_str().unwrap(),
            "--still",
            "600",
            "--quality",
            "preview",
            "--blender",
            &blender,
        ])
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let still = wave.join("renders/frames/still.png");
    assert!(still.is_file(), "the still frame exists");
    assert!(
        std::fs::metadata(&still).unwrap().len() > 10_000,
        "the frame is a real image, not an empty file"
    );

    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn the_zero_argument_flow_simulates_a_package_from_its_root() {
    // Copy the example so the flow's .lab/build output stays out of the
    // repository.
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/golden-gate")
        .canonicalize()
        .unwrap();
    let package = std::env::temp_dir().join(format!(
        "lab-golden-gate-flow-{}-{}",
        std::process::id(),
        line!()
    ));
    if package.exists() {
        std::fs::remove_dir_all(&package).unwrap();
    }
    let copy = Command::new("cp")
        .args(["-R", example.to_str().unwrap(), package.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(copy.status.success());
    std::fs::remove_dir_all(package.join(".lab")).ok();

    // `lab build` with no arguments, from the package directory: the
    // manifest's workcell target lands under .lab/build/.
    let build = Command::new(env!("CARGO_BIN_EXE_lab"))
        .current_dir(&package)
        .arg("build")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "package build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        package
            .join(".lab/build/workcell-star/wave-001/plan.workcell.json")
            .is_file()
    );

    // `lab simulate` with no arguments: every wave, facility by
    // convention from facility.toml at the root.
    let simulate = Command::new(env!("CARGO_BIN_EXE_lab"))
        .current_dir(&package)
        .args(["simulate", "--json"])
        .output()
        .unwrap();
    assert!(
        simulate.status.success(),
        "package simulate failed: {}",
        String::from_utf8_lossy(&simulate.stderr)
    );
    let report: Value = serde_json::from_slice(&simulate.stdout).unwrap();
    let waves = report["result"].as_array().unwrap();
    assert_eq!(waves.len(), 2, "both waves simulate: {report}");
    assert_eq!(waves[0]["wave"], "wave-001");
    for wave in ["wave-001", "wave-002"] {
        assert!(
            package
                .join(format!(".lab/build/workcell-star/{wave}/sim-trace.json"))
                .is_file()
        );
    }
    // The facility's 45 s walk shortened handoffs: the summary's attended
    // time proves facility.toml was picked up without a flag (default
    // handoffs would cost 90 s each).
    let attended = waves[0]["summary"]["attended_seconds"].as_f64().unwrap();
    assert!(
        (attended - 90.0).abs() < 1.0,
        "two 45 s facility handoffs, not two 90 s defaults: {attended}"
    );

    // `lab scene` with no arguments covers every wave too.
    let scene = Command::new(env!("CARGO_BIN_EXE_lab"))
        .current_dir(&package)
        .args(["scene", "--animated", "--json"])
        .output()
        .unwrap();
    assert!(
        scene.status.success(),
        "package scene failed: {}",
        String::from_utf8_lossy(&scene.stderr)
    );
    for wave in ["wave-001", "wave-002"] {
        let usda = std::fs::read_to_string(
            package.join(format!(".lab/build/workcell-star/{wave}/scene.usda")),
        )
        .unwrap();
        assert!(usda.contains("timeSamples"), "{wave} is animated");
        assert!(
            usda.contains("room_floor") || usda.contains("room:floor"),
            "{wave} renders the facility room"
        );
    }

    std::fs::remove_dir_all(&package).unwrap();
}
