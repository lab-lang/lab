//! Read-only reconstruction of the facility-bound semantic model from Allocated LAIR.
//!
//! This is the inverse of allocation application at the semantic boundary: every backend-facing
//! fact is recovered from verifier-valid `allocation.method` regions rather than from the planning
//! problem or solution that originally produced them.

use std::cell::Ref;
use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{MethodId, PropertyConstraint};
use pliron::builtin::attributes::StringAttr;
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::{Context, Ptr};
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::{Operation, verify_operation};
use pliron::pass::{Analysis, AnalysisManager};
use pliron::printable::Printable;
use pliron::r#type::Typed;
use pliron::value::{DefiningEntity, Value};
use thiserror::Error;

use super::ir::{
    BindingOp, ContextOp as AllocationContextOp, MaterialBindingOp, MethodOp, ParameterMatchOp,
    YieldOp,
};
use super::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, AllocatedRequirementBinding,
    InvocationAdapter,
};
use crate::capability::ir::{ConstraintOp, RequirementOp};
use crate::method::{IntentOperationId, LocalId, PortType};
use crate::planning::{
    PlanningMethodYield, PlanningPort, PlanningProcedureParameter, PlanningTaskInput,
    PlanningTaskOutput, PlanningValueSource, SelectedCapabilityParameter, SelectedMaterialBinding,
    SelectedMaterialSource,
};
use crate::procedure::ir::{MaterialInputOp, ParameterOp, TaskOp, semantic_port_type};
use crate::stage::{IrStage, detect_stage};

/// A failure to reconstruct the semantic allocation encoded by textual Allocated LAIR.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AllocatedProgramExtractionError {
    #[error("cannot extract a semantic allocation from LAIR that failed verification: {0}")]
    InvalidIr(String),
    #[error("cannot extract a semantic allocation from nonlinear material flow: {0}")]
    MaterialLinearity(String),
    #[error("cannot extract a semantic allocation from invalid LAIR: {0}")]
    InvalidStage(String),
    #[error("expected allocated-procedure LAIR, found {0}")]
    WrongStage(IrStage),
    #[error("allocated LAIR is missing the builtin.module entry block")]
    MissingModuleBlock,
    #[error("allocated LAIR is missing allocation.context")]
    MissingAllocationContext,
    #[error("allocated LAIR contains more than one allocation.context")]
    DuplicateAllocationContext,
    #[error("{owner} is missing required attribute `{attribute}`")]
    MissingAttribute {
        owner: String,
        attribute: &'static str,
    },
    #[error("{owner} has an invalid `{attribute}` attribute: {message}")]
    InvalidAttribute {
        owner: String,
        attribute: &'static str,
        message: String,
    },
    #[error("allocated Method `{choice}` is missing its entry block")]
    MissingMethodBlock { choice: LocalId },
    #[error("allocated Method `{choice}` has no allocation.yield terminator")]
    MissingYield { choice: LocalId },
    #[error("{owner} contains a value whose Procedure port type cannot be decoded")]
    UnsupportedPortType { owner: String },
    #[error("{owner} references a value that has no stable source identity")]
    UnresolvedValue { owner: String },
    #[error("{kind} `{identity}` references missing Procedure task `{task}`")]
    MissingTask {
        kind: &'static str,
        identity: String,
        task: LocalId,
    },
    #[error("allocated Method `{choice}` repeats Procedure task `{task}`")]
    DuplicateTask { choice: LocalId, task: LocalId },
    #[error("allocated Method `{choice}` repeats Procedure parameter `{parameter}`")]
    DuplicateParameter { choice: LocalId, parameter: LocalId },
    #[error("allocated Method `{choice}` repeats material input `{input}`")]
    DuplicateMaterial { choice: LocalId, input: LocalId },
    #[error("allocated Method `{choice}` repeats capability Requirement `{requirement}`")]
    DuplicateRequirement {
        choice: LocalId,
        requirement: LocalId,
    },
    #[error("material input `{input}` has no allocation.material binding")]
    MissingMaterialBinding { input: LocalId },
    #[error("Requirement `{requirement}` has no allocation.binding")]
    MissingRequirementBinding { requirement: LocalId },
    #[error("allocation.material for `{input}` does not match its semantic declaration")]
    MaterialBindingMismatch { input: LocalId },
    #[error("allocation.binding for `{requirement}` does not match its semantic declaration")]
    RequirementBindingMismatch { requirement: LocalId },
    #[error("Capability constraint for Requirement `{requirement}` is invalid: {message}")]
    InvalidConstraint {
        requirement: LocalId,
        message: String,
    },
    #[error(
        "allocation.parameter_match records do not exactly cover Requirement `{requirement}` constraints"
    )]
    ParameterMatchCoverage { requirement: LocalId },
    #[error("allocated Method `{choice}` contains unexpected operation `{operation}`")]
    UnexpectedMethodOperation { choice: LocalId, operation: String },
}

#[derive(Clone)]
struct AllocationContext {
    problem_sha256: String,
    inventory_sha256: String,
    facility: String,
}

#[derive(Clone)]
struct MaterialDeclaration {
    input: LocalId,
    node: LocalId,
    symbol: String,
}

