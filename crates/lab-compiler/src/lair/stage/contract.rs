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
