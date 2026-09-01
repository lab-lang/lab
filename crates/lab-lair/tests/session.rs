use std::str::FromStr;

use lab_lair::pipeline::PassPipeline;
use lab_lair::session::{CompilerSession, SessionError};
use lab_lair::stage::IrStage;

fn allocated_ir() -> &'static str {
    include_str!("fixtures/allocated_acceptance.ir")
}

#[test]
fn compiled_ir_round_trips_through_a_fresh_session() {
    let ir = allocated_ir();
    assert!(ir.contains("allocation.context"));

    let mut parsed = CompilerSession::default();
    parsed.parse_ir(ir).unwrap();
    assert_eq!(parsed.detect_stage().unwrap(), IrStage::AllocatedProcedure);
    let pipeline = PassPipeline::from_str("builtin.module(check-material-linearity)").unwrap();
    parsed.run_pass_pipeline(&pipeline).unwrap();

    let reprinted = parsed.ir().unwrap();
    let mut reparsed = CompilerSession::default();
    reparsed.parse_ir(&reprinted).unwrap();
    reparsed.verify_stage(IrStage::AllocatedProcedure).unwrap();
}

#[test]
fn parser_rejects_trailing_input_and_leaves_the_session_reusable() {
    let mut session = CompilerSession::default();
    let error = session
        .parse_ir(&format!("{} trailing-garbage", allocated_ir()))
        .unwrap_err();
    assert!(matches!(error, SessionError::ParseIr(_)));

    session.parse_ir(allocated_ir()).unwrap();
    session.verify_stage(IrStage::AllocatedProcedure).unwrap();
}

#[test]
fn parsing_and_biological_verification_are_distinct_failures() {
    let invalid = allocated_ir().replace("ACGT", "ACGN");
    let mut session = CompilerSession::default();

    session.parse_ir(&invalid).unwrap();
    let error = session.verify().unwrap_err();
    let SessionError::VerificationFailed(diagnostic) = error else {
        panic!("expected biological verification failure");
    };
    assert!(diagnostic.contains(
        "design.dna_sequence elements must be non-empty, uppercase, and unambiguous DNA"
    ));
}

#[test]
fn stage_identity_is_explicit_and_must_match_the_module_structure() {
    let missing = allocated_ir().replace(
        "    lair.stage () [] [stage: builtin.string \"allocated-procedure\"]: <() -> ()>;\n",
        "",
    );
    let mut session = CompilerSession::default();
    session.parse_ir(&missing).unwrap();
    let error = session.detect_stage().unwrap_err().to_string();
    assert!(
        error.contains("requires exactly one lair.stage marker, found 0"),
        "{error}"
    );

    let mismatched = allocated_ir().replace("allocated-procedure", "design-intent");
    let mut session = CompilerSession::default();
    session.parse_ir(&mismatched).unwrap();
    let error = session.detect_stage().unwrap_err().to_string();
    assert!(
        error.contains("operation 'allocation.context' belongs to dialect 'allocation'"),
        "{error}"
    );
}
