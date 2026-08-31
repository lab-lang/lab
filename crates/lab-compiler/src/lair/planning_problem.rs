//! Read-only projection from verifier-valid refined LAIR into the solver boundary.

use std::collections::BTreeMap;

use lab_method::LocalId;
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::r#type::Typed;
use pliron::value::Value;
use thiserror::Error;

use crate::lair::dialect::capability::{ConstraintOp, RequirementOp};
use crate::lair::dialect::method::{ChoiceOp, YieldOp};
use crate::lair::dialect::procedure::{MaterialInputOp, ParameterOp, TaskOp, semantic_port_type};
use crate::lair::stage::{IrStage, detect_stage};
use crate::planning::{
    PLANNING_PROBLEM_SCHEMA_VERSION, PlanningCapabilityRequirement, PlanningMaterialInput,
    PlanningMaterialSource, PlanningMethodCandidate, PlanningMethodChoice, PlanningMethodYield,
    PlanningPort, PlanningProblem, PlanningProblemValidationError, PlanningProcedureParameter,
    PlanningProcedureTask, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
};

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PlanningProblemExtractionError {
    #[error("cannot extract a planning problem from invalid LAIR: {0}")]
    InvalidStage(String),
    #[error("expected refined-alternatives LAIR, found {0}")]
    WrongStage(IrStage),
    #[error("refined LAIR is missing the builtin.module entry block")]
    MissingModuleBlock,
    #[error("method choice `{choice}` candidate {candidate} is missing its entry block")]
    MissingCandidateBlock { choice: LocalId, candidate: usize },
    #[error("method choice `{choice}` candidate {candidate} has no method.yield terminator")]
    MissingYield { choice: LocalId, candidate: usize },
    #[error("{owner} contains a value whose Procedure port type cannot be decoded")]
    UnsupportedPortType { owner: String },
    #[error("{owner} references a value that has no stable source identity")]
    UnresolvedValue { owner: String },
    #[error("Procedure metadata references missing task `{task}`")]
    MissingTask { task: LocalId },
    #[error("Capability constraint references missing requirement `{requirement}`")]
    MissingRequirement { requirement: LocalId },
    #[error("Capability constraint for `{requirement}` is invalid: {message}")]
    InvalidConstraint {
        requirement: LocalId,
        message: String,
    },
    #[error("several method choices claim to realize artifact `{artifact}`")]
    DuplicateArtifactChoice { artifact: String },
    #[error(transparent)]
    InvalidProblem(#[from] PlanningProblemValidationError),
}

pub(crate) fn extract_planning_problem(
    context: &Context,
    module: ModuleOp,
) -> Result<PlanningProblem, PlanningProblemExtractionError> {
    let stage =
        detect_stage(context, module).map_err(PlanningProblemExtractionError::InvalidStage)?;
    if stage != IrStage::RefinedAlternatives {
        return Err(PlanningProblemExtractionError::WrongStage(stage));
    }
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or(PlanningProblemExtractionError::MissingModuleBlock)?;
    let choice_operations = block
        .deref(context)
        .iter(context)
        .filter_map(|operation| Operation::get_op::<ChoiceOp>(operation, context))
        .collect::<Vec<_>>();
    let choice_outputs = choice_operations
        .iter()
        .flat_map(|choice| {
            let choice_id = choice.semantic_choice_id(context);
            let results = choice
                .get_operation()
                .deref(context)
                .results()
                .collect::<Vec<_>>();
            results
                .into_iter()
                .zip(choice.output_names(context))
                .map(move |(value, output)| {
                    (
                        value,
                        PlanningValueSource::ChoiceOutput {
                            choice: choice_id.clone(),
                            output,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut artifact_choices = BTreeMap::new();
    for choice in &choice_operations {
        let Some(artifact) = choice.artifact_name(context) else {
            continue;
        };
        if artifact_choices
            .insert(artifact.clone(), choice.semantic_choice_id(context))
            .is_some()
        {
            return Err(PlanningProblemExtractionError::DuplicateArtifactChoice { artifact });
        }
    }
    let choices = choice_operations
        .iter()
        .map(|choice| extract_choice(context, choice, &choice_outputs, &artifact_choices))
        .collect::<Result<Vec<_>, _>>()?;
    let problem = PlanningProblem {
        schema_version: PLANNING_PROBLEM_SCHEMA_VERSION.to_owned(),
        choices,
    };
    problem.validate()?;
    Ok(problem)
}

fn extract_choice(
    context: &Context,
    choice: &ChoiceOp,
    choice_outputs: &[(Value, PlanningValueSource)],
    artifact_choices: &BTreeMap<String, LocalId>,
) -> Result<PlanningMethodChoice, PlanningProblemExtractionError> {
    let choice_id = choice.semantic_choice_id(context);
    let operation = choice.get_operation().deref(context);
    let inputs = choice
        .input_names(context)
        .into_iter()
        .zip(operation.operands())
        .map(|(name, value)| {
            Ok(PlanningPort {
                name,
                port_type: decode_port_type(
                    context,
                    value,
                    format!("method choice `{choice_id}` input"),
                )?,
                source: source_for_value(choice_outputs, value),
            })
        })
        .collect::<Result<Vec<_>, PlanningProblemExtractionError>>()?;
    let outputs = choice
        .output_names(context)
        .into_iter()
        .zip(operation.results())
        .map(|(name, value)| {
            Ok(PlanningPort {
                name,
                port_type: decode_port_type(
                    context,
                    value,
                    format!("method choice `{choice_id}` output"),
                )?,
                source: None,
            })
        })
        .collect::<Result<Vec<_>, PlanningProblemExtractionError>>()?;
    let methods = choice.candidate_ids(context);
    let after = choice
        .dependency_artifacts(context)
        .into_iter()
        .filter_map(|artifact| artifact_choices.get(&artifact).cloned())
        .collect();
    let candidates = methods
        .into_iter()
        .enumerate()
        .map(|(candidate, method)| {
            extract_candidate(
                context,
                choice,
                &choice_id,
                candidate,
                method,
                artifact_choices,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlanningMethodChoice {
        id: choice_id,
        source_operation: choice.source_operation(context),
        after,
        inputs,
        outputs,
        candidates,
    })
}

fn extract_candidate(
    context: &Context,
    choice: &ChoiceOp,
    choice_id: &LocalId,
    candidate_index: usize,
    method: lab_capability::MethodId,
    artifact_choices: &BTreeMap<String, LocalId>,
) -> Result<PlanningMethodCandidate, PlanningProblemExtractionError> {
    let block = choice
        .candidate_region(context, candidate_index)
        .deref(context)
        .get_head()
        .ok_or_else(|| PlanningProblemExtractionError::MissingCandidateBlock {
            choice: choice_id.clone(),
            candidate: candidate_index,
        })?;
    let choice_operation = choice.get_operation().deref(context);
    let mut values = choice_operation
        .operands()
        .zip(choice.input_names(context))
        .map(|(value, input)| (value, PlanningValueSource::ChoiceInput { input }))
        .collect::<Vec<_>>();
    let mut tasks = Vec::new();
    let mut task_indexes = BTreeMap::new();
    let mut requirements = BTreeMap::new();
    let mut constraint_targets = Vec::new();
    let mut yield_operation = None;

    for operation in block.deref(context).iter(context) {
        if let Some(task) = Operation::get_op::<TaskOp>(operation, context) {
            let task_id = task.semantic_node_id(context);
            let inputs = operation
                .deref(context)
                .operands()
                .map(|value| {
                    let source = source_for_value(&values, value).ok_or_else(|| {
                        PlanningProblemExtractionError::UnresolvedValue {
                            owner: format!("Procedure task `{task_id}`"),
                        }
                    })?;
                    Ok(PlanningTaskInput {
                        source,
                        port_type: decode_port_type(
                            context,
                            value,
                            format!("Procedure task `{task_id}` input"),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, PlanningProblemExtractionError>>()?;
            let output_names = task.output_names(context);
            let outputs = operation
                .deref(context)
                .results()
                .zip(output_names)
                .map(|(value, name)| {
                    let source = PlanningValueSource::TaskOutput {
                        task: task_id.clone(),
                        output: name.clone(),
                    };
                    values.push((value, source));
                    Ok(PlanningTaskOutput {
                        name,
                        port_type: decode_port_type(
                            context,
                            value,
                            format!("Procedure task `{task_id}` output"),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, PlanningProblemExtractionError>>()?;
            task_indexes.insert(task_id.clone(), tasks.len());
            let program = task.semantic_program(context);
            let binding_scope = program
                .as_ref()
                .map(|program| {
                    program
                        .validate()
                        .expect("verified Procedure program revalidates during planning projection")
                        .capability_formula()
                        .binding_scope
                })
                .unwrap_or_default();
            tasks.push(PlanningProcedureTask {
                id: task_id,
                operation: task.semantic_operation(context),
                program,
                binding_scope,
                inputs,
                outputs,
                parameters: Vec::new(),
                materials: Vec::new(),
                requirements: Vec::new(),
            });
            continue;
        }
        if let Some(material) = Operation::get_op::<MaterialInputOp>(operation, context) {
            let task = LocalId::new(material.procedure_node(context))
                .expect("verified Procedure node references are stable IDs");
            let Some(task_index) = task_indexes.get(&task).copied() else {
                return Err(PlanningProblemExtractionError::MissingTask { task });
            };
            let symbol = material.symbol(context);
            let source =
                artifact_choices
                    .get(&symbol)
                    .map_or(PlanningMaterialSource::Inventory, |choice| {
                        PlanningMaterialSource::ChoiceOutput {
                            choice: choice.clone(),
                        }
                    });
            tasks[task_index].materials.push(PlanningMaterialInput {
                id: LocalId::new(material.input_id(context))
                    .expect("verified material input identities are stable IDs"),
                symbol,
                source,
            });
            continue;
        }
        if let Some(parameter) = Operation::get_op::<ParameterOp>(operation, context) {
            let task = LocalId::new(parameter.procedure_node(context))
                .expect("verified Procedure node references are stable IDs");
            let Some(task_index) = task_indexes.get(&task).copied() else {
                return Err(PlanningProblemExtractionError::MissingTask { task });
            };
            let (id, property_kind, value) = parameter.semantic_parameter(context);
            tasks[task_index]
                .parameters
                .push(PlanningProcedureParameter {
                    id,
                    property_kind,
                    value,
                });
            continue;
        }
        if let Some(requirement) = Operation::get_op::<RequirementOp>(operation, context) {
            let task = LocalId::new(requirement.procedure_node(context))
                .expect("verified Procedure node references are stable IDs");
            let Some(task_index) = task_indexes.get(&task).copied() else {
                return Err(PlanningProblemExtractionError::MissingTask { task });
            };
            let id = LocalId::new(requirement.requirement_id(context))
                .expect("verified Capability requirement identities are stable IDs");
            let requirement_index = tasks[task_index].requirements.len();
            tasks[task_index]
                .requirements
                .push(PlanningCapabilityRequirement {
                    id: id.clone(),
                    capability_kind: requirement.semantic_capability_kind(context),
                    minimum_qualification: requirement.semantic_minimum_qualification(context),
                    accepted_control_modes: requirement.semantic_control_modes(context),
                    constraints: Vec::new(),
                });
            requirements.insert(id, (task_index, requirement_index));
            continue;
        }
        if let Some(constraint) = Operation::get_op::<ConstraintOp>(operation, context) {
            constraint_targets.push(constraint);
            continue;
        }
        if let Some(yield_op) = Operation::get_op::<YieldOp>(operation, context) {
            yield_operation = Some(yield_op);
        }
    }

    for constraint in constraint_targets {
        let requirement = LocalId::new(constraint.requirement_id(context))
            .expect("verified Capability constraint references are stable IDs");
        let Some((task_index, requirement_index)) = requirements.get(&requirement).copied() else {
            return Err(PlanningProblemExtractionError::MissingRequirement { requirement });
        };
        let decoded = constraint.decode(context).map_err(|message| {
            PlanningProblemExtractionError::InvalidConstraint {
                requirement: requirement.clone(),
                message,
            }
        })?;
        tasks[task_index].requirements[requirement_index]
            .constraints
            .push(decoded);
    }

    let yield_op = yield_operation.ok_or_else(|| PlanningProblemExtractionError::MissingYield {
        choice: choice_id.clone(),
        candidate: candidate_index,
    })?;
    let yields = choice
        .output_names(context)
        .into_iter()
        .zip(yield_op.get_operation().deref(context).operands())
        .map(|(output, value)| {
            let source = source_for_value(&values, value).ok_or_else(|| {
                PlanningProblemExtractionError::UnresolvedValue {
                    owner: format!("method choice `{choice_id}` yield"),
                }
            })?;
            Ok(PlanningMethodYield { output, source })
        })
        .collect::<Result<Vec<_>, PlanningProblemExtractionError>>()?;
    Ok(PlanningMethodCandidate {
        method,
        tasks,
        yields,
    })
}

fn source_for_value(
    values: &[(Value, PlanningValueSource)],
    value: Value,
) -> Option<PlanningValueSource> {
    values
        .iter()
        .find(|(candidate, _)| *candidate == value)
        .map(|(_, source)| source.clone())
}

fn decode_port_type(
    context: &Context,
    value: Value,
    owner: String,
) -> Result<lab_method::PortType, PlanningProblemExtractionError> {
    semantic_port_type(context, value.get_type(context))
        .ok_or(PlanningProblemExtractionError::UnsupportedPortType { owner })
}
