use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn specimen(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs")
        .join("language")
        .join("specimens")
        .join(name)
}

fn compile(path: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_labc"))
        .arg(path)
        .args(arguments)
        .output()
        .unwrap()
}

fn module_ir(name: &str) -> Value {
    let output = compile(&specimen(name), &["--emit", "module-ir"]);
    assert!(
        output.status.success(),
        "{name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn plasmid_design_compiles_through_the_portable_module_boundary() {
    let module = module_ir("plasmid-design.lab");
    assert_eq!(module["imports"].as_array().unwrap().len(), 2);
    let declarations = module["declarations"].as_array().unwrap();
    assert!(declarations.iter().any(|declaration| {
        declaration["kind"] == "binding"
            && declaration["targets"][0]["type"]["name"] == "Circuit"
            && declaration["targets"][0]["type"]["arguments"][0]["name"] == "Tetracycline"
            && declaration["targets"][0]["type"]["arguments"][1]["name"]
                == "GreenFluorescentProtein"
    }));
    assert!(declarations.iter().any(|declaration| {
        declaration["kind"] == "artifact"
            && declaration["artifact"] == "plasmid"
            && declaration["name"] == "p_tet_reporter"
            && declaration["requirements"].as_array().unwrap().len() == 3
            && declaration["acceptance"].as_array().unwrap().len() == 3
    }));

    let human = compile(&specimen("plasmid-design.lab"), &[]);
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("plasmid p_tet_reporter")
    );
}

#[test]
fn plasmid_build_compiles_typed_effects_and_reactive_handlers() {
    let module = module_ir("plasmid-build.lab");
    let declarations = module["declarations"].as_array().unwrap();
    let workflow = declarations
        .iter()
        .find(|declaration| declaration["name"] == "build_plasmid")
        .unwrap();
    assert_eq!(workflow["outputs"][0]["name"], "outcome");
    assert_eq!(workflow["outputs"][0]["type"]["kind"], "union");
    assert_eq!(
        workflow["outputs"][0]["type"]["alternatives"][0]["name"],
        "Accepted"
    );
    assert_eq!(
        workflow["outputs"][0]["type"]["alternatives"][1]["name"],
        "Rejected"
    );
    let serialized = serde_json::to_string(workflow).unwrap();
    assert!(serialized.contains("workflow.await_colonies"));
    assert!(serialized.contains("std.lab.plasmid_actions.split"));
    assert!(serialized.contains("\"mode\":\"take\""));
    assert!(serialized.contains("\"mode\":\"borrow\""));

    let await_colonies = declarations
        .iter()
        .find(|declaration| declaration["name"] == "await_colonies")
        .unwrap();
    let serialized = serde_json::to_string(await_colonies).unwrap();
    assert_eq!(await_colonies["state"][0]["name"], "observations");
    assert!(serialized.contains("\"kind\":\"state_update\""));
    assert!(serialized.contains("\"kind\":\"every\""));
    assert!(serialized.contains("\"kind\":\"after\""));

    let human = compile(&specimen("plasmid-build.lab"), &[]);
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("workflow build_plasmid")
    );
}

#[test]
fn inventory_specimen_preserves_properties_and_resolved_operations() {
    let module = module_ir("inventory-plasmid.lab");
    let declarations = module["declarations"].as_array().unwrap();
    let reporter = declarations
        .iter()
        .find(|declaration| declaration["name"] == "reporter")
        .unwrap();
    assert_eq!(reporter["kind"], "artifact");
    assert_eq!(reporter["artifact"], "plasmid");
    assert!(reporter.get("bindings").is_none());
    assert!(
        reporter["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|property| property["name"] == "components"
                && property["value"]["type"]["element"]["name"] == "Part")
    );

    let serialized = serde_json::to_string(&module).unwrap();
    assert!(serialized.contains("std.bio.inventory.part"));
    assert!(serialized.contains("std.bio.build.realize"));
    assert!(serialized.contains("artifact_realization"));
}

#[test]
fn dependency_specimen_preserves_typed_material_edges_without_levels() {
    let module = module_ir("dependency-build.lab");
    let declarations = module["declarations"].as_array().unwrap();
    let reporter = declarations
        .iter()
        .find(|declaration| declaration["name"] == "reporter_region")
        .unwrap();
    let components = reporter["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| property["name"] == "components")
        .unwrap();
    let alternatives = components["value"]["type"]["element"]["alternatives"]
        .as_array()
        .unwrap();
    assert_eq!(alternatives[0]["name"], "Plasmid");
    assert_eq!(alternatives[1]["name"], "Part");

    let assembly = declarations
        .iter()
        .find(|declaration| declaration["name"] == "assemble_reporter_region")
        .unwrap();
    assert_eq!(assembly["inputs"][0]["name"], "promoter_carrier");
    assert_eq!(assembly["inputs"][0]["type"]["name"], "Material");
    assert_eq!(assembly["outputs"][0]["name"], "outcome");
    let serialized = serde_json::to_string(assembly).unwrap();
    assert!(serialized.contains("std.bio.build.realize"));
    assert!(!serialized.contains("level1"));
    assert!(!serialized.contains("level2"));

    let host = declarations
        .iter()
        .find(|declaration| declaration["name"] == "build_reporter_host")
        .unwrap();
    assert_eq!(host["outputs"][0]["name"], "strain");
    assert_eq!(host["outputs"][1]["name"], "plate");
    assert!(
        serde_json::to_string(host)
            .unwrap()
            .contains("std.lab.plasmid_actions.transform")
    );
}
