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
    assert_eq!(result["result"]["target"], "opentrons-ot2");
    assert!(
        out_dir
            .join("opentrons-ot2/wave-001/assembly_protocol.py")
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
    assert!(!out_dir.join("opentrons-ot2").exists());
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
            .join("wave-001/stations/star-1/manual_protocol.md")
            .is_file(),
        "deck and source loading stay with the handler's own manual"
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
