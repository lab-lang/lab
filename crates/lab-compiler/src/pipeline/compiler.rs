use super::{Compilation, CompilerError};
use crate::CompilerSession;
use crate::{ArtifactSpec, LabProfile};

#[derive(Clone, Copy, Debug, Default)]
pub struct Compiler;

impl Compiler {
    pub fn compile(
        &self,
        specification: &ArtifactSpec,
        lab: &LabProfile,
    ) -> Result<Compilation, CompilerError> {
        specification.validate()?;
        let mut session = CompilerSession::default();
        session.import_specification(specification)?;
        session.verify_stage(crate::IrStage::Design)?;
        let assembly = session.resolve_target(lab)?;
        session.run_default_pipeline(lab, assembly)?;

        let plan = session.export_plan(lab.name())?;
        plan.validate()?;
        Ok(session.finish(plan)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AcceptanceCriterion, ArtifactSpec, Capability, Concentration, DnaSequence, LabProfile,
        PlasmidSpec, Topology, Volume,
    };

    use super::*;

    fn specification() -> ArtifactSpec {
        ArtifactSpec::plasmid(
            "p_sensor",
            PlasmidSpec::new(DnaSequence::new("ACGTACGT").unwrap(), Topology::Circular).unwrap(),
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
    fn compiles_through_verified_protocol_ir_into_a_plan() {
        let compilation = Compiler
            .compile(&specification(), &LabProfile::reference())
            .unwrap();
        let ir = compilation.ir();

        assert!(ir.contains("design.plasmid"));
        assert!(ir.contains("protocol.sample"));
        assert!(ir.contains("protocol.sequence"));
        assert!(ir.contains("protocol.quantify"));
        assert!(ir.contains("minimum_concentration_ng_per_ul"));
        assert!(ir.contains("protocol.accept"));
        assert_eq!(compilation.plan().steps.len(), 13);
        compilation.plan().validate().unwrap();
    }

    #[test]
    fn compilation_fails_closed_on_missing_target_capability() {
        let lab = LabProfile::new(
            "under-equipped",
            [
                Capability::DnaSynthesis,
                Capability::GibsonAssembly,
                Capability::ChemicalTransformation,
                Capability::CultureIncubation,
                Capability::AntibioticSelection,
                Capability::CloneScreening,
                Capability::PlasmidPurification,
            ],
            "DH5alpha",
        );

        let error = match Compiler.compile(&specification(), &lab) {
            Ok(_) => panic!("expected compilation to fail"),
            Err(error) => error,
        };
        let CompilerError::MissingCapabilities(missing) = error else {
            panic!("expected missing capabilities error");
        };
        assert_eq!(
            missing,
            vec![Capability::SangerSequencing, Capability::DnaQuantification]
        );
    }

    #[test]
    fn compilation_does_not_silently_ignore_requested_replicates() {
        let specification = ArtifactSpec::plasmid(
            "p_replicated",
            PlasmidSpec::new(DnaSequence::new("ACGT").unwrap(), Topology::Circular).unwrap(),
            2,
            vec![AcceptanceCriterion::ExactSequence],
        )
        .unwrap();

        let error = match Compiler.compile(&specification, &LabProfile::reference()) {
            Ok(_) => panic!("expected compilation to fail"),
            Err(error) => error,
        };
        assert!(matches!(error, CompilerError::UnsupportedCopyCount(2)));
    }
}
