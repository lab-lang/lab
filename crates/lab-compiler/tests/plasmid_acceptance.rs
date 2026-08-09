use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("p_acceptance.lab")
}

fn emit(kind: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([fixture().to_str().unwrap(), "--emit", kind])
        .output()
        .unwrap()
}

fn successful_stdout(kind: &str) -> String {
    let output = emit(kind);
    assert!(
        output.status.success(),
        "--emit {kind} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn plasmid_acceptance_uses_the_canonical_frontend_boundary() {
    let source_ast: Value = serde_json::from_str(&successful_stdout("source-ast")).unwrap();
    // The kinds are imported, so the artifact is not the first item.
    let artifact = source_ast["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["item"] == "artifact")
        .expect("the specimen declares an artifact");
    assert_eq!(artifact["kind"]["value"], "plasmid");
    assert_eq!(artifact["name"]["value"], "p_acceptance");

    let module: Value = serde_json::from_str(&successful_stdout("module-ir")).unwrap();
    let declared = module["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|declaration| declaration["kind"] == "artifact")
        .expect("the module lowers an artifact");
    assert_eq!(declared["artifact"], "plasmid");
    assert_eq!(declared["name"], "p_acceptance");
    assert_eq!(declared["acceptance"].as_array().unwrap().len(), 3);

    let human = successful_stdout("human");
    assert!(human.contains("Lab module compiled"));
    assert!(human.contains("plasmid p_acceptance"));
    assert!(human.contains("3 acceptance claims"));
    assert!(human.contains("no laboratory target was selected or executed"));
}
