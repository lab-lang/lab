use std::path::PathBuf;
use std::process::Command;

#[test]
fn textual_ir_exposes_the_ordered_lowering_pipeline() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("sensor.lab");
    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([source.to_str().unwrap(), "--emit", "ir"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let ir = String::from_utf8(output.stdout).unwrap();
    let design = ir.find("design.plasmid").unwrap();
    let synthesize = ir.find("protocol.synthesize").unwrap();
    let assemble = ir.find("protocol.assemble").unwrap();
    let accept = ir.find("protocol.accept").unwrap();

    assert!(design < synthesize && synthesize < assemble && assemble < accept);
    assert!(ir.contains("copies: builtin.integer"));
    assert!(ir.contains("exact_sequence_required: builtin.bool"));
    assert!(ir.contains("minimum_concentration_ng_per_ul: builtin.integer"));
}
