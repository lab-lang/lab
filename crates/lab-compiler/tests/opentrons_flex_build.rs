//! End-to-end coverage of the Flex backend through the `labc` binary: the
//! artifacts a bundle contains, and the commands inside the emitted JSON
//! protocols field by field.
//!
//! Setting `LAB_OPENTRONS_ANALYZE` to a Python interpreter with the
//! `opentrons` package installed additionally runs `opentrons analyze` over
//! every emitted protocol, which is the ground truth the authoring crate's
//! guarantees are measured against.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn flex_profile() -> PathBuf {
    fixture("opentrons-flex-adapter.toml")
}

fn command_types(protocol: &Value) -> Vec<&str> {
    protocol["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["commandType"].as_str().unwrap())
        .collect()
}

fn commands_of<'a>(protocol: &'a Value, command_type: &str) -> Vec<&'a Value> {
    protocol["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|command| command["commandType"] == command_type)
        .collect()
}

fn read_protocol(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Run `opentrons analyze` over every emitted protocol when the gate variable
/// names an interpreter, and assert each one is accepted.
fn analyze_if_requested(output_dir: &Path) {
    let Ok(python) = std::env::var("LAB_OPENTRONS_ANALYZE") else {
        return;
    };
    let python = PathBuf::from(python);
    let python = if python.is_absolute() {
        python
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(python)
    };
    let mut analyzed = 0;
    for entry in std::fs::read_dir(output_dir).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_protocol.json"))
        {
            continue;
        }
        let analysis = Command::new(&python)
            .env("OT_API_CONFIG_DIR", output_dir.join(".opentrons-config"))
            .args([
                "-m",
                "opentrons.cli",
                "analyze",
                "--check",
                path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            analysis.status.success(),
            "opentrons analyze rejected {}: {}{}",
            path.display(),
            String::from_utf8_lossy(&analysis.stdout),
            String::from_utf8_lossy(&analysis.stderr)
        );
        analyzed += 1;
    }
    assert_eq!(analyzed, 3, "every stage protocol must be analyzed");
}

#[test]
fn writes_a_complete_flex_automation_bundle_of_json_protocols() {
    let output_dir =
        std::env::temp_dir().join(format!("lab-flex-build-test-{}", std::process::id()));
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([
            fixture("reporter-library.lab").to_str().unwrap(),
            "--emit",
            "automation-bundle",
            "--adapter",
            "opentrons.flex",
            "--adapter-profile",
            flex_profile().to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Flex automation bundle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for name in [
        "assembly_protocol.json",
        "automation_manifest.json",
        "lab-style.typ",
        "manual_protocol.typ",
        "plating_protocol.json",
        "transformation_protocol.json",
    ] {
        assert!(output_dir.join(name).is_file(), "missing {name}");
    }
    assert!(
        !output_dir.join("assembly_protocol.py").exists(),
        "the Flex backend emits JSON protocols, not Python"
    );

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("automation_manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["target"], "opentrons.flex");
    assert_eq!(manifest["schema_version"], "lab.automation.v0");
    assert_eq!(manifest["deck"]["target"]["backend"], "opentrons.flex");
    assert_eq!(manifest["assemblies"].as_array().unwrap().len(), 2);
    assert_eq!(manifest["strains"].as_array().unwrap().len(), 2);

    let assembly = read_protocol(&output_dir.join("assembly_protocol.json"));
    assert_eq!(assembly["$otSharedSchema"], "#/protocol/schemas/8");
    assert_eq!(assembly["schemaVersion"], 8);
    assert_eq!(assembly["commandSchemaId"], "opentronsCommandSchemaV8");
    assert_eq!(
        assembly["labwareDefinitionSchemaId"],
        "opentronsLabwareSchemaV2"
    );
    assert_eq!(
        assembly["robot"],
        serde_json::json!({
            "model": "OT-3 Standard",
            "deckId": "ot3_standard"
        })
    );

    // Loads come first and in dependency order: modules before the labware
    // they carry, and the pipette after the tip racks it draws from.
    let types = command_types(&assembly);
    let load_module = types.iter().position(|kind| *kind == "loadModule").unwrap();
    let load_labware = types
        .iter()
        .position(|kind| *kind == "loadLabware")
        .unwrap();
    let load_pipette = types
        .iter()
        .position(|kind| *kind == "loadPipette")
        .unwrap();
    let first_pipetting = types.iter().position(|kind| *kind == "pickUpTip").unwrap();
    assert!(load_module < load_labware);
    assert!(load_pipette < first_pipetting);

    let modules = commands_of(&assembly, "loadModule");
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[0]["params"]["model"], "temperatureModuleV2");
    assert_eq!(modules[0]["params"]["location"]["slotName"], "C1");
    assert_eq!(modules[1]["params"]["model"], "thermocyclerModuleV2");
    assert_eq!(
        modules[1]["params"]["location"]["slotName"], "B1",
        "the thermocycler spans A1 and B1 and is addressed as B1"
    );

    let pipettes = commands_of(&assembly, "loadPipette");
    assert_eq!(
        pipettes.len(),
        1,
        "assembly runs on the small pipette alone"
    );
    assert_eq!(pipettes[0]["params"]["pipetteName"], "p50_single_flex");
    assert_eq!(pipettes[0]["params"]["mount"], "left");

    // Every labware a command names is embedded, keyed the way Opentrons
    // tooling derives the key.
    let definitions = assembly["labwareDefinitions"].as_object().unwrap();
    assert!(definitions.contains_key("opentrons/opentrons_flex_96_tiprack_50ul/1"));
    assert!(definitions.contains_key("opentrons/nest_96_wellplate_100ul_pcr_full_skirt/3"));
    for command in commands_of(&assembly, "loadLabware") {
        let key = format!(
            "{}/{}/{}",
            command["params"]["namespace"].as_str().unwrap(),
            command["params"]["loadName"].as_str().unwrap(),
            command["params"]["version"].as_u64().unwrap()
        );
        assert!(
            definitions.contains_key(&key),
            "unembedded definition {key}"
        );
    }

    // The fixture's plasmids state no chemistry, so the standard Golden Gate
    // values drive the block: 75 cycles of 37 C for 2 min and 16 C for 5 min,
    // then the 50 C and 80 C soaks.
    let profiles = commands_of(&assembly, "thermocycler/runProfile");
    assert_eq!(profiles.len(), 1, "a batch shares one thermal profile");
    let steps = profiles[0]["params"]["profile"].as_array().unwrap();
    assert_eq!(steps.len(), 75 * 2 + 2);
    assert_eq!(
        steps[0],
        serde_json::json!({"celsius": 37.0, "holdSeconds": 120.0})
    );
    assert_eq!(
        steps[1],
        serde_json::json!({"celsius": 16.0, "holdSeconds": 300.0})
    );
    assert_eq!(
        steps[steps.len() - 2],
        serde_json::json!({"celsius": 50.0, "holdSeconds": 300.0})
    );
    assert_eq!(
        steps[steps.len() - 1],
        serde_json::json!({"celsius": 80.0, "holdSeconds": 600.0})
    );
    assert_eq!(profiles[0]["params"]["blockMaxVolumeUl"], 20.0);

    // Tips go to the movable trash by addressable area; a Flex has no trash
    // labware, so no plain dropTip appears.
    let trash_moves = commands_of(&assembly, "moveToAddressableAreaForDropTip");
    assert!(!trash_moves.is_empty());
    assert_eq!(
        trash_moves[0]["params"]["addressableAreaName"],
        "movableTrashA3"
    );
    assert!(commands_of(&assembly, "dropTip").is_empty());
    assert_eq!(
        commands_of(&assembly, "dropTipInPlace").len(),
        trash_moves.len(),
        "each positioning move is followed by one in-place drop"
    );

    let transformation = read_protocol(&output_dir.join("transformation_protocol.json"));
    assert_eq!(
        commands_of(&transformation, "loadPipette").len(),
        2,
        "transformation uses both the small and large pipettes"
    );
    // The heat-shock sequence holds cold, shocks, returns cold, then recovers.
    let block_targets: Vec<f64> =
        commands_of(&transformation, "thermocycler/setTargetBlockTemperature")
            .iter()
            .map(|command| command["params"]["celsius"].as_f64().unwrap())
            .collect();
    assert_eq!(block_targets, [4.0, 42.0, 4.0, 37.0, 4.0]);
    let holds: Vec<f64> = commands_of(&transformation, "waitForDuration")
        .iter()
        .map(|command| command["params"]["seconds"].as_f64().unwrap())
        .collect();
    assert_eq!(holds, [1800.0, 60.0, 120.0, 3600.0]);

    let plating = read_protocol(&output_dir.join("plating_protocol.json"));
    // Agar spotting dispenses below the well top so the drop reaches the agar.
    let spotting = commands_of(&plating, "dispense")
        .into_iter()
        .find(|command| command["params"]["wellLocation"].is_object())
        .expect("colonies are spotted with an explicit well location");
    assert_eq!(spotting["params"]["wellLocation"]["origin"], "top");
    assert_eq!(spotting["params"]["wellLocation"]["offset"]["z"], -8.0);

    let manual = std::fs::read_to_string(output_dir.join("manual_protocol.typ")).unwrap();
    assert!(
        manual.contains("`p_gfp`"),
        "identifiers render in the code face"
    );
    assert!(manual.contains("`p_rfp`"));
    assert!(manual.contains("Opentrons JSON protocol (schema 8)"));
    assert!(manual.contains("= Execution boundary"));

    analyze_if_requested(&output_dir);

    std::fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn packages_a_dependency_driven_flex_build_by_wave() {
    let output_dir =
        std::env::temp_dir().join(format!("lab-flex-full-build-test-{}", std::process::id()));
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([
            fixture("full-build.lab").to_str().unwrap(),
            "--emit",
            "full-build-bundle",
            "--adapter",
            "opentrons.flex",
            "--adapter-profile",
            flex_profile().to_str().unwrap(),
            "--inventory",
            fixture("full-build-inventory.json").to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Flex full build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for protocol in [
        "wave-001/assembly_protocol.json",
        "wave-002/assembly_protocol.json",
        "wave-003/assembly_protocol.json",
        "wave-004/transformation_protocol.json",
        "wave-004/plating_protocol.json",
    ] {
        assert!(output_dir.join(protocol).is_file(), "missing {protocol}");
    }
    assert!(
        !output_dir.join("wave-001/plating_protocol.json").exists(),
        "an assembly-only wave emits no plating protocol"
    );
    assert!(output_dir.join("dependency_report.typ").is_file());
    assert!(output_dir.join("lab-style.typ").is_file());

    let instructions = std::fs::read_to_string(output_dir.join("manual_protocol.typ")).unwrap();
    assert!(instructions.contains("= Execution order"));
    assert!(instructions.contains("= #hl(\"Run 004\")`reporter_host`"));

    std::fs::remove_dir_all(output_dir).unwrap();
}
