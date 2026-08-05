use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("opentrons-build")
        .join("reporter-library.lab")
}

fn examples_dir() -> PathBuf {
    example().parent().unwrap().to_owned()
}

#[test]
fn writes_a_complete_multi_construct_automation_bundle() {
    let output_dir =
        std::env::temp_dir().join(format!("lab-opentrons-build-test-{}", std::process::id()));
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([
            example().to_str().unwrap(),
            "--emit",
            "automation-bundle",
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "automation bundle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = [
        "assembly_protocol.py",
        "automation_manifest.json",
        "manual_protocol.md",
        "plating_protocol.py",
        "transformation_protocol.py",
    ];
    for name in expected {
        assert!(output_dir.join(name).is_file(), "missing {name}");
    }

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("automation_manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["constructs"].as_array().unwrap().len(), 2);
    assert_eq!(manifest["constructs"][0]["artifact"], "p_gfp");
    assert_eq!(manifest["constructs"][0]["backbone"], "pSB1C3");
    assert_eq!(
        manifest["constructs"][0]["components"],
        serde_json::json!(["J23101", "B0034", "GFP", "B0015"])
    );
    assert_eq!(
        manifest["constructs"][1]["transformations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let manual = std::fs::read_to_string(output_dir.join("manual_protocol.md")).unwrap();
    assert!(manual.contains("p_gfp"));
    assert!(manual.contains("p_rfp"));
    assert!(manual.contains("Execution boundary"));

    std::fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn derives_and_packages_a_dependency_driven_full_build() {
    let output_dir =
        std::env::temp_dir().join(format!("lab-full-build-test-{}", std::process::id()));
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([
            examples_dir().join("full-build.lab").to_str().unwrap(),
            "--emit",
            "full-build-bundle",
            "--inventory",
            examples_dir()
                .join("full-build-inventory.json")
                .to_str()
                .unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "full build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("dependency_manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["status"], "complete");
    assert_eq!(manifest["roots"], serde_json::json!(["final_device"]));
    assert_eq!(
        manifest["nodes"][0]["steps"],
        serde_json::json!(["assemble", "transform", "recover", "dilute", "plate"])
    );
    let iterations = manifest["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            (
                node["artifact"].as_str().unwrap(),
                node["generated_in_iteration"].as_u64().unwrap(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(iterations["promoter_carrier"], 1);
    assert_eq!(iterations["reporter_region"], 2);
    assert_eq!(iterations["regulator_region"], 1);
    assert_eq!(iterations["final_device"], 3);

    let assembly_protocols = [
        "batch-001-promoter_carrier/assembly_protocol.py",
        "batch-002-regulator_region/assembly_protocol.py",
        "batch-003-reporter_region/assembly_protocol.py",
        "batch-004-final_device/assembly_protocol.py",
    ];
    for protocol in assembly_protocols {
        assert!(output_dir.join(protocol).is_file(), "missing {protocol}");
    }
    assert!(output_dir.join("dependency_report.md").is_file());
    let instructions = std::fs::read_to_string(output_dir.join("manual_protocol.md")).unwrap();
    assert!(instructions.contains("## Execution order"));
    assert!(instructions.contains("## Batch 001 — `promoter_carrier`"));
    assert!(instructions.contains("## Batch 004 — `final_device`"));
    assert!(instructions.contains("Required generated or retrieved artifact inputs"));
    assert!(instructions.contains("### Stage 3 — Serial dilution and plating"));

    if let Ok(simulator) = std::env::var("LAB_OPENTRONS_SIMULATOR") {
        let simulator = PathBuf::from(simulator);
        let simulator = if simulator.is_absolute() {
            simulator
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(simulator)
        };
        for entry in std::fs::read_dir(&output_dir).unwrap() {
            let entry = entry.unwrap();
            if !entry.file_type().unwrap().is_dir()
                || !entry.file_name().to_string_lossy().starts_with("batch-")
            {
                continue;
            }
            for protocol in [
                "assembly_protocol.py",
                "transformation_protocol.py",
                "plating_protocol.py",
            ] {
                let path = entry.path().join(protocol);
                let simulation = Command::new(&simulator)
                    .env("OT_API_CONFIG_DIR", output_dir.join(".opentrons-config"))
                    .args(["-o", "nothing", path.to_str().unwrap()])
                    .output()
                    .unwrap();
                assert!(
                    simulation.status.success(),
                    "simulation failed for {}: {}",
                    path.display(),
                    String::from_utf8_lossy(&simulation.stderr)
                );
            }
        }
    }

    std::fs::remove_dir_all(output_dir).unwrap();
}
