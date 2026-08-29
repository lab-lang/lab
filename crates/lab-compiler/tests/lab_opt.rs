use std::io::Write;
use std::process::{Command, Output, Stdio};

fn compiled_ir() -> &'static str {
    include_str!("fixtures/p_acceptance_protocol.ir")
}

fn run_with_stdin(arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lab-opt"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn optimizer_verifies_runs_a_named_pipeline_and_prints_ir() {
    let output = run_with_stdin(
        &[
            "--input-stage",
            "method-selected-protocol",
            "--pass-pipeline",
            "builtin.module(protocol-check-material-linearity)",
        ],
        compiled_ir(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("builtin.module @p_acceptance"));
    assert!(stdout.contains("outlined_attributes:"));
}

#[test]
fn optimizer_keeps_diagnostics_off_stdout() {
    let output = run_with_stdin(&[], "not compiler ir");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed to parse compiler IR from standard input"));
}

#[test]
fn optimizer_reports_non_local_material_linearity_failures() {
    let ir = compiled_ir();
    let assemble = ir
        .lines()
        .find(|line| line.contains(" = protocol.assemble "))
        .unwrap();
    let duplicate = assemble
        .rsplit_once(" !")
        .unwrap()
        .0
        .replace("assembled_construct", "duplicate_construct")
        + ";";
    let nonlinear = ir.replacen(assemble, &format!("{assemble}\n{duplicate}"), 1);

    let output = run_with_stdin(
        &[
            "--pass-pipeline",
            "builtin.module(protocol-check-material-linearity)",
        ],
        &nonlinear,
    );

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("physical material value has 2 consumers"),
        "{stderr}"
    );
    assert!(stderr.contains("use an explicit split or sample operation"));
}

#[test]
fn optimizer_lists_its_stable_facility_independent_pass_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_lab-opt"))
        .arg("--list-passes")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("protocol-check-material-linearity"));
    assert!(stdout.contains("method-selected-protocol -> method-selected-protocol"));
}

#[test]
fn optimizer_rejects_unknown_passes_before_reading_ir() {
    let output = Command::new(env!("CARGO_BIN_EXE_lab-opt"))
        .args(["--pass-pipeline", "does-not-exist"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown pass 'does-not-exist'"));
    assert!(stderr.contains("available passes: protocol-check-material-linearity"));
}
