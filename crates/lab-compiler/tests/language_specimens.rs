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
        declaration["kind"] == "plasmid"
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
    assert_eq!(workflow["output"]["kind"], "union");
    assert_eq!(workflow["output"]["alternatives"][0]["name"], "Accepted");
    assert_eq!(workflow["output"]["alternatives"][1]["name"], "Rejected");
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
