use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("plasmid-acceptance")
        .join("p_acceptance.lab")
}

fn emit(kind: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([example().to_str().unwrap(), "--emit", kind])
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
    assert_eq!(source_ast["items"][0]["item"], "artifact");
    assert_eq!(source_ast["items"][0]["kind"], "plasmid");
    assert_eq!(source_ast["items"][0]["name"]["value"], "p_acceptance");

    let module: Value = serde_json::from_str(&successful_stdout("module-ir")).unwrap();
    assert_eq!(module["declarations"][0]["kind"], "artifact");
    assert_eq!(module["declarations"][0]["artifact"], "plasmid");
    assert_eq!(module["declarations"][0]["name"], "p_acceptance");
    assert_eq!(
        module["declarations"][0]["acceptance"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let human = successful_stdout("human");
    assert!(human.contains("Lab module compiled"));
    assert!(human.contains("plasmid p_acceptance"));
    assert!(human.contains("3 acceptance claims"));
    assert!(human.contains("no laboratory target was selected or executed"));
}
