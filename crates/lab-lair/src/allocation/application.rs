//! Apply one complete facility solution to the refined LAIR that produced it.

use std::collections::BTreeMap;

use crate::method::LocalId;
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::irbuild::cloning::{IrMapping, clone_operation};
use pliron::irbuild::inserter::{Inserter, OpInsertionPoint};
use pliron::irbuild::listener::DummyListener;
use pliron::irbuild::rewriter::{IRRewriter, Rewriter};
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;
use thiserror::Error;

use crate::allocation::ir::{BindingOp, ContextOp, MaterialBindingOp, MethodOp, ParameterMatchOp};
use crate::capability::ir::RequirementOp;
use crate::method::ir::{ChoiceOp, YieldOp};
use crate::planning::{
    FacilityPlanningSolution, FacilityPlanningSolutionValidationError, PlanningProblem,
};
use crate::procedure::ir::MaterialInputOp;

#[derive(Debug, Error)]
pub enum AllocationApplicationError {
    #[error(transparent)]
    InvalidSolution(#[from] FacilityPlanningSolutionValidationError),
    #[error("allocated LAIR module has no entry block")]
    MissingModuleBlock,
    #[error("solution does not select method choice `{choice}`")]
    MissingChoice { choice: LocalId },
    #[error("solution selects method `{method}` outside choice `{choice}`")]
    MissingCandidate { choice: LocalId, method: String },
    #[error("selected method choice `{choice}` candidate has no entry block")]
    MissingCandidateBlock { choice: LocalId },
    #[error("selected method choice `{choice}` candidate has no method.yield")]
    MissingYield { choice: LocalId },
    #[error("solution does not bind Requirement `{requirement}`")]
    MissingRequirementBinding { requirement: LocalId },
    #[error("solution does not bind Procedure material input `{input}`")]
    MissingMaterialBinding { input: LocalId },
    #[error("selected method choice `{choice}` yields a value that was not cloned")]
    MissingYieldMapping { choice: LocalId },
}

pub(crate) fn apply_facility_solution(
    context: &mut Context,
    module: ModuleOp,
    problem: &PlanningProblem,
    solution: &FacilityPlanningSolution,
) -> Result<(), AllocationApplicationError> {
    solution.validate_against(problem)?;
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or(AllocationApplicationError::MissingModuleBlock)?;
    let choices = block
        .deref(context)
        .iter(context)
        .filter_map(|operation| Operation::get_op::<ChoiceOp>(operation, context))
        .collect::<Vec<_>>();
    let selections = solution
        .selections
        .iter()
        .map(|selection| (selection.choice.clone(), selection))
        .collect::<BTreeMap<_, _>>();
    let mut inserted_context = false;

    for choice in choices {
        let choice_id = choice.semantic_choice_id(context);
        let selection = selections.get(&choice_id).copied().ok_or_else(|| {
            AllocationApplicationError::MissingChoice {
                choice: choice_id.clone(),
            }
        })?;
        let candidate = choice
            .candidate_ids(context)
            .iter()
            .position(|method| method == &selection.method)
            .ok_or_else(|| AllocationApplicationError::MissingCandidate {
                choice: choice_id.clone(),
                method: selection.method.to_string(),
            })?;
        let candidate_block = choice
            .candidate_region(context, candidate)
            .deref(context)
            .get_head()
            .ok_or_else(|| AllocationApplicationError::MissingCandidateBlock {
                choice: choice_id.clone(),
            })?;
        let operations = candidate_block
            .deref(context)
            .iter(context)
            .collect::<Vec<_>>();
        let yield_op = operations
            .iter()
            .find_map(|operation| Operation::get_op::<YieldOp>(*operation, context))
            .ok_or_else(|| AllocationApplicationError::MissingYield {
                choice: choice_id.clone(),
            })?;
        let bindings = selection
            .tasks
            .iter()
            .flat_map(|task| {
                task.requirements
                    .iter()
                    .map(move |binding| (binding.requirement.clone(), (task.task.clone(), binding)))
            })
            .collect::<BTreeMap<_, _>>();
        let material_bindings = selection
            .tasks
            .iter()
            .flat_map(|task| {
                task.materials
                    .iter()
                    .map(move |binding| (binding.input.clone(), (task.task.clone(), binding)))
            })
            .collect::<BTreeMap<_, _>>();

        let mut rewriter = IRRewriter::<DummyListener>::default();
        rewriter.set_insertion_point(OpInsertionPoint::BeforeOperation(choice.get_operation()));
        if !inserted_context {
            let allocation_context = ContextOp::new(
                context,
                solution.problem_sha256.clone(),
                solution.inventory_sha256.clone(),
                solution.facility.clone(),
            );
            rewriter.append_operation(context, allocation_context.get_operation());
            inserted_context = true;
        }
        let method = MethodOp::new(context, selection);
        rewriter.append_operation(context, method.get_operation());
        let mut mapping = IrMapping::new();
        for operation in operations {
            if Operation::get_op::<YieldOp>(operation, context).is_some() {
                continue;
            }
            let requirement_id =
                Operation::get_op::<RequirementOp>(operation, context).map(|requirement| {
                    LocalId::new(requirement.requirement_id(context))
                        .expect("verified Requirement identity is stable")
                });
            let material_input_id =
                Operation::get_op::<MaterialInputOp>(operation, context).map(|material| {
                    LocalId::new(material.input_id(context))
                        .expect("verified material input identity is stable")
                });
            let cloned = clone_operation(operation, context, &mut rewriter, &mut mapping);
            rewriter.append_operation(context, cloned);
            if let Some(input) = material_input_id {
                let (procedure_node, binding) = material_bindings.get(&input).ok_or_else(|| {
                    AllocationApplicationError::MissingMaterialBinding {
                        input: input.clone(),
                    }
                })?;
                let binding = MaterialBindingOp::new(context, procedure_node, binding);
                rewriter.append_operation(context, binding.get_operation());
            }
            if let Some(requirement_id) = requirement_id {
                let (procedure_node, binding) = bindings.get(&requirement_id).ok_or_else(|| {
                    AllocationApplicationError::MissingRequirementBinding {
                        requirement: requirement_id.clone(),
                    }
                })?;
                let allocation = BindingOp::new(context, procedure_node, binding);
                rewriter.append_operation(context, allocation.get_operation());
                for parameter in &binding.parameters {
                    let parameter = ParameterMatchOp::new(context, &requirement_id, parameter);
                    rewriter.append_operation(context, parameter.get_operation());
                }
            }
        }
        let replacements = yield_op
            .get_operation()
            .deref(context)
            .operands()
            .map(|value| {
                mapping.lookup_value(value).or_else(|| {
                    choice
                        .get_operation()
                        .deref(context)
                        .operands()
                        .any(|operand| operand == value)
                        .then_some(value)
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| AllocationApplicationError::MissingYieldMapping {
                choice: choice_id.clone(),
            })?;
        rewriter.replace_operation_with_values(context, choice.get_operation(), replacements);
    }
    Ok(())
}
