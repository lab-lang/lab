use std::collections::BTreeSet;

use thiserror::Error;

use super::program::ThermalProgramV1;
use crate::ProcedureLocalId;

impl ThermalProgramV1 {
    pub fn validate(self) -> Result<ValidatedThermalProgramV1, ThermalProgramValidationError> {
        if self.load.sample_count == 0 {
            return Err(ThermalProgramValidationError::NoSamples);
        }
        if self.load.outputs.is_empty() {
            return Err(ThermalProgramValidationError::NoOutputs);
        }
        let mut outputs = BTreeSet::new();
        for output in &self.load.outputs {
            if !outputs.insert(output.clone()) {
                return Err(ThermalProgramValidationError::DuplicateOutput {
                    output: output.clone(),
                });
            }
        }
        if self.stages.is_empty() {
            return Err(ThermalProgramValidationError::NoStages);
        }
        let mut stage_ids = BTreeSet::new();
        let mut step_ids = BTreeSet::new();
        for stage in &self.stages {
            if !stage_ids.insert(stage.id.clone()) {
                return Err(ThermalProgramValidationError::DuplicateStage {
                    stage: stage.id.clone(),
                });
            }
            if stage.repeats == 0 {
                return Err(ThermalProgramValidationError::ZeroStageRepeats {
                    stage: stage.id.clone(),
                });
            }
            if stage.steps.is_empty() {
                return Err(ThermalProgramValidationError::EmptyStage {
                    stage: stage.id.clone(),
                });
            }
            for step in &stage.steps {
                if !step_ids.insert(step.id.clone()) {
                    return Err(ThermalProgramValidationError::DuplicateStep {
                        step: step.id.clone(),
                    });
                }
            }
        }
        Ok(ValidatedThermalProgramV1(self))
    }
}

/// A thermal program whose structure, quantities, and identities satisfy the V1 contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedThermalProgramV1(ThermalProgramV1);

impl ValidatedThermalProgramV1 {
    pub fn as_program(&self) -> &ThermalProgramV1 {
        &self.0
    }
}

impl AsRef<ThermalProgramV1> for ValidatedThermalProgramV1 {
    fn as_ref(&self) -> &ThermalProgramV1 {
        self.as_program()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ThermalProgramValidationError {
    #[error("thermal program has no samples")]
    NoSamples,
    #[error("thermal program has no outputs")]
    NoOutputs,
    #[error("thermal program repeats output `{output}`")]
    DuplicateOutput { output: ProcedureLocalId },
    #[error("thermal program has no stages")]
    NoStages,
    #[error("thermal program repeats stage `{stage}`")]
    DuplicateStage { stage: ProcedureLocalId },
    #[error("thermal stage `{stage}` has no steps")]
    EmptyStage { stage: ProcedureLocalId },
    #[error("thermal stage `{stage}` has zero repeats")]
    ZeroStageRepeats { stage: ProcedureLocalId },
    #[error("thermal program repeats step `{step}`")]
    DuplicateStep { step: ProcedureLocalId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thermal::program::test_program;

    #[test]
    fn structure_and_identity_validation_fail_closed() {
        let mut invalid = test_program();
        invalid.load.sample_count = 0;
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::NoSamples)
        ));

        let mut invalid = test_program();
        invalid.load.outputs.clear();
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::NoOutputs)
        ));

        let mut invalid = test_program();
        let duplicate = invalid.load.outputs[0].clone();
        invalid.load.outputs.push(duplicate);
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::DuplicateOutput { .. })
        ));

        let mut invalid = test_program();
        invalid.stages[0].repeats = 0;
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::ZeroStageRepeats { .. })
        ));

        let mut invalid = test_program();
        let duplicate = invalid.stages[0].steps[0].clone();
        invalid.stages[0].steps.push(duplicate);
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::DuplicateStep { .. })
        ));
    }
}
