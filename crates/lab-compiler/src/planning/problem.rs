//! Stable, facility-independent constraint input projected from refined LAIR.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{
    CapabilityKind, ControlMode, MethodId, OperationId, PropertyConstraint, PropertyKind,
    PropertyValue, QualificationLevel,
};
use lab_method::{IntentOperationId, LocalId, PortType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PLANNING_PROBLEM_SCHEMA_VERSION: &str = "lab.planning-problem.v1";

/// Every unresolved method choice and its complete Procedure requirement graph.
///
/// This record is immutable, serializable, and contains no Pliron handles or facility facts. A
/// facility planner combines it with one validated inventory snapshot and explicit adapter
/// bindings to construct the global allocation problem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningProblem {
    pub schema_version: String,
    pub choices: Vec<PlanningMethodChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningMethodChoice {
    pub id: LocalId,
    pub source_operation: IntentOperationId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<PlanningPort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<PlanningPort>,
    pub candidates: Vec<PlanningMethodCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningPort {
    pub name: LocalId,
    pub port_type: PortType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningMethodCandidate {
    pub method: MethodId,
    pub tasks: Vec<PlanningProcedureTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yields: Vec<PlanningMethodYield>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningProcedureTask {
    pub id: LocalId,
    pub operation: OperationId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<PlanningTaskInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<PlanningTaskOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<PlanningProcedureParameter>,
    pub requirements: Vec<PlanningCapabilityRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningTaskInput {
    pub source: PlanningValueSource,
    pub port_type: PortType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningTaskOutput {
    pub name: LocalId,
    pub port_type: PortType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningProcedureParameter {
    pub id: LocalId,
    pub property_kind: PropertyKind,
    pub value: PropertyValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningCapabilityRequirement {
    pub id: LocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<PropertyConstraint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningMethodYield {
    pub output: LocalId,
    pub source: PlanningValueSource,
}

/// A stable edge in one candidate Procedure graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanningValueSource {
    ChoiceInput { input: LocalId },
    TaskOutput { task: LocalId, output: LocalId },
}

impl PlanningProblem {
    /// Digest the canonical serde representation used to bind a solution to this exact problem.
    pub fn sha256(&self) -> String {
        let bytes = serde_json::to_vec(self)
            .expect("PlanningProblem contains only infallibly serializable semantic values");
        hex_sha256(&bytes)
    }

    /// Validate a deserialized problem before a solver or facility query consumes it.
    pub fn validate(&self) -> Result<(), PlanningProblemValidationError> {
        if self.schema_version != PLANNING_PROBLEM_SCHEMA_VERSION {
            return Err(PlanningProblemValidationError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        if self.choices.is_empty() {
            return Err(PlanningProblemValidationError::EmptyProblem);
        }
        let mut choices = BTreeSet::new();
        let mut task_ids = BTreeSet::new();
        let mut requirement_ids = BTreeSet::new();
        let mut parameter_ids = BTreeSet::new();
        for choice in &self.choices {
            if !choices.insert(choice.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateChoice {
                    choice: choice.id.clone(),
                });
            }
            validate_ports(&choice.id, "input", &choice.inputs)?;
            validate_ports(&choice.id, "output", &choice.outputs)?;
            if choice.candidates.is_empty() {
                return Err(PlanningProblemValidationError::EmptyChoice {
                    choice: choice.id.clone(),
                });
            }
            let mut methods = BTreeSet::new();
            for candidate in &choice.candidates {
                if !methods.insert(candidate.method.clone()) {
                    return Err(PlanningProblemValidationError::DuplicateCandidate {
                        choice: choice.id.clone(),
                        method: candidate.method.clone(),
                    });
                }
                validate_candidate(
                    choice,
                    candidate,
                    &mut task_ids,
                    &mut requirement_ids,
                    &mut parameter_ids,
                )?;
            }
        }
        Ok(())
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_ports(
    choice: &LocalId,
    kind: &'static str,
    ports: &[PlanningPort],
) -> Result<(), PlanningProblemValidationError> {
    let mut names = BTreeSet::new();
    if let Some(port) = ports.iter().find(|port| !names.insert(port.name.clone())) {
        return Err(PlanningProblemValidationError::DuplicatePort {
            choice: choice.clone(),
            kind,
            port: port.name.clone(),
        });
    }
    Ok(())
}

fn validate_candidate(
    choice: &PlanningMethodChoice,
    candidate: &PlanningMethodCandidate,
    global_tasks: &mut BTreeSet<LocalId>,
    global_requirements: &mut BTreeSet<LocalId>,
    global_parameters: &mut BTreeSet<LocalId>,
) -> Result<(), PlanningProblemValidationError> {
    if candidate.tasks.is_empty() {
        return Err(PlanningProblemValidationError::EmptyProcedure {
            choice: choice.id.clone(),
            method: candidate.method.clone(),
        });
    }
    let mut available = choice
        .inputs
        .iter()
        .map(|input| {
            (
                PlanningValueSource::ChoiceInput {
                    input: input.name.clone(),
                },
                input.port_type.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for task in &candidate.tasks {
        if !global_tasks.insert(task.id.clone()) {
            return Err(PlanningProblemValidationError::DuplicateTask {
                task: task.id.clone(),
            });
        }
        for input in &task.inputs {
            let Some(available_type) = available.get(&input.source) else {
                return Err(PlanningProblemValidationError::UnavailableValue {
                    task: task.id.clone(),
                    value_source: input.source.clone(),
                });
            };
            if available_type != &input.port_type {
                return Err(PlanningProblemValidationError::ValueTypeMismatch {
                    owner: task.id.clone(),
                    value_source: input.source.clone(),
                });
            }
        }
        let mut output_names = BTreeSet::new();
        for output in &task.outputs {
            if !output_names.insert(output.name.clone()) {
                return Err(PlanningProblemValidationError::DuplicateTaskOutput {
                    task: task.id.clone(),
                    output: output.name.clone(),
                });
            }
            available.insert(
                PlanningValueSource::TaskOutput {
                    task: task.id.clone(),
                    output: output.name.clone(),
                },
                output.port_type.clone(),
            );
        }
        for parameter in &task.parameters {
            if !global_parameters.insert(parameter.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateParameter {
                    parameter: parameter.id.clone(),
                });
            }
        }
        if task.requirements.is_empty() {
            return Err(PlanningProblemValidationError::MissingRequirement {
                task: task.id.clone(),
            });
        }
        for requirement in &task.requirements {
            if !global_requirements.insert(requirement.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateRequirement {
                    requirement: requirement.id.clone(),
                });
            }
            if requirement.accepted_control_modes.is_empty()
                || requirement
                    .accepted_control_modes
                    .contains(&ControlMode::Unspecified)
            {
                return Err(PlanningProblemValidationError::InvalidControlPolicy {
                    requirement: requirement.id.clone(),
                });
            }
        }
    }
    if candidate.yields.len() != choice.outputs.len() {
        return Err(PlanningProblemValidationError::YieldArity {
            choice: choice.id.clone(),
            method: candidate.method.clone(),
        });
    }
    for (expected, yielded) in choice.outputs.iter().zip(&candidate.yields) {
        if yielded.output != expected.name {
            return Err(PlanningProblemValidationError::YieldName {
                choice: choice.id.clone(),
                method: candidate.method.clone(),
                expected: expected.name.clone(),
                found: yielded.output.clone(),
            });
        }
        let Some(available_type) = available.get(&yielded.source) else {
            return Err(PlanningProblemValidationError::UnavailableYield {
                choice: choice.id.clone(),
                method: candidate.method.clone(),
                value_source: yielded.source.clone(),
            });
        };
        if available_type != &expected.port_type {
            return Err(PlanningProblemValidationError::ValueTypeMismatch {
                owner: choice.id.clone(),
                value_source: yielded.source.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PlanningProblemValidationError {
    #[error(
        "planning problem declares schema `{found}`, expected `{PLANNING_PROBLEM_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("planning problem contains no method choices")]
    EmptyProblem,
    #[error("method choice `{choice}` occurs more than once")]
    DuplicateChoice { choice: LocalId },
    #[error("method choice `{choice}` has no candidates")]
    EmptyChoice { choice: LocalId },
    #[error("method choice `{choice}` {kind} port `{port}` occurs more than once")]
    DuplicatePort {
        choice: LocalId,
        kind: &'static str,
        port: LocalId,
    },
    #[error("method choice `{choice}` repeats candidate `{method}`")]
    DuplicateCandidate { choice: LocalId, method: MethodId },
    #[error("method choice `{choice}` candidate `{method}` contains no Procedure tasks")]
    EmptyProcedure { choice: LocalId, method: MethodId },
    #[error("Procedure task `{task}` occurs more than once")]
    DuplicateTask { task: LocalId },
    #[error("Procedure task `{task}` output `{output}` occurs more than once")]
    DuplicateTaskOutput { task: LocalId, output: LocalId },
    #[error("Procedure task `{task}` references unavailable value `{value_source:?}`")]
    UnavailableValue {
        task: LocalId,
        value_source: PlanningValueSource,
    },
    #[error("value `{value_source:?}` has the wrong type for `{owner}`")]
    ValueTypeMismatch {
        owner: LocalId,
        value_source: PlanningValueSource,
    },
    #[error("Procedure parameter `{parameter}` occurs more than once")]
    DuplicateParameter { parameter: LocalId },
    #[error("Procedure task `{task}` has no Capability requirement")]
    MissingRequirement { task: LocalId },
    #[error("Capability requirement `{requirement}` occurs more than once")]
    DuplicateRequirement { requirement: LocalId },
    #[error("Capability requirement `{requirement}` has no concrete control policy")]
    InvalidControlPolicy { requirement: LocalId },
    #[error("method choice `{choice}` candidate `{method}` yields the wrong number of values")]
    YieldArity { choice: LocalId, method: MethodId },
    #[error(
        "method choice `{choice}` candidate `{method}` yields `{found}` where `{expected}` is required"
    )]
    YieldName {
        choice: LocalId,
        method: MethodId,
        expected: LocalId,
        found: LocalId,
    },
    #[error(
        "method choice `{choice}` candidate `{method}` yields unavailable value `{value_source:?}`"
    )]
    UnavailableYield {
        choice: LocalId,
        method: MethodId,
        value_source: PlanningValueSource,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deserialized_problem_is_revalidated_before_use() {
        let problem = PlanningProblem {
            schema_version: PLANNING_PROBLEM_SCHEMA_VERSION.to_owned(),
            choices: Vec::new(),
        };
        let json = serde_json::to_string(&problem).unwrap();
        let decoded: PlanningProblem = serde_json::from_str(&json).unwrap();

        assert_eq!(
            decoded.validate().unwrap_err(),
            PlanningProblemValidationError::EmptyProblem
        );
        assert_eq!(decoded.sha256().len(), 64);
    }
}
