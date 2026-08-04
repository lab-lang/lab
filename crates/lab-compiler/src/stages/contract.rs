use super::IrStage;

/// Structural contract for a named, verifier-valid Lab Compiler IR stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageContract {
    stage: IrStage,
}

impl StageContract {
    pub fn for_stage(stage: IrStage) -> Self {
        Self { stage }
    }

    pub fn stage(self) -> IrStage {
        self.stage
    }

    pub(crate) fn verify(self, actual: IrStage) -> Result<(), String> {
        if actual != self.stage {
            return Err(format!(
                "expected {} IR, but the module satisfies the {} stage",
                self.stage, actual
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{AcceptanceCriterion, ArtifactSpec, DnaSequence, PlasmidSpec, Topology};
    use pliron::context::Context;

    use crate::ir::detect_stage;
    use crate::translations::lower_specification_to_design;

    use super::*;

    #[test]
    fn recognizes_verified_design_ir() {
        let specification = ArtifactSpec::plasmid(
            "p_stage",
            PlasmidSpec::new(DnaSequence::new("ACGT").unwrap(), Topology::Circular).unwrap(),
            1,
            vec![AcceptanceCriterion::ExactSequence],
        )
        .unwrap();
        let mut context = Context::new();
        let module = lower_specification_to_design(&mut context, &specification).unwrap();

        assert_eq!(detect_stage(&context, module).unwrap(), IrStage::Design);
    }
}
