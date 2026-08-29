use std::str::FromStr;

use lab_compiler::{CompilerSession, IrStage, PassPipeline, SessionError};

fn protocol_ir() -> &'static str {
    include_str!("fixtures/p_acceptance_protocol.ir")
}

#[test]
fn compiled_ir_round_trips_through_a_fresh_session() {
    let ir = protocol_ir();
    assert!(ir.contains("outlined_attributes:"));

    let mut parsed = CompilerSession::default();
    parsed.parse_ir(ir).unwrap();
    assert_eq!(
        parsed.detect_stage().unwrap(),
        IrStage::MethodSelectedProtocol
    );
    let pipeline =
        PassPipeline::from_str("builtin.module(protocol-check-material-linearity)").unwrap();
    parsed.run_pass_pipeline(&pipeline).unwrap();

    let reprinted = parsed.ir().unwrap();
    let mut reparsed = CompilerSession::default();
    reparsed.parse_ir(&reprinted).unwrap();
    reparsed
        .verify_stage(IrStage::MethodSelectedProtocol)
        .unwrap();
}

#[test]
fn parser_rejects_trailing_input_and_leaves_the_session_reusable() {
    let mut session = CompilerSession::default();
    let error = session
        .parse_ir(&format!("{} trailing-garbage", protocol_ir()))
        .unwrap_err();
    assert!(matches!(error, SessionError::ParseIr(_)));

    session.parse_ir(protocol_ir()).unwrap();
    session
        .verify_stage(IrStage::MethodSelectedProtocol)
        .unwrap();
}

#[test]
fn parsing_and_biological_verification_are_distinct_failures() {
    let invalid = protocol_ir().replace(
        "GCTAGCGGATCCATGACCATGATTACGCCAAGCTTGAATTCGAGCTCGGTACCCGGGGATCCTCTAGAGTCGACCTGCAGGCATGCAAGCTT",
        "GCTAGCN",
    );
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