#[derive(Clone)]
struct RequirementDeclaration {
    id: LocalId,
    node: LocalId,
    capability_kind: lab_capability::CapabilityKind,
    minimum_qualification: lab_capability::QualificationLevel,
    accepted_control_modes: BTreeSet<lab_capability::ControlMode>,
}

/// Reconstruct the complete semantic allocation carried by verifier-valid Allocated LAIR.
pub fn extract_allocated_program(
    context: &Context,
    module: ModuleOp,
) -> Result<AllocatedProgram, AllocatedProgramExtractionError> {
    verify_operation(module.get_operation(), context).map_err(|error| {
        AllocatedProgramExtractionError::InvalidIr(error.disp(context).to_string())
    })?;
    crate::procedure::analysis::MaterialLinearityAnalysis::compute(
        module.get_operation(),
        context,
        &mut AnalysisManager::default(),
    )
    .map_err(|error| {
        AllocatedProgramExtractionError::MaterialLinearity(error.disp(context).to_string())
    })?;
    let stage =
        detect_stage(context, module).map_err(AllocatedProgramExtractionError::InvalidStage)?;
    if stage != IrStage::AllocatedProcedure {
        return Err(AllocatedProgramExtractionError::WrongStage(stage));
    }
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or(AllocatedProgramExtractionError::MissingModuleBlock)?;

    let mut allocation_context = None;
    let mut methods = Vec::new();
    for operation in block.deref(context).iter(context) {
        if let Some(encoded) = Operation::get_op::<AllocationContextOp>(operation, context) {
            if allocation_context.is_some() {
                return Err(AllocatedProgramExtractionError::DuplicateAllocationContext);
            }
            allocation_context = Some(extract_context(context, &encoded)?);
        } else if let Some(method) = Operation::get_op::<MethodOp>(operation, context) {
            methods.push(method);
        }
    }
    let allocation_context =
        allocation_context.ok_or(AllocatedProgramExtractionError::MissingAllocationContext)?;

    let method_outputs = methods
        .iter()
        .flat_map(|method| {
            let choice = method.choice(context);
            method
                .get_operation()
                .deref(context)
                .results()
                .zip(method.output_names(context))
                .map(move |(value, output)| {
                    (
                        value,
                        PlanningValueSource::ChoiceOutput {
                            choice: choice.clone(),
                            output,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut allocated_methods = Vec::with_capacity(methods.len());
    for method in &methods {
        let allocated = extract_method(context, method, &method_outputs)?;
        allocated_methods.push(allocated);
    }

    Ok(AllocatedProgram {
        problem_sha256: allocation_context.problem_sha256,
        inventory_sha256: allocation_context.inventory_sha256,
        facility: allocation_context.facility,
        methods: allocated_methods,
    })
}

fn extract_context(
    context: &Context,
    encoded: &AllocationContextOp,
) -> Result<AllocationContext, AllocatedProgramExtractionError> {
    let owner = "allocation.context";
    Ok(AllocationContext {
        problem_sha256: required_string(
            encoded.get_attr_problem_sha256(context),
            owner,
            "problem_sha256",
        )?,
        inventory_sha256: required_string(
            encoded.get_attr_inventory_sha256(context),
            owner,
            "inventory_sha256",
        )?,
        facility: required_string(encoded.get_attr_facility(context), owner, "facility")?,
    })
}

fn extract_method(
    context: &Context,
    method: &MethodOp,
    method_outputs: &[(Value, PlanningValueSource)],
) -> Result<AllocatedMethod, AllocatedProgramExtractionError> {
    let choice = parse_local_id(
        required_string(
            method.get_attr_selected_choice(context),
            "allocation.method",
            "selected_choice",
        )?,
        "allocation.method",
        "selected_choice",
    )?;
    let owner = format!("allocation.method `{choice}`");
    let source_operation = IntentOperationId::new(required_string(
        method.get_attr_selected_source_operation(context),
        &owner,
        "selected_source_operation",
    )?)
    .map_err(|error| AllocatedProgramExtractionError::InvalidAttribute {
        owner: owner.clone(),
        attribute: "selected_source_operation",
        message: error.to_string(),
    })?;
    let method_id = MethodId::new(required_string(
        method.get_attr_selected_method(context),
        &owner,
        "selected_method",
    )?)
    .map_err(|error| AllocatedProgramExtractionError::InvalidAttribute {
        owner: owner.clone(),
        attribute: "selected_method",
        message: error.to_string(),
    })?;
    let operation = method.get_operation().deref(context);
    let input_names = method.input_names(context);
    let output_names = method.output_names(context);
    let inputs = input_names
        .iter()
        .cloned()
        .zip(operation.operands())
        .map(|(name, value)| {
            let source = source_for_value(method_outputs, value);
            if source.is_none() && !is_design_value(context, value) {
                return Err(AllocatedProgramExtractionError::UnresolvedValue {
                    owner: format!("{owner} input `{name}`"),
                });
            }
            Ok(PlanningPort {
                name,
                port_type: decode_port_type(context, value, format!("{owner} input"))?,
                source,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = output_names
        .iter()
        .cloned()
        .zip(operation.results())
        .map(|(name, value)| {
            Ok(PlanningPort {
                name,
                port_type: decode_port_type(context, value, format!("{owner} output"))?,
                source: None,
            })
        })
        .collect::<Result<Vec<_>, AllocatedProgramExtractionError>>()?;

    let block = method
        .body(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| AllocatedProgramExtractionError::MissingMethodBlock {
            choice: choice.clone(),
        })?;
    let mut task_operations = Vec::<Ptr<Operation>>::new();
    let mut task_ids = BTreeSet::new();
    let mut parameters = BTreeMap::<LocalId, Vec<PlanningProcedureParameter>>::new();
    let mut parameter_ids = BTreeSet::new();
    let mut materials = BTreeMap::<LocalId, Vec<MaterialDeclaration>>::new();
    let mut material_ids = BTreeSet::new();
    let mut material_bindings = BTreeMap::<LocalId, MaterialBindingOp>::new();
    let mut requirements = BTreeMap::<LocalId, Vec<RequirementDeclaration>>::new();
    let mut requirement_ids = BTreeSet::new();
    let mut constraints = BTreeMap::<LocalId, Vec<PropertyConstraint>>::new();
    let mut bindings = BTreeMap::<LocalId, BindingOp>::new();
    let mut parameter_matches = BTreeMap::<LocalId, Vec<SelectedCapabilityParameter>>::new();
    let mut yield_op = None;

    for nested in block.deref(context).iter(context) {
        if let Some(task) = Operation::get_op::<TaskOp>(nested, context) {
            let task_id = task.semantic_node_id(context);
            if !task_ids.insert(task_id.clone()) {
                return Err(AllocatedProgramExtractionError::DuplicateTask {
                    choice: choice.clone(),
                    task: task_id,
                });
            }
            task_operations.push(nested);
            continue;
        }
        if let Some(parameter) = Operation::get_op::<ParameterOp>(nested, context) {
            let node = parse_local_id(
                parameter.procedure_node(context),
                "procedure.parameter",
                "procedure_parameter_node",
            )?;
            let (id, property_kind, value) = parameter.semantic_parameter(context);
            if !parameter_ids.insert(id.clone()) {
                return Err(AllocatedProgramExtractionError::DuplicateParameter {
                    choice: choice.clone(),
                    parameter: id,
                });
            }
            parameters
                .entry(node)
                .or_default()
                .push(PlanningProcedureParameter {
                    id,
                    property_kind,
                    value,
                });
            continue;
        }
        if let Some(material) = Operation::get_op::<MaterialInputOp>(nested, context) {
            let input = parse_local_id(
                material.input_id(context),
                "procedure.material_input",
                "material_input_id",
            )?;
            if !material_ids.insert(input.clone()) {
                return Err(AllocatedProgramExtractionError::DuplicateMaterial {
                    choice: choice.clone(),
                    input,
                });
            }
            let node = parse_local_id(
                material.procedure_node(context),
                "procedure.material_input",
                "material_input_node",
            )?;
            materials
                .entry(node.clone())
                .or_default()
                .push(MaterialDeclaration {
                    input,
                    node,
                    symbol: material.symbol(context),
                });
            continue;
        }
        if let Some(binding) = Operation::get_op::<MaterialBindingOp>(nested, context) {
            let input = binding.input(context);
            if material_bindings.insert(input.clone(), binding).is_some() {
                return Err(AllocatedProgramExtractionError::DuplicateMaterial {
                    choice: choice.clone(),
                    input,
                });
            }
            continue;
        }
        if let Some(requirement) = Operation::get_op::<RequirementOp>(nested, context) {
            let id = parse_local_id(
                requirement.requirement_id(context),
                "capability.requirement",
                "requirement_id",
            )?;
            if !requirement_ids.insert(id.clone()) {
                return Err(AllocatedProgramExtractionError::DuplicateRequirement {
                    choice: choice.clone(),
                    requirement: id,
                });
            }
            let node = parse_local_id(
                requirement.procedure_node(context),
                "capability.requirement",
                "procedure_node",
            )?;
            requirements
                .entry(node.clone())
                .or_default()
                .push(RequirementDeclaration {
                    id,
                    node,
                    capability_kind: requirement.semantic_capability_kind(context),
                    minimum_qualification: requirement.semantic_minimum_qualification(context),
                    accepted_control_modes: requirement.semantic_control_modes(context),
                });
            continue;
        }
        if let Some(constraint) = Operation::get_op::<ConstraintOp>(nested, context) {
            let requirement = parse_local_id(
                constraint.requirement_id(context),
                "capability.constraint",
                "constraint_requirement_id",
            )?;
            let decoded = constraint.decode(context).map_err(|message| {
                AllocatedProgramExtractionError::InvalidConstraint {
                    requirement: requirement.clone(),
                    message,
                }
            })?;
            constraints.entry(requirement).or_default().push(decoded);
            continue;
        }
        if let Some(binding) = Operation::get_op::<BindingOp>(nested, context) {
            let requirement = binding.requirement(context);
            if bindings.insert(requirement.clone(), binding).is_some() {
                return Err(AllocatedProgramExtractionError::DuplicateRequirement {
                    choice: choice.clone(),
                    requirement,
                });
            }
            continue;
        }
        if let Some(parameter_match) = Operation::get_op::<ParameterMatchOp>(nested, context) {
            parameter_matches
                .entry(parameter_match.requirement(context))
                .or_default()
                .push(parameter_match.selected_parameter(context));
            continue;
        }
        if let Some(found_yield) = Operation::get_op::<YieldOp>(nested, context) {
            yield_op = Some(found_yield);
            continue;
        }
        return Err(AllocatedProgramExtractionError::UnexpectedMethodOperation {
            choice: choice.clone(),
            operation: Operation::get_opid(nested, context).to_string(),
        });
    }

    verify_metadata_owners(&task_ids, &parameters, "Procedure parameter")?;
    verify_metadata_owners(&task_ids, &materials, "material input")?;
    verify_metadata_owners(&task_ids, &requirements, "Capability Requirement")?;

    let mut local_values = block
        .deref(context)
        .arguments()
        .zip(input_names)
        .map(|(value, input)| (value, PlanningValueSource::ChoiceInput { input }))
        .collect::<Vec<_>>();
    let mut tasks = Vec::with_capacity(task_operations.len());
    for task_operation in task_operations {
        let task = Operation::get_op::<TaskOp>(task_operation, context)
            .expect("the collected task operation was identified as procedure.task");
        let task_id = task.semantic_node_id(context);
        let raw = task_operation.deref(context);
        let inputs = raw
            .operands()
            .map(|value| {
                let source = source_for_value(&local_values, value).ok_or_else(|| {
                    AllocatedProgramExtractionError::UnresolvedValue {
                        owner: format!("Procedure task `{task_id}` input"),
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
            .collect::<Result<Vec<_>, AllocatedProgramExtractionError>>()?;
        let outputs = raw
            .results()
            .zip(task.output_names(context))
            .map(|(value, name)| {
                local_values.push((
                    value,
                    PlanningValueSource::TaskOutput {
                        task: task_id.clone(),
                        output: name.clone(),
                    },
                ));
                Ok(PlanningTaskOutput {
                    name,
                    port_type: decode_port_type(
                        context,
                        value,
                        format!("Procedure task `{task_id}` output"),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, AllocatedProgramExtractionError>>()?;
        let selected_materials = materials
            .remove(&task_id)
            .unwrap_or_default()
            .into_iter()
            .map(|declaration| {
                let binding = material_bindings
                    .remove(&declaration.input)
                    .ok_or_else(|| AllocatedProgramExtractionError::MissingMaterialBinding {
                        input: declaration.input.clone(),
                    })?;
                extract_material(context, declaration, &binding)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let selected_requirements = requirements
            .remove(&task_id)
            .unwrap_or_default()
            .into_iter()
            .map(|declaration| {
                let binding = bindings.remove(&declaration.id).ok_or_else(|| {
                    AllocatedProgramExtractionError::MissingRequirementBinding {
                        requirement: declaration.id.clone(),
                    }
                })?;
                let matches = parameter_matches
                    .remove(&declaration.id)
                    .unwrap_or_default();
                let expected = constraints.remove(&declaration.id).unwrap_or_default();
                if !parameters_cover_constraints(&matches, &expected) {
                    return Err(AllocatedProgramExtractionError::ParameterMatchCoverage {
                        requirement: declaration.id.clone(),
                    });
                }
                extract_requirement(context, declaration, &binding, matches)
            })
            .collect::<Result<Vec<_>, _>>()?;
        tasks.push(AllocatedProcedureTask {
            id: task_id.clone(),
            operation: task.semantic_operation(context),
            program: task.semantic_program(context),
            inputs,
            outputs,
            parameters: parameters.remove(&task_id).unwrap_or_default(),
            materials: selected_materials,
            requirements: selected_requirements,
        });
    }

    let yield_op = yield_op.ok_or_else(|| AllocatedProgramExtractionError::MissingYield {
        choice: choice.clone(),
    })?;
    let yields = output_names
        .into_iter()
        .zip(yield_op.get_operation().deref(context).operands())
        .map(|(output, value)| {
            let source = source_for_value(&local_values, value).ok_or_else(|| {
                AllocatedProgramExtractionError::UnresolvedValue {
                    owner: format!("allocation.method `{choice}` yield `{output}`"),
                }
            })?;
            Ok(PlanningMethodYield { output, source })
        })
        .collect::<Result<Vec<_>, AllocatedProgramExtractionError>>()?;

    Ok(AllocatedMethod {
        choice,
        source_operation,
        method: method_id,
        after: method.after(context),
        inputs,
        outputs,
        yields,
        tasks,
    })
}

fn extract_material(
    context: &Context,
    declaration: MaterialDeclaration,
    binding: &MaterialBindingOp,
) -> Result<SelectedMaterialBinding, AllocatedProgramExtractionError> {
    if binding.procedure_node(context) != declaration.node
        || binding.symbol(context) != declaration.symbol
    {
        return Err(AllocatedProgramExtractionError::MaterialBindingMismatch {
            input: declaration.input,
        });
    }
    let owner = format!("allocation.material `{}`", declaration.input);
    let source_kind = required_string(
        binding.get_attr_material_source_kind(context),
        &owner,
        "material_source_kind",
    )?;
    let source = match source_kind.as_str() {
        "material_lot" => SelectedMaterialSource::MaterialLot {
            component: required_string(
                binding.get_attr_bound_component(context),
                &owner,
                "bound_component",
            )?,
            material_lot: required_string(
                binding.get_attr_bound_material_lot(context),
                &owner,
                "bound_material_lot",
            )?,
        },
        "choice_output" => SelectedMaterialSource::ChoiceOutput {
            choice: parse_local_id(
                required_string(
                    binding.get_attr_bound_choice(context),
                    &owner,
                    "bound_choice",
                )?,
                &owner,
                "bound_choice",
            )?,
        },
        other => {
            return Err(AllocatedProgramExtractionError::InvalidAttribute {
                owner,
                attribute: "material_source_kind",
                message: format!("unknown material source kind `{other}`"),
            });
        }
    };
    Ok(SelectedMaterialBinding {
        input: declaration.input,
        symbol: declaration.symbol,
        source,
        interchangeable_alternatives: binding.interchangeable_alternatives(context),
    })
}

fn extract_requirement(
    context: &Context,
    declaration: RequirementDeclaration,
    binding: &BindingOp,
    parameters: Vec<SelectedCapabilityParameter>,
) -> Result<AllocatedRequirementBinding, AllocatedProgramExtractionError> {
    if binding.procedure_node(context) != declaration.node {
        return Err(
            AllocatedProgramExtractionError::RequirementBindingMismatch {
                requirement: declaration.id,
            },
        );
    }
    let owner = format!("allocation.binding `{}`", declaration.id);
    let selected_adapter = binding.selected_adapter(context);
    let procedure_implementation = selected_adapter
        .as_ref()
        .and_then(|adapter| adapter.procedure_implementation.clone());
    let adapter = selected_adapter.map(|adapter| InvocationAdapter {
        driver: adapter.driver,
        profile_path: adapter.profile_path,
        profile_sha256: adapter.profile_sha256,
        features: adapter.features,
        accepted_run_formats: adapter.accepted_run_formats,
        emitted_run_formats: adapter.emitted_run_formats,
    });
    Ok(AllocatedRequirementBinding {
        id: declaration.id,
        capability_kind: declaration.capability_kind,
        minimum_qualification: declaration.minimum_qualification,
        accepted_control_modes: declaration.accepted_control_modes,
        offering: required_string(
            binding.get_attr_bound_offering(context),
            &owner,
            "bound_offering",
        )?,
        asset: required_string(binding.get_attr_bound_asset(context), &owner, "bound_asset")?,
        observed_qualification: required_string(
            binding.get_attr_observed_qualification(context),
            &owner,
            "observed_qualification",
        )?,
        control_mode: required_string(
            binding.get_attr_observed_control_mode(context),
            &owner,
            "observed_control_mode",
        )?,
        parameters,
        procedure_implementation,
        adapter,
    })
}

fn verify_metadata_owners<T>(
    tasks: &BTreeSet<LocalId>,
    metadata: &BTreeMap<LocalId, Vec<T>>,
    kind: &'static str,
) -> Result<(), AllocatedProgramExtractionError> {
    if let Some(task) = metadata.keys().find(|task| !tasks.contains(*task)) {
        return Err(AllocatedProgramExtractionError::MissingTask {
            kind,
            identity: task.to_string(),
            task: task.clone(),
        });
    }
    Ok(())
}

fn parameters_cover_constraints(
    parameters: &[SelectedCapabilityParameter],
    constraints: &[PropertyConstraint],
) -> bool {
    if parameters.len() != constraints.len()
        || parameters
            .iter()
            .map(|parameter| parameter.offering_parameter.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            != parameters.len()
    {
        return false;
    }
    let mut matched = vec![false; constraints.len()];
    parameters.iter().all(|parameter| {
        let Some(index) = constraints
            .iter()
            .enumerate()
            .find_map(|(index, constraint)| {
                (!matched[index]
                    && parameter.property_kind == constraint.property_kind
                    && parameter.relation == constraint.relation
                    && parameter.required == constraint.required)
                    .then_some(index)
            })
        else {
            return false;
        };
        matched[index] = true;
        true
    })
}

fn required_string(
    value: Option<Ref<'_, StringAttr>>,
    owner: impl Into<String>,
    attribute: &'static str,
) -> Result<String, AllocatedProgramExtractionError> {
    value.map(|value| value.as_str().to_owned()).ok_or_else(|| {
        AllocatedProgramExtractionError::MissingAttribute {
            owner: owner.into(),
            attribute,
        }
    })
}

fn parse_local_id(
    value: String,
    owner: impl Into<String>,
    attribute: &'static str,
) -> Result<LocalId, AllocatedProgramExtractionError> {
    LocalId::new(value).map_err(|error| AllocatedProgramExtractionError::InvalidAttribute {
        owner: owner.into(),
        attribute,
        message: error.to_string(),
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

fn is_design_value(context: &Context, value: Value) -> bool {
    matches!(
        value.defining_entity(),
        DefiningEntity::Op(operation)
            if Operation::get_opid(operation, context).dialect.as_ref() == "design"
    )
}

fn decode_port_type(
    context: &Context,
    value: Value,
    owner: String,
) -> Result<PortType, AllocatedProgramExtractionError> {
    semantic_port_type(context, value.get_type(context))
        .ok_or(AllocatedProgramExtractionError::UnsupportedPortType { owner })
}

#[cfg(test)]
mod tests {
    use lab_capability::{
        AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, ExactInteger, MethodId,
        OperationId, PropertyConstraint, PropertyKind, PropertyValue, QualificationLevel,
        ScalarValue,
    };
    use pliron::builtin::attributes::StringAttr;
    use pliron::builtin::op_interfaces::SingleBlockRegionInterface;
    use pliron::combine::{Parser, eof};
    use pliron::identifier::Identifier;
    use pliron::irfmt::parsers::spaced;
    use pliron::operation::verify_operation;
    use pliron::parsable::parse_from_str;
    use pliron::printable::Printable;

    use super::*;
    use crate::allocation::ir::{
        BindingOp, ContextOp as AllocationContextOp, MaterialBindingOp, MethodOp, ParameterMatchOp,
        YieldOp,
    };
    use crate::capability::ir::{ConstraintOp, RequirementOp};
    use crate::design::ir::DesignDnaSequenceOp;
    use crate::method::ProcedureValue;
    use crate::planning::{
        PlanningMethodCandidate, PlanningMethodChoice, PlanningPort, SelectedAdapter,
        SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
        SelectedRequirementBinding,
    };
    use crate::procedure::ir::{MaterialInputOp, MaterialType, ParameterOp, TaskOp};
    use crate::stage::{IrStage, initialize_stage};

    #[test]
    fn allocated_semantics_survive_print_parse_without_planning_sidecars() {
        let (context, module) = simple_allocated_module();
        verify_operation(module.get_operation(), &context).unwrap();
        let expected = extract_allocated_program(&context, module).unwrap();
        let text = module.get_operation().disp(&context).to_string();

        let mut reparsed_context = Context::new();
        let root = parse_from_str(
            spaced(Operation::top_level_parser()).skip(eof()),
            &mut reparsed_context,
            &text,
        )
        .unwrap();
        let reparsed_module = Operation::get_op::<ModuleOp>(root, &reparsed_context).unwrap();
        verify_operation(reparsed_module.get_operation(), &reparsed_context).unwrap();
        let actual = extract_allocated_program(&reparsed_context, reparsed_module).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(actual.problem_sha256, "a".repeat(64));
        assert_eq!(actual.inventory_sha256, "b".repeat(64));
        assert_eq!(actual.facility, "https://example.org/facility");
        assert_eq!(actual.methods.len(), 1);
        assert_eq!(actual.methods[0].tasks.len(), 1);
        let task = &actual.methods[0].tasks[0];
        assert_eq!(task.parameters.len(), 1);
        assert_eq!(task.materials.len(), 1);
        assert_eq!(task.requirements.len(), 1);
        assert_eq!(
            task.materials[0].interchangeable_alternatives,
            ["https://example.org/lot/alternate".to_owned()]
        );
        let adapter = task.requirements[0].adapter.as_ref().unwrap();
        assert_eq!(adapter.driver, "example.driver");
        assert_eq!(
            adapter.features,
            ["temperature-control".to_owned()].into_iter().collect()
        );
        assert_eq!(task.requirements[0].parameters.len(), 1);
    }

    #[test]
    fn allocated_lair_revalidates_registered_task_program_provenance() {
        let (context, module) = simple_allocated_module();
        let module_block = module
            .get_region(&context)
            .deref(&context)
            .get_head()
            .unwrap();
        let method = module_block
            .deref(&context)
            .iter(&context)
            .find_map(|operation| Operation::get_op::<MethodOp>(operation, &context))
            .unwrap();
        let task = method
            .entry_block(&context)
            .deref(&context)
            .iter(&context)
            .find_map(|operation| Operation::get_op::<TaskOp>(operation, &context))
            .unwrap();
        task.set_attr_operation(
            &context,
            StringAttr::new(crate::procedure::vocabulary::PLATE_DILUTED_CULTURE.to_owned()),
        );
        verify_operation(module.get_operation(), &context).unwrap();

        let error = extract_allocated_program(&context, module).unwrap_err();
        assert!(matches!(
            error,
            AllocatedProgramExtractionError::InvalidStage(message)
                if message.contains("cannot be normalized")
        ));
    }

    #[test]
    fn method_value_sources_do_not_depend_on_module_order() {
        let mut context = Context::new();
        let module = ModuleOp::new(
            &mut context,
            Identifier::try_from("forward_method_reference").unwrap(),
        );
        initialize_stage(&mut context, module, IrStage::AllocatedProcedure);
        let design = DesignDnaSequenceOp::new(&mut context, "template", "ACGT");
        module.append_operation(&mut context, design.get_operation(), 0);
        let allocation_context = AllocationContextOp::new(
            &mut context,
            "a".repeat(64),
            "b".repeat(64),
            "https://example.org/facility",
        );
        module.append_operation(&mut context, allocation_context.get_operation(), 0);

        let state = AbsoluteIri::new("https://example.org/material/sample").unwrap();
        let material_type = MaterialType::get(
            &context,
            StringAttr::new("https://example.org/material/sample".to_owned()),
        )
        .into();
        let producer_id = LocalId::new("producer").unwrap();
        let producer_candidate = PlanningMethodCandidate {
            method: MethodId::new("https://example.org/method/produce").unwrap(),
            tasks: Vec::new(),
            yields: Vec::new(),
        };
        let producer_choice = PlanningMethodChoice {
            id: producer_id.clone(),
            source_operation: IntentOperationId::new("example.produce").unwrap(),
            after: Vec::new(),
            inputs: Vec::new(),
            outputs: vec![PlanningPort {
                name: LocalId::new("sample").unwrap(),
                port_type: PortType::Material {
                    state: state.clone(),
                },
                source: None,
            }],
            candidates: vec![producer_candidate.clone()],
        };
        let producer = MethodOp::new(
            &mut context,
            &producer_choice,
            &producer_candidate,
            Vec::new(),
            vec![material_type],
        );
        let producer_task_id = LocalId::new("producer::task").unwrap();
        let producer_task = TaskOp::new(
            &mut context,
            producer_task_id.to_string(),
            &OperationId::new("https://example.org/operation/produce").unwrap(),
            Vec::new(),
            vec![material_type],
            &[LocalId::new("sample").unwrap()],
        );
        let produced_value = producer_task.get_operation().deref(&context).get_result(0);
        producer.append_body_operation(&mut context, producer_task.get_operation());
        append_manual_requirement(
            &mut context,
            &producer,
            &producer_task_id,
            "producer::requirement",
        );
        let producer_yield = YieldOp::new(&mut context, vec![produced_value]);
        producer.append_body_operation(&mut context, producer_yield.get_operation());
        let producer_result = producer.get_operation().deref(&context).get_result(0);

        let consumer_candidate = PlanningMethodCandidate {
            method: MethodId::new("https://example.org/method/consume").unwrap(),
            tasks: Vec::new(),
            yields: Vec::new(),
        };
        let consumer_choice = PlanningMethodChoice {
            id: LocalId::new("consumer").unwrap(),
            source_operation: IntentOperationId::new("example.consume").unwrap(),
            after: Vec::new(),
            inputs: vec![PlanningPort {
                name: LocalId::new("sample").unwrap(),
                port_type: PortType::Material { state },
                source: Some(PlanningValueSource::ChoiceOutput {
                    choice: producer_id.clone(),
                    output: LocalId::new("sample").unwrap(),
                }),
            }],
            outputs: Vec::new(),
            candidates: vec![consumer_candidate.clone()],
        };
        let consumer = MethodOp::new(
            &mut context,
            &consumer_choice,
            &consumer_candidate,
            vec![producer_result],
            Vec::new(),
        );
        let consumer_input = consumer
            .entry_block(&context)
            .deref(&context)
            .get_argument(0);
        let consumer_task_id = LocalId::new("consumer::task").unwrap();
        let consumer_task = TaskOp::new(
            &mut context,
            consumer_task_id.to_string(),
            &OperationId::new("https://example.org/operation/consume").unwrap(),
            vec![consumer_input],
            Vec::new(),
            &[],
        );
        consumer.append_body_operation(&mut context, consumer_task.get_operation());
        append_manual_requirement(
            &mut context,
            &consumer,
            &consumer_task_id,
            "consumer::requirement",
        );
        let consumer_yield = YieldOp::new(&mut context, Vec::new());
        consumer.append_body_operation(&mut context, consumer_yield.get_operation());

        // Deliberately place the consumer first. Allocated LAIR is an acyclic graph; textual order
        // is not its dependency model.
        module.append_operation(&mut context, consumer.get_operation(), 0);
        module.append_operation(&mut context, producer.get_operation(), 0);
        verify_operation(module.get_operation(), &context).unwrap();

        let allocated = extract_allocated_program(&context, module).unwrap();
        let consumer = allocated
            .methods
            .iter()
            .find(|method| method.choice.as_str() == "consumer")
            .unwrap();
        assert_eq!(
            consumer.inputs[0].source,
            Some(PlanningValueSource::ChoiceOutput {
                choice: producer_id,
                output: LocalId::new("sample").unwrap(),
            })
        );
    }

    fn simple_allocated_module() -> (Context, ModuleOp) {
        let mut context = Context::new();
        let module = ModuleOp::new(
            &mut context,
            Identifier::try_from("allocated_extraction").unwrap(),
        );
        initialize_stage(&mut context, module, IrStage::AllocatedProcedure);
        let design = DesignDnaSequenceOp::new(&mut context, "template", "ACGT");
        module.append_operation(&mut context, design.get_operation(), 0);
        let allocation_context = AllocationContextOp::new(
            &mut context,
            "a".repeat(64),
            "b".repeat(64),
            "https://example.org/facility",
        );
        module.append_operation(&mut context, allocation_context.get_operation(), 0);

        let choice_id = LocalId::new("choice").unwrap();
        let task_id = LocalId::new("choice::task").unwrap();
        let requirement_id = LocalId::new("choice::task::requirement").unwrap();
        let method_id = MethodId::new("https://example.org/method").unwrap();
        let candidate = PlanningMethodCandidate {
            method: method_id.clone(),
            tasks: Vec::new(),
            yields: Vec::new(),
        };
        let choice = PlanningMethodChoice {
            id: choice_id,
            source_operation: IntentOperationId::new("example.operation").unwrap(),
            after: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            candidates: vec![candidate.clone()],
        };
        let method = MethodOp::new(&mut context, &choice, &candidate, Vec::new(), Vec::new());
        module.append_operation(&mut context, method.get_operation(), 0);

        let task = TaskOp::new(
            &mut context,
            task_id.to_string(),
            &OperationId::new("https://example.org/operation").unwrap(),
            Vec::new(),
            Vec::new(),
            &[],
        );
        method.append_body_operation(&mut context, task.get_operation());

        let property_kind = PropertyKind::new("https://example.org/property/count").unwrap();
        let required =
            PropertyValue::unitless(ScalarValue::Integer(ExactInteger::parse("1").unwrap()));
        let parameter = ParameterOp::new(
            &mut context,
            "choice::task::parameter",
            task_id.to_string(),
            &property_kind,
            &ProcedureValue::Scalar {
                value: required.clone(),
            },
        );
        method.append_body_operation(&mut context, parameter.get_operation());

        let material_id = LocalId::new("choice::task::material").unwrap();
        let material = MaterialInputOp::new(
            &mut context,
            material_id.to_string(),
            task_id.to_string(),
            "sample",
        );
        method.append_body_operation(&mut context, material.get_operation());
        let selected_material = SelectedMaterialBinding {
            input: material_id,
            symbol: "sample".to_owned(),
            source: SelectedMaterialSource::MaterialLot {
                component: "https://example.org/component/sample".to_owned(),
                material_lot: "https://example.org/lot/selected".to_owned(),
            },
            interchangeable_alternatives: vec!["https://example.org/lot/alternate".to_owned()],
        };
        let material_binding = MaterialBindingOp::new(&mut context, &task_id, &selected_material);
        method.append_body_operation(&mut context, material_binding.get_operation());

        let capability_kind = CapabilityKind::new("https://example.org/capability").unwrap();
        let requirement = RequirementOp::new(
            &mut context,
            requirement_id.to_string(),
            task_id.to_string(),
            &capability_kind,
            QualificationLevel::Executable,
            [ControlMode::Manual],
        );
        method.append_body_operation(&mut context, requirement.get_operation());
        let constraint = PropertyConstraint {
            property_kind: property_kind.clone(),
            relation: ConstraintRelation::Exact,
            required: required.clone(),
        };
        let selected_parameter = SelectedCapabilityParameter {
            property_kind,
            relation: ConstraintRelation::Exact,
            required,
            offering_parameter: "https://example.org/offering/count".to_owned(),
            observed: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse("1").unwrap(),
            )),
        };
        let selected = SelectedRequirementBinding {
            requirement: requirement_id.clone(),
            capability_kind,
            minimum_qualification: QualificationLevel::Executable,
            accepted_control_modes: [ControlMode::Manual].into_iter().collect(),
            offering: "https://example.org/offering".to_owned(),
            asset: "https://example.org/asset".to_owned(),
            observed_qualification: QualificationLevel::Executable.to_string(),
            control_mode: ControlMode::Manual.to_string(),
            parameters: vec![selected_parameter.clone()],
            adapter: Some(SelectedAdapter {
                driver: "example.driver".to_owned(),
                procedure_implementation: None,
                profile_path: "adapters/example.toml".into(),
                profile_sha256: "c".repeat(64),
                features: ["temperature-control".to_owned()].into_iter().collect(),
                accepted_run_formats: ["application/json".to_owned()].into_iter().collect(),
                emitted_run_formats: ["text/plain".to_owned()].into_iter().collect(),
            }),
            rejected_candidates: Vec::new(),
        };
        let binding = BindingOp::new(&mut context, &task_id, &selected);
        method.append_body_operation(&mut context, binding.get_operation());
        let parameter_match =
            ParameterMatchOp::new(&mut context, &requirement_id, &selected_parameter);
        method.append_body_operation(&mut context, parameter_match.get_operation());
        let constraint = ConstraintOp::new(&mut context, requirement_id.to_string(), &constraint);
        method.append_body_operation(&mut context, constraint.get_operation());
        let terminator = YieldOp::new(&mut context, Vec::new());
        method.append_body_operation(&mut context, terminator.get_operation());

        (context, module)
    }

    fn append_manual_requirement(
        context: &mut Context,
        method: &MethodOp,
        task: &LocalId,
        requirement: &str,
    ) {
        let requirement_id = LocalId::new(requirement).unwrap();
        let capability_kind = CapabilityKind::new("https://example.org/capability").unwrap();
        let declaration = RequirementOp::new(
            context,
            requirement,
            task.to_string(),
            &capability_kind,
            QualificationLevel::Executable,
            [ControlMode::Manual],
        );
        method.append_body_operation(context, declaration.get_operation());
        let selected = SelectedRequirementBinding {
            requirement: requirement_id,
            capability_kind,
            minimum_qualification: QualificationLevel::Executable,
            accepted_control_modes: [ControlMode::Manual].into_iter().collect(),
            offering: format!("https://example.org/offering/{requirement}"),
            asset: format!("https://example.org/asset/{requirement}"),
            observed_qualification: QualificationLevel::Executable.to_string(),
            control_mode: ControlMode::Manual.to_string(),
            parameters: Vec::new(),
            adapter: None,
            rejected_candidates: Vec::new(),
        };
        let binding = BindingOp::new(context, task, &selected);
        method.append_body_operation(context, binding.get_operation());
    }
}
