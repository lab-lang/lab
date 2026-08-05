use std::collections::{BTreeMap, BTreeSet};

use crate::planning::protocol::AcceptanceCriterion;
use thiserror::Error;

use crate::planning::protocol::{OperationKind, PlanValue, ProtocolPlan, ValueKind};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PlanError {
    #[error("duplicate step id '{0}'")]
    DuplicateStep(String),
    #[error("duplicate plan value '{0}'")]
    DuplicateValue(String),
    #[error("step '{step}' references unknown input '{input}'")]
    UnknownInput { step: String, input: String },
    #[error("material value '{value}' is consumed more than once")]
    MaterialConsumedMoreThanOnce { value: String },
    #[error("acceptance criterion references unknown evidence step '{0}'")]
    UnknownEvidenceStep(String),
    #[error("acceptance evidence step '{0}' has the wrong operation kind")]
    InvalidEvidenceStep(String),
    #[error(
        "acceptance criterion references evidence value '{value}' not produced by step '{step}'"
    )]
    InvalidEvidenceValue { step: String, value: String },
    #[error("acceptance evidence value '{0}' is not consumed by an acceptance step")]
    UnconsumedAcceptanceEvidence(String),
}

impl ProtocolPlan {
    /// Verify value provenance, affine material use, and evidence references.
    pub fn validate(&self) -> Result<(), PlanError> {
        let mut step_kinds = BTreeMap::new();
        let mut steps_by_id = BTreeMap::new();
        for step in &self.steps {
            if step_kinds.insert(step.id.clone(), step.operation).is_some() {
                return Err(PlanError::DuplicateStep(step.id.clone()));
            }
            steps_by_id.insert(step.id.clone(), step);
        }

        let mut values = BTreeMap::new();
        for value in &self.initial_values {
            insert_value(&mut values, value)?;
        }

        let mut consumed_materials = BTreeSet::new();
        for step in &self.steps {
            for input in &step.inputs {
                let kind = values.get(input).ok_or_else(|| PlanError::UnknownInput {
                    step: step.id.clone(),
                    input: input.clone(),
                })?;
                if *kind == ValueKind::Material && !consumed_materials.insert(input.clone()) {
                    return Err(PlanError::MaterialConsumedMoreThanOnce {
                        value: input.clone(),
                    });
                }
            }
            for output in &step.outputs {
                insert_value(&mut values, output)?;
            }
        }

        for obligation in &self.acceptance {
            let Some(operation) = step_kinds.get(&obligation.evidence_step) else {
                return Err(PlanError::UnknownEvidenceStep(
                    obligation.evidence_step.clone(),
                ));
            };
            let expected_operation = match obligation.criterion {
                AcceptanceCriterion::ExactSequence => OperationKind::Sequence,
                AcceptanceCriterion::MinimumConcentration { .. }
                | AcceptanceCriterion::MinimumVolume { .. } => OperationKind::Quantify,
            };
            if *operation != expected_operation {
                return Err(PlanError::InvalidEvidenceStep(
                    obligation.evidence_step.clone(),
                ));
            }
            let evidence_step = steps_by_id[&obligation.evidence_step];
            if !evidence_step.outputs.iter().any(|output| {
                output.kind == ValueKind::Evidence && output.name == obligation.evidence_value
            }) {
                return Err(PlanError::InvalidEvidenceValue {
                    step: obligation.evidence_step.clone(),
                    value: obligation.evidence_value.clone(),
                });
            }
            if !self.steps.iter().any(|step| {
                step.operation == OperationKind::Accept
                    && step.inputs.contains(&obligation.evidence_value)
            }) {
                return Err(PlanError::UnconsumedAcceptanceEvidence(
                    obligation.evidence_value.clone(),
                ));
            }
        }
        Ok(())
    }
}

fn insert_value(
    values: &mut BTreeMap<String, ValueKind>,
    value: &PlanValue,
) -> Result<(), PlanError> {
    if values.insert(value.name.clone(), value.kind).is_some() {
        return Err(PlanError::DuplicateValue(value.name.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::planning::protocol::{AcceptanceObligation, PlanStep};

    use crate::planning::protocol::validation::*;

    #[test]
    fn rejects_implicit_material_copying() {
        let plan = ProtocolPlan {
            artifact: "p_test".into(),
            lab_profile: "test".into(),
            initial_values: vec![PlanValue::material("tube")],
            steps: vec![
                PlanStep::new(
                    "consume_once",
                    OperationKind::Purify,
                    ["tube"],
                    [PlanValue::material("first")],
                ),
                PlanStep::new(
                    "consume_twice",
                    OperationKind::Purify,
                    ["tube"],
                    [PlanValue::material("second")],
                ),
            ],
            acceptance: vec![],
        };

        assert_eq!(
            plan.validate(),
            Err(PlanError::MaterialConsumedMoreThanOnce {
                value: "tube".into()
            })
        );
    }

    #[test]
    fn rejects_acceptance_evidence_not_connected_to_accept() {
        let plan = ProtocolPlan {
            artifact: "p_test".into(),
            lab_profile: "test".into(),
            initial_values: vec![
                PlanValue::material("aliquot"),
                PlanValue::material("retained"),
            ],
            steps: vec![
                PlanStep::new(
                    "sequence",
                    OperationKind::Sequence,
                    ["aliquot"],
                    [PlanValue::evidence("sequence_evidence")],
                ),
                PlanStep::new(
                    "accept",
                    OperationKind::Accept,
                    ["retained"],
                    [PlanValue::material("validated")],
                ),
            ],
            acceptance: vec![AcceptanceObligation {
                criterion: AcceptanceCriterion::ExactSequence,
                evidence_step: "sequence".into(),
                evidence_value: "sequence_evidence".into(),
            }],
        };

        assert_eq!(
            plan.validate(),
            Err(PlanError::UnconsumedAcceptanceEvidence(
                "sequence_evidence".into()
            ))
        );
    }
}
