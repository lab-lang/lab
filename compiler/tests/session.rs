use std::str::FromStr;

use labc::{
    AcceptanceCriterion, ArtifactSpec, Concentration, DnaSequence, LabProfile, PlasmidSpec,
    Topology, Volume,
};
use labc::{Compiler, CompilerSession, IrStage, PassPipeline, SessionError};

fn specification() -> ArtifactSpec {
    ArtifactSpec::plasmid(
        "p_session",
        PlasmidSpec::new(
            DnaSequence::new("ATGCGTACGTTAGCTA").unwrap(),
            Topology::Circular,
        )
        .unwrap(),
        1,
        vec![
            AcceptanceCriterion::ExactSequence,
            AcceptanceCriterion::MinimumConcentration {
                concentration: Concentration::nanograms_per_microliter(100),
            },
            AcceptanceCriterion::MinimumVolume {
                volume: Volume::microliters(20),
            },
        ],
    )
    .unwrap()
}

#[test]
fn compiled_ir_round_trips_through_a_fresh_session() {
    let compilation = Compiler
        .compile(&specification(), &LabProfile::reference())
        .unwrap();
    let ir = compilation.ir();
    assert!(ir.contains("outlined_attributes:"));

    let mut parsed = CompilerSession::default();
    parsed.parse_ir(&ir).unwrap();
    assert_eq!(
        parsed.detect_stage().unwrap(),
        IrStage::TargetSelectedProtocol
    );
    let pipeline =
        PassPipeline::from_str("builtin.module(protocol-check-material-linearity)").unwrap();
    parsed.run_pass_pipeline(&pipeline).unwrap();

    let reprinted = parsed.ir().unwrap();
    let mut reparsed = CompilerSession::default();
    reparsed.parse_ir(&reprinted).unwrap();
    reparsed
        .verify_stage(IrStage::TargetSelectedProtocol)
        .unwrap();
}

#[test]
fn parser_rejects_trailing_input_and_leaves_the_session_reusable() {
    let compilation = Compiler
        .compile(&specification(), &LabProfile::reference())
        .unwrap();
    let mut session = CompilerSession::default();
    let error = session
        .parse_ir(&format!("{} trailing-garbage", compilation.ir()))
        .unwrap_err();
    assert!(matches!(error, SessionError::ParseIr(_)));

    session.import_specification(&specification()).unwrap();
    session.verify_stage(IrStage::Design).unwrap();
}

#[test]
fn parsing_and_biological_verification_are_distinct_failures() {
    let compilation = Compiler
        .compile(&specification(), &LabProfile::reference())
        .unwrap();
    let invalid = compilation
        .ir()
        .replace("ATGCGTACGTTAGCTA", "ATGCGTACGTTAGCTAN");
    let mut session = CompilerSession::default();

    session.parse_ir(&invalid).unwrap();
    let error = session.verify().unwrap_err();
    let SessionError::VerificationFailed(diagnostic) = error else {
        panic!("expected biological verification failure");
    };
    assert!(
        diagnostic
            .contains("design.plasmid sequence must be non-empty, uppercase, and unambiguous DNA")
    );
}

#[test]
fn pass_stage_preconditions_reject_design_ir() {
    let mut session = CompilerSession::default();
    session.import_specification(&specification()).unwrap();
    let pipeline = PassPipeline::from_str("protocol-check-material-linearity").unwrap();

    let error = session.run_pass_pipeline(&pipeline).unwrap_err();
    let SessionError::StageContract(diagnostic) = error else {
        panic!("expected stage-contract failure");
    };
    assert!(diagnostic.contains("expected target-selected-protocol IR"));
}
