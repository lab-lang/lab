//! Facility-independent projection of validated method definitions into LAIR candidate regions.

use std::collections::BTreeMap;

use crate::method::{
    IntentOperationId, LocalId, MaterialSourceExpression, MethodDefinition, MethodRegistry,
    PortType, ProcedureTaskExecutionDefinition, ProcedureValue, ProcedureValueExpression,
    ScalarValueExpression, ValueReference,
};
use lab_capability::{
    CapabilityKind, ControlMode, PropertyConstraint, PropertyValue, QualificationLevel, ScalarValue,
};
use pliron::builtin::attributes::StringAttr;
use pliron::context::{Context, Ptr};
use pliron::input_err;
use pliron::irbuild::dialect_conversion::{
    DialectConversion, DialectConversionRewriter, OperandsInfo, apply_dialect_conversion,
};
use pliron::irbuild::inserter::Inserter;
use pliron::irbuild::rewriter::Rewriter;
use pliron::linked_list::ContainsLinkedList;
use pliron::location::Located;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::{TypeHandle, Typed};
use pliron::value::Value;

use crate::capability::ir::{ConstraintOp, RequirementOp};
use crate::design::ir::DesignType;
use crate::method::ir::{ChoiceOp, ChoicePorts, YieldOp};
use crate::procedure::ir::{
    DataType as ProcedureDataType, MaterialInputOp, MaterialType as ProcedureMaterialType,
    ParameterOp, TaskOp,
};
use crate::procedure::{
    ProcedureCompiler, ProcedureProgramBuildContext, ResolvedProcedureMaterial,
    ResolvedProcedureParameter,
};
use crate::workflow::ir::{
    DataType as WorkflowDataType, MaterialType as WorkflowMaterialType, PerformOp,
};

pub(crate) fn refine_method_alternatives(
    context: &mut Context,
    root: Ptr<Operation>,
    registry: &MethodRegistry,
    procedures: &ProcedureCompiler,
) -> Result<()> {
    apply_dialect_conversion(
        context,
        &mut MethodRefinement::new(registry, procedures),
        root,
    )?;
    Ok(())
}

struct MethodRefinement<'a> {
    registry: &'a MethodRegistry,
    procedures: &'a ProcedureCompiler,
    next_choice: BTreeMap<IntentOperationId, usize>,
}

impl<'a> MethodRefinement<'a> {
    fn new(registry: &'a MethodRegistry, procedures: &'a ProcedureCompiler) -> Self {
        Self {
            registry,
            procedures,
            next_choice: BTreeMap::new(),
        }
    }
}

