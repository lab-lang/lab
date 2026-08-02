use std::path::PathBuf;
use std::process::{Command, Output};

use labc::{CompilerSession, IrStage};
use serde_json::Value;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
fn plasmid_acceptance_exposes_each_compiler_boundary() {
    let specification: Value =
        serde_json::from_str(&successful_stdout("specification-json")).unwrap();
    assert_eq!(specification["name"], "p_acceptance");
    assert_eq!(specification["copies"], 1);
    assert_eq!(specification["acceptance"].as_array().unwrap().len(), 3);

    let mut design = CompilerSession::default();
    design.parse_ir(&successful_stdout("design-ir")).unwrap();
    design.verify_stage(IrStage::Design).unwrap();

    let mut target = CompilerSession::default();
    target.parse_ir(&successful_stdout("target-ir")).unwrap();
    target
        .verify_stage(IrStage::TargetSelectedProtocol)
        .unwrap();

    let plan: Value = serde_json::from_str(&successful_stdout("plan-json")).unwrap();
    assert_eq!(plan["lab_profile"], "reference-lab");
    assert_eq!(plan["steps"].as_array().unwrap().len(), 13);
    assert_eq!(plan["acceptance"].as_array().unwrap().len(), 3);

    let human = successful_stdout("human");
    assert!(human.contains("Phase 5 — Accept or reject"));
    assert!(human.contains("13. Evaluate acceptance"));
    assert!(human.contains("[ ] Concentration is at least 100 ng/µL"));
    assert!(human.contains("[ ] Retained volume is at least 20 µL"));
    assert!(!human.contains("p_acceptance.sequence_evidence"));

    let simulation: Value = serde_json::from_str(&successful_stdout("simulation")).unwrap();
    assert_eq!(simulation["events"].as_array().unwrap().len(), 13);
    assert_eq!(simulation["acceptance"].as_array().unwrap().len(), 3);
}