impl DialectConversion for MethodRefinement<'_> {
    fn can_convert_op(&self, context: &Context, operation: Ptr<Operation>) -> bool {
        intent_operation(context, operation).is_some()
    }

    fn can_convert_type(&self, context: &Context, ty: TypeHandle) -> bool {
        let ty = ty.deref(context);
        ty.downcast_ref::<WorkflowMaterialType>().is_some()
            || ty.downcast_ref::<WorkflowDataType>().is_some()
    }

    fn convert_type(&mut self, context: &mut Context, ty: TypeHandle) -> Result<TypeHandle> {
        Ok(converted_type(context, ty))
    }

    fn rewrite(
        &mut self,
        context: &mut Context,
        rewriter: &mut DialectConversionRewriter,
        operation: Ptr<Operation>,
        _operands_info: &OperandsInfo,
    ) -> Result<()> {
        let instance = intent_instance(context, operation)?;
        let declared_candidates = self.registry.methods_for(&instance.operation);
        if declared_candidates.is_empty() {
            return input_err!(
                operation.deref(context).loc(),
                "no method definition refines Intent operation '{}'",
                instance.operation
            );
        }
        let signature = declared_candidates[0]
            .validate()
            .expect("MethodRegistry contains only validated definitions");
        let operands = operation.deref(context).operands().collect::<Vec<_>>();
        verify_inputs(
            context,
            operation,
            &instance.operation,
            &signature.inputs,
            &instance.input_names,
            &operands,
        )?;
        verify_results(
            context,
            operation,
            &signature.outputs,
            &instance.output_names,
        )?;
        let candidates = declared_candidates
            .iter()
            .filter(|candidate| method_is_applicable(candidate, &instance.parameters))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return input_err!(
                operation.deref(context).loc(),
                "no Method refining '{}' is applicable to the Intent parameters supplied by this operation",
                instance.operation
            );
        }

        let ordinal = self
            .next_choice
            .entry(instance.operation.clone())
            .or_default();
        let choice_id = format!("{}-{ordinal}", choice_label(&instance.operation));
        *ordinal += 1;
        let method_ids = candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        // A port whose state the Intent decides reads it from the result it
        // corresponds to, so the Intent's own result types are resolved first.
        let requested = requested_types(context, operation);
        let result_types = signature
            .outputs
            .iter()
            .zip(requested.iter().copied())
            .map(|(output, requested)| port_type(context, &output.port_type, requested))
            .collect::<Vec<_>>();
        let choice_artifact = instance
            .intent
            .action
            .results
            .iter()
            .any(|result| result.lineage == lab_language::ResultLineage::Begins)
            .then(|| {
                instance.intent.artifact.as_ref().map(|artifact| {
                    format!(
                        "{}::{}",
                        artifact.definition.module, artifact.definition.local
                    )
                })
            })
            .flatten();
        let choice = ChoiceOp::new_with_intent(
            context,
            &choice_id,
            instance.operation.as_str(),
            &method_ids,
            ChoicePorts {
                inputs: signature
                    .inputs
                    .iter()
                    .zip(operands.iter().copied())
                    .map(|(input, value)| (input.name.clone(), value))
                    .collect(),
                outputs: signature
                    .outputs
                    .iter()
                    .zip(result_types)
                    .map(|(output, ty)| (output.name.clone(), ty))
                    .collect(),
            },
            choice_artifact.as_deref(),
            &instance.intent.artifact_dependencies,
            &instance.intent,
        );

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            let parameters = resolve_method_parameters(candidate, &instance.parameters)
                .expect("applicable Method parameters resolve deterministically");
            append_candidate(
                context,
                &choice,
                candidate_index,
                &choice_id,
                candidate,
                &instance.intent,
                &parameters,
                self.procedures,
            )?;
        }
        rewriter.insert_operation(context, choice.get_operation());
        let old_results = operation.deref(context).results().collect::<Vec<_>>();
        let new_results = choice
            .get_operation()
            .deref(context)
            .results()
            .collect::<Vec<_>>();
        for (old, new) in old_results.into_iter().zip(new_results) {
            rewriter.replace_value_uses_with(context, old, new);
        }
        rewriter.erase_operation(context, operation);
        Ok(())
    }
}

struct IntentInstance {
    intent: crate::workflow::IntentAction,
    operation: IntentOperationId,
    parameters: BTreeMap<LocalId, ProcedureValue>,
    input_names: Vec<LocalId>,
    output_names: Vec<LocalId>,
}

fn intent_operation(context: &Context, operation: Ptr<Operation>) -> Option<IntentOperationId> {
    Operation::get_op::<PerformOp>(operation, context)
        .and_then(|perform| IntentOperationId::new(perform.operation(context)).ok())
}

fn intent_instance(context: &Context, operation: Ptr<Operation>) -> Result<IntentInstance> {
    let perform = Operation::get_op::<PerformOp>(operation, context)
        .expect("dialect conversion only queues workflow.perform operations");
    let intent = perform.intent(context);
    let semantic_operation = IntentOperationId::new(
        intent
            .operation()
            .expect("a verified Intent action has an operation"),
    )
    .expect("a verified Intent action has a stable operation ID");
    let parameters = intent
        .parameters
        .iter()
        .map(|(name, value)| {
            Ok((
                LocalId::new(name)
                    .map_err(|error| pliron::input_error!(operation.deref(context).loc(), error))?,
                value.clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let input_names = perform
        .operand_names(context)
        .into_iter()
        .map(|name| {
            LocalId::new(name)
                .map_err(|error| pliron::input_error!(operation.deref(context).loc(), error))
        })
        .collect::<Result<Vec<_>>>()?;
    let output_names = intent
        .action
        .results
        .iter()
        .map(|result| {
            LocalId::new(&result.name)
                .map_err(|error| pliron::input_error!(operation.deref(context).loc(), error))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(IntentInstance {
        intent,
        operation: semantic_operation,
        parameters,
        input_names,
        output_names,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal pass inputs and validation context are explicit"
)]
fn append_candidate(
    context: &mut Context,
    choice: &ChoiceOp,
    candidate_index: usize,
    choice_id: &str,
    method: &MethodDefinition,
    intent: &crate::workflow::IntentAction,
    parameters: &BTreeMap<LocalId, ProcedureValue>,
    procedures: &ProcedureCompiler,
) -> Result<()> {
    let candidate_inputs = choice
        .candidate_region(context, candidate_index)
        .deref(context)
        .get_head()
        .expect("method.choice construction creates a candidate block")
        .deref(context)
        .arguments()
        .collect::<Vec<_>>();
    // A task output whose state the Intent decides reads it from the choice
    // result the Method exports it as. Ports that name their own state ignore
    // this map entirely.
    let choice_results = choice
        .get_operation()
        .deref(context)
        .results()
        .map(|value| value.get_type(context))
        .collect::<Vec<_>>();
    let requested_by_task_output = method
        .outputs
        .iter()
        .zip(choice_results)
        .filter_map(|(output, ty)| match &output.source {
            ValueReference::TaskOutput { task, output } => {
                Some(((task.clone(), output.clone()), ty))
            }
            ValueReference::Input { .. } => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut values = method
        .inputs
        .iter()
        .zip(candidate_inputs)
        .map(|(input, value)| {
            (
                ValueReference::Input {
                    input: input.name.clone(),
                },
                value,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for task in &method.tasks {
        let task_operands = task
            .inputs
            .iter()
            .map(|reference| values[reference])
            .collect();
        let task_results = task
            .outputs
            .iter()
            .map(|output| {
                let requested = requested_by_task_output
                    .get(&(task.id.clone(), output.name.clone()))
                    .copied();
                port_type(context, &output.port_type, requested)
            })
            .collect();
        let node_id = qualified_id(choice_id, &method.id, &task.id);
        let output_names = task
            .outputs
            .iter()
            .map(|output| output.name.clone())
            .collect::<Vec<_>>();
        let task_op = TaskOp::new(
            context,
            &node_id,
            &task.operation,
            task_operands,
            task_results,
            &output_names,
        );
        for (index, output) in task.outputs.iter().enumerate() {
            values.insert(
                ValueReference::TaskOutput {
                    task: task.id.clone(),
                    output: output.name.clone(),
                },
                task_op.get_operation().deref(context).get_result(index),
            );
        }
        choice.append_candidate_operation(context, candidate_index, task_op.get_operation());

        let mut resolved_materials = Vec::new();
        for material in &task.materials {
            let symbols = resolve_material_symbols(
                operation_location(choice, context),
                &material.source,
                parameters,
            )?;
            let indexed = matches!(
                &material.source,
                MaterialSourceExpression::IntentParameter { parameter }
                    if matches!(parameters.get(parameter), Some(ProcedureValue::List { .. }))
            );
            for (index, symbol) in symbols.into_iter().enumerate() {
                let suffix = if indexed {
                    format!("::{index:04}")
                } else {
                    String::new()
                };
                let input_id = format!("{node_id}::material::{}{suffix}", material.id);
                resolved_materials.push(ResolvedProcedureMaterial {
                    id: LocalId::new(&input_id)
                        .expect("qualified Method material identity is stable"),
                    symbol: symbol.clone(),
                });
                let material_op = MaterialInputOp::new(context, input_id, &node_id, symbol);
                choice.append_candidate_operation(
                    context,
                    candidate_index,
                    material_op.get_operation(),
                );
            }
        }

        let mut resolved_parameters = Vec::new();
        for parameter in &task.parameters {
            let parameter_id = format!("{node_id}::parameter::{}", parameter.id);
            let value = resolve_procedure_value(
                operation_location(choice, context),
                &parameter.value,
                parameters,
            )?;
            resolved_parameters.push(ResolvedProcedureParameter {
                id: LocalId::new(&parameter_id)
                    .expect("qualified Method parameter identity is stable"),
                value: value.clone(),
            });
            let parameter_op = ParameterOp::new(
                context,
                parameter_id,
                &node_id,
                &parameter.property_kind,
                &value,
            );
            choice.append_candidate_operation(
                context,
                candidate_index,
                parameter_op.get_operation(),
            );
        }

        let derived_program = match &task.execution {
            ProcedureTaskExecutionDefinition::Template {
                contract,
                body,
                policy,
            } => Some((
                procedures
                    .render_template(
                        contract,
                        body,
                        &ProcedureProgramBuildContext {
                            intent,
                            input_count: task.inputs.len(),
                            outputs: &output_names,
                            parameters: &resolved_parameters,
                            materials: &resolved_materials,
                        },
                    )
                    .map_err(|error| {
                        pliron::input_error!(operation_location(choice, context), error)
                    })?,
                policy,
            )),
            ProcedureTaskExecutionDefinition::Builder {
                builder,
                contract,
                policy,
            } => Some((
                procedures
                    .build(
                        builder,
                        contract,
                        &ProcedureProgramBuildContext {
                            intent,
                            input_count: task.inputs.len(),
                            outputs: &output_names,
                            parameters: &resolved_parameters,
                            materials: &resolved_materials,
                        },
                    )
                    .map_err(|error| {
                        pliron::input_error!(operation_location(choice, context), error)
                    })?,
                policy,
            )),
            ProcedureTaskExecutionDefinition::Primitive { requirements } => {
                for requirement in requirements {
                    let requirement_id = format!("{node_id}::requirement::{}", requirement.id);
                    let constraints = requirement
                        .constraints
                        .iter()
                        .map(|constraint| {
                            let required = resolve_scalar_value(
                                operation_location(choice, context),
                                &constraint.required,
                                parameters,
                            )?;
                            Ok(PropertyConstraint {
                                property_kind: constraint.property_kind.clone(),
                                relation: constraint.relation,
                                required,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    append_requirement(
                        context,
                        choice,
                        candidate_index,
                        &node_id,
                        &requirement_id,
                        &requirement.capability_kind,
                        requirement.minimum_qualification,
                        requirement.accepted_control_modes.iter().copied(),
                        &constraints,
                    );
                }
                None
            }
        };
        if let Some((validated, policy)) = derived_program {
            task_op.set_semantic_program(context, validated.document());
            for clause in validated.capability_formula().all_of {
                let requirement_id = format!("{node_id}::requirement::{}", clause.role);
                append_requirement(
                    context,
                    choice,
                    candidate_index,
                    &node_id,
                    &requirement_id,
                    &clause.capability_kind,
                    policy.minimum_qualification,
                    policy.accepted_control_modes.iter().copied(),
                    &clause.constraints,
                );
            }
        }
    }
    let yielded = method
        .outputs
        .iter()
        .map(|output| values[&output.source])
        .collect();
    let yield_op = YieldOp::new(context, yielded);
    choice.append_candidate_operation(context, candidate_index, yield_op.get_operation());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_requirement(
    context: &mut Context,
    choice: &ChoiceOp,
    candidate_index: usize,
    node_id: &str,
    requirement_id: &str,
    capability_kind: &CapabilityKind,
    minimum_qualification: QualificationLevel,
    accepted_control_modes: impl IntoIterator<Item = ControlMode>,
    constraints: &[PropertyConstraint],
) {
    let requirement_op = RequirementOp::new(
        context,
        requirement_id,
        node_id,
        capability_kind,
        minimum_qualification,
        accepted_control_modes,
    );
    choice.append_candidate_operation(context, candidate_index, requirement_op.get_operation());
    for constraint in constraints {
        let constraint_op = ConstraintOp::new(context, requirement_id, constraint);
        choice.append_candidate_operation(context, candidate_index, constraint_op.get_operation());
    }
}

fn resolve_material_symbols(
    location: pliron::location::Location,
    expression: &MaterialSourceExpression,
    parameters: &BTreeMap<LocalId, ProcedureValue>,
) -> Result<Vec<String>> {
    match expression {
        MaterialSourceExpression::Literal { symbol } => Ok(vec![symbol.clone()]),
        MaterialSourceExpression::IntentParameter { parameter } => {
            let Some(value) = parameters.get(parameter) else {
                return input_err!(location, "Intent parameter '{parameter}' is unavailable");
            };
            match value {
                ProcedureValue::Scalar { value } => match &value.value {
                    ScalarValue::Text(symbol) => Ok(vec![symbol.clone()]),
                    _ => input_err!(
                        location,
                        "material input parameter '{parameter}' is not text-valued"
                    ),
                },
                ProcedureValue::List { values, .. } => values
                    .iter()
                    .map(|value| match &value.value {
                        ScalarValue::Text(symbol) => Ok(symbol.clone()),
                        _ => input_err!(
                            location.clone(),
                            "material input parameter '{parameter}' contains a non-text value"
                        ),
                    })
                    .collect(),
            }
        }
    }
}

fn resolve_scalar_value(
    location: pliron::location::Location,
    expression: &ScalarValueExpression,
    parameters: &BTreeMap<LocalId, ProcedureValue>,
) -> Result<PropertyValue> {
    match expression {
        ScalarValueExpression::Literal { value } => Ok(value.clone()),
        ScalarValueExpression::IntentParameter { parameter, unit } => {
            let Some(ProcedureValue::Scalar { value: source }) = parameters.get(parameter) else {
                return input_err!(location, "Intent parameter '{parameter}' is unavailable");
            };
            let resolved_unit = match (&source.unit, unit) {
                (Some(source), Some(required)) if source != required => {
                    return input_err!(
                        location,
                        "Intent parameter '{parameter}' uses unit '{source}', but the method requires '{required}'"
                    );
                }
                (Some(source), _) => Some(source.clone()),
                (None, required) => required.clone(),
            };
            PropertyValue::new(source.value.clone(), resolved_unit)
                .map_err(|error| pliron::input_error!(location, error))
        }
    }
}

fn resolve_procedure_value(
    location: pliron::location::Location,
    expression: &ProcedureValueExpression,
    parameters: &BTreeMap<LocalId, ProcedureValue>,
) -> Result<ProcedureValue> {
    match expression {
        ProcedureValueExpression::Literal { value } => Ok(value.clone()),
        ProcedureValueExpression::IntentParameter { parameter, unit } => {
            let Some(source) = parameters.get(parameter) else {
                return input_err!(location, "Intent parameter '{parameter}' is unavailable");
            };
            match source {
                ProcedureValue::Scalar { value } => {
                    let resolved_unit = match (&value.unit, unit) {
                        (Some(source), Some(required)) if source != required => {
                            return input_err!(
                                location,
                                "Intent parameter '{parameter}' uses unit '{source}', but the method requires '{required}'"
                            );
                        }
                        (Some(source), _) => Some(source.clone()),
                        (None, required) => required.clone(),
                    };
                    let value = PropertyValue::new(value.value.clone(), resolved_unit)
                        .map_err(|error| pliron::input_error!(location, error))?;
                    Ok(ProcedureValue::Scalar { value })
                }
                ProcedureValue::List { .. } if unit.is_some() => input_err!(
                    location,
                    "Intent list parameter '{parameter}' cannot be assigned a unit"
                ),
                ProcedureValue::List { .. } => Ok(source.clone()),
            }
        }
    }
}

fn verify_inputs(
    context: &Context,
    operation: Ptr<Operation>,
    intent_operation: &IntentOperationId,
    expected: &[crate::method::MethodInput],
    actual_names: &[LocalId],
    operands: &[Value],
) -> Result<()> {
    if expected.len() != operands.len() || actual_names.len() != operands.len() {
        return input_err!(
            operation.deref(context).loc(),
            "Method for Intent operation '{}' expects {} inputs, but the checked action has {}",
            intent_operation,
            expected.len(),
            operands.len()
        );
    }
    for ((expected, actual_name), actual) in expected.iter().zip(actual_names).zip(operands) {
        if &expected.name != actual_name {
            return input_err!(
                operation.deref(context).loc(),
                "Intent input '{}' does not match Method input '{}'",
                actual_name,
                expected.name
            );
        }
        let actual_type = actual.get_type(context);
        let matches = if matches!(expected.port_type, PortType::MaterialAsSupplied) {
            let actual = actual_type.deref(context);
            actual.downcast_ref::<WorkflowMaterialType>().is_some()
                || actual.downcast_ref::<ProcedureMaterialType>().is_some()
        } else {
            port_type_readonly(context, &expected.port_type, None) == actual_type
        };
        if !matches {
            return input_err!(
                operation.deref(context).loc(),
                "Intent input '{}' does not match its method signature",
                expected.name
            );
        }
    }
    Ok(())
}

fn method_is_applicable(
    method: &MethodDefinition,
    actual: &BTreeMap<LocalId, ProcedureValue>,
) -> bool {
    resolve_method_parameters(method, actual).is_some()
}

/// Bind a Method's local parameter names to exact Intent values, applying only defaults declared
/// by that Method. A present value with the wrong type makes the candidate inapplicable; it never
/// falls through to a default and hides a malformed scientific statement.
fn resolve_method_parameters(
    method: &MethodDefinition,
    actual: &BTreeMap<LocalId, ProcedureValue>,
) -> Option<BTreeMap<LocalId, ProcedureValue>> {
    method
        .parameters
        .iter()
        .map(|parameter| {
            let source = parameter.source.as_ref().unwrap_or(&parameter.name);
            let value = match actual.get(source) {
                Some(value) if value.value_type() == parameter.value_type => value.clone(),
                Some(_) => return None,
                None => parameter.resolved_default().ok().flatten()?,
            };
            Some((parameter.name.clone(), value))
        })
        .collect()
}

fn verify_results(
    context: &Context,
    operation: Ptr<Operation>,
    expected: &[crate::method::TaskOutput],
    actual_names: &[LocalId],
) -> Result<()> {
    let actual = operation.deref(context).results().collect::<Vec<_>>();
    if expected.len() != actual.len() || actual_names.len() != actual.len() {
        return input_err!(
            operation.deref(context).loc(),
            "method signature yields {} results, but Intent operation has {}",
            expected.len(),
            actual.len()
        );
    }
    for ((expected, actual_name), actual) in expected.iter().zip(actual_names).zip(actual) {
        if &expected.name != actual_name {
            return input_err!(
                operation.deref(context).loc(),
                "Intent result '{}' does not match Method output '{}'",
                actual_name,
                expected.name
            );
        }
        let actual_type = converted_type_readonly(context, actual.get_type(context));
        if port_type_readonly(context, &expected.port_type, Some(actual_type)) != actual_type {
            return input_err!(
                operation.deref(context).loc(),
                "Intent result '{}' does not match its method signature",
                expected.name
            );
        }
    }
    Ok(())
}

fn converted_type_readonly(context: &Context, ty: TypeHandle) -> TypeHandle {
    let handle = ty.deref(context);
    if let Some(material) = handle.downcast_ref::<WorkflowMaterialType>() {
        return ProcedureMaterialType::get(context, StringAttr::new(material.iri().to_owned()))
            .into();
    }
    if let Some(data) = handle.downcast_ref::<WorkflowDataType>() {
        return ProcedureDataType::get(context, StringAttr::new(data.iri().to_owned())).into();
    }
    ty
}

fn converted_type(context: &mut Context, ty: TypeHandle) -> TypeHandle {
    converted_type_readonly(context, ty)
}

/// The concrete type of one port.
///
/// `requested` is the type the Intent operation's corresponding result carries,
/// which is what a port whose state the Intent decides resolves to. Every other
/// port names its own type and ignores it.
fn port_type(
    context: &mut Context,
    port_type: &PortType,
    requested: Option<TypeHandle>,
) -> TypeHandle {
    match port_type {
        PortType::Design => DesignType::get(context).into(),
        PortType::Material { state } => procedure_material_type(context, state.as_str()),
        PortType::MaterialAsRequested => requested
            .expect("a requested port is resolved against the Intent result it corresponds to"),
        PortType::MaterialAsSupplied => requested
            .expect("a supplied port is resolved against the Intent operand it corresponds to"),
        PortType::Data { data_kind } => {
            ProcedureDataType::get(context, StringAttr::new(data_kind.to_string())).into()
        }
    }
}

fn port_type_readonly(
    context: &Context,
    port_type: &PortType,
    requested: Option<TypeHandle>,
) -> TypeHandle {
    match port_type {
        PortType::Design => DesignType::get(context).into(),
        PortType::Material { state } => {
            ProcedureMaterialType::get(context, StringAttr::new(state.to_string())).into()
        }
        PortType::MaterialAsRequested => requested
            .expect("a requested port is resolved against the Intent result it corresponds to"),
        PortType::MaterialAsSupplied => requested
            .expect("a supplied port is resolved against the Intent operand it corresponds to"),
        PortType::Data { data_kind } => {
            ProcedureDataType::get(context, StringAttr::new(data_kind.to_string())).into()
        }
    }
}

/// The type each of an Intent operation's results carries, in Procedure terms.
///
/// This is what a port whose state the Intent decides resolves to. The results
/// were checked against the Method signature before this runs, so a port that
/// names its own state has already been agreed with the one here.
fn requested_types(context: &Context, operation: Ptr<Operation>) -> Vec<Option<TypeHandle>> {
    operation
        .deref(context)
        .results()
        .map(|value| Some(converted_type_readonly(context, value.get_type(context))))
        .collect()
}

fn procedure_material_type(context: &mut Context, state: &str) -> TypeHandle {
    ProcedureMaterialType::get(context, StringAttr::new(state.to_owned())).into()
}

fn qualified_id(choice: &str, method: &lab_capability::MethodId, local: &LocalId) -> String {
    format!("{choice}::{method}::{local}")
}

fn choice_label(operation: &IntentOperationId) -> String {
    operation
        .as_str()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn operation_location(choice: &ChoiceOp, context: &Context) -> pliron::location::Location {
    choice.loc(context)
}
