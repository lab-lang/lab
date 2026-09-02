//! Facility-independent projection of validated method definitions into LAIR candidate regions.

use std::collections::BTreeMap;

use crate::method::{
    IntentOperationId, LocalId, MaterialSourceExpression, MethodDefinition, MethodRegistry,
    PortType, ProcedureValue, ProcedureValueExpression, ScalarType, ScalarValueExpression,
    ValueReference,
};
use lab_capability::{
    CapabilityKind, ControlMode, ExactDecimal, ExactInteger, PropertyConstraint, PropertyValue,
    QualificationLevel, ScalarValue, UnitIri,
};
use pliron::attribute::AttrObj;
use pliron::builtin::attributes::{StringAttr, VecAttr};
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
use crate::ir::attributes::{quantity_entry, u32_value};
use crate::method::ir::{ChoiceOp, ChoicePorts, YieldOp};
use crate::procedure::ir::{
    DataType as ProcedureDataType, MaterialInputOp, MaterialType as ProcedureMaterialType,
    ParameterOp, TaskOp,
};
use crate::procedure::normalization::{
    ProcedureTaskInstance, ResolvedProcedureMaterial, ResolvedProcedureParameter, normalize_task,
};
use crate::workflow::chemistry::{ASSEMBLY_CHEMISTRY_KEYS, STRAIN_CHEMISTRY_KEYS};
use crate::workflow::ir::{
    DiluteOp, MaterialType as WorkflowMaterialType, PlateOp, ProvisionOp, RealizeOp, RecoverOp,
    TransformOp,
};

pub(crate) fn refine_method_alternatives(
    context: &mut Context,
    root: Ptr<Operation>,
    registry: &MethodRegistry,
) -> Result<()> {
    apply_dialect_conversion(context, &mut MethodRefinement::new(registry), root)?;
    Ok(())
}

struct MethodRefinement<'a> {
    registry: &'a MethodRegistry,
    next_choice: BTreeMap<IntentOperationId, usize>,
}

impl<'a> MethodRefinement<'a> {
    fn new(registry: &'a MethodRegistry) -> Self {
        Self {
            registry,
            next_choice: BTreeMap::new(),
        }
    }
}

impl DialectConversion for MethodRefinement<'_> {
    fn can_convert_op(&self, context: &Context, operation: Ptr<Operation>) -> bool {
        intent_operation(context, operation).is_some()
    }

    fn can_convert_type(&self, context: &Context, ty: TypeHandle) -> bool {
        ty.deref(context)
            .downcast_ref::<WorkflowMaterialType>()
            .is_some()
    }

    fn convert_type(&mut self, context: &mut Context, ty: TypeHandle) -> Result<TypeHandle> {
        let material = {
            let ty = ty.deref(context);
            ty.downcast_ref::<WorkflowMaterialType>().copied()
        };
        Ok(material.map_or(ty, |material| workflow_material_type(context, material)))
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
        verify_inputs(context, operation, &signature.inputs, &operands)?;
        verify_results(context, operation, &signature.outputs)?;
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
        let result_types = signature
            .outputs
            .iter()
            .map(|output| port_type(context, &output.port_type))
            .collect::<Vec<_>>();
        let choice_artifact = text_parameter(&instance.parameters, "artifact");
        let choice_dependencies = text_list_parameter(&instance.parameters, "dependencies");
        let choice = ChoiceOp::new(
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
            &choice_dependencies,
        );

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            append_candidate(
                context,
                &choice,
                candidate_index,
                &choice_id,
                candidate,
                &instance.parameters,
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

fn text_parameter(parameters: &BTreeMap<LocalId, ProcedureValue>, name: &str) -> Option<String> {
    let ProcedureValue::Scalar { value } = parameters.get(&local(name))? else {
        return None;
    };
    let ScalarValue::Text(value) = &value.value else {
        return None;
    };
    Some(value.clone())
}

fn text_list_parameter(parameters: &BTreeMap<LocalId, ProcedureValue>, name: &str) -> Vec<String> {
    let Some(ProcedureValue::List { values, .. }) = parameters.get(&local(name)) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|value| match &value.value {
            ScalarValue::Text(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

struct IntentInstance {
    operation: IntentOperationId,
    parameters: BTreeMap<LocalId, ProcedureValue>,
}

fn intent_operation(context: &Context, operation: Ptr<Operation>) -> Option<IntentOperationId> {
    let value = if Operation::get_op::<RealizeOp>(operation, context).is_some() {
        "std.bio.build.realize"
    } else if Operation::get_op::<ProvisionOp>(operation, context).is_some() {
        "std.lab.plasmid.provision"
    } else if Operation::get_op::<TransformOp>(operation, context).is_some() {
        "std.lab.plasmid.transform"
    } else if Operation::get_op::<RecoverOp>(operation, context).is_some() {
        "std.lab.plasmid.recover"
    } else if Operation::get_op::<DiluteOp>(operation, context).is_some() {
        "std.lab.plasmid.dilute"
    } else if Operation::get_op::<PlateOp>(operation, context).is_some() {
        "std.lab.plasmid.plate"
    } else {
        return None;
    };
    Some(IntentOperationId::new(value).expect("standard Intent operation identities are valid"))
}

fn intent_instance(context: &Context, operation: Ptr<Operation>) -> Result<IntentInstance> {
    let semantic_operation = intent_operation(context, operation)
        .expect("dialect conversion only queues supported Workflow operations");
    let mut parameters = BTreeMap::new();
    if let Some(realize) = Operation::get_op::<RealizeOp>(operation, context) {
        insert_text(
            &mut parameters,
            "artifact",
            required_string(realize.get_attr_realize_artifact(context)),
        );
        insert_text_list(
            &mut parameters,
            "dependencies",
            required_strings(realize.get_attr_realize_dependencies(context)),
        );
        if let Some(restriction_enzyme) = realize.get_attr_realize_restriction_enzyme(context) {
            insert_text(
                &mut parameters,
                "backbone",
                required_string(realize.get_attr_realize_backbone(context)),
            );
            insert_text_list(
                &mut parameters,
                "components",
                required_strings(realize.get_attr_realize_components(context)),
            );
            insert_text(
                &mut parameters,
                "restriction_enzyme",
                restriction_enzyme.as_str().to_owned(),
            );
            insert_integer(
                &mut parameters,
                "assembly_replicates",
                u32_value(
                    &realize
                        .get_attr_realize_assembly_replicates(context)
                        .expect("verified Golden Gate recipe is complete"),
                ),
            );
            let chemistry = realize
                .get_attr_realize_chemistry(context)
                .expect("verified Golden Gate recipe is complete");
            insert_chemistry(&mut parameters, &chemistry, ASSEMBLY_CHEMISTRY_KEYS);
        }
    } else if let Some(provision) = Operation::get_op::<ProvisionOp>(operation, context) {
        insert_text(
            &mut parameters,
            "item",
            required_string(provision.get_attr_provision_item(context)),
        );
    } else if let Some(transform) = Operation::get_op::<TransformOp>(operation, context) {
        insert_text(
            &mut parameters,
            "artifact",
            required_string(transform.get_attr_transform_artifact(context)),
        );
        insert_text(
            &mut parameters,
            "chassis",
            required_string(transform.get_attr_transform_chassis(context)),
        );
        insert_text_list(
            &mut parameters,
            "plasmids",
            required_strings(transform.get_attr_transform_plasmids(context)),
        );
        insert_text_list(
            &mut parameters,
            "dependencies",
            required_strings(transform.get_attr_transform_dependencies(context)),
        );
        insert_integer(
            &mut parameters,
            "replicates",
            u32_value(&transform.get_attr_transform_replicates(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "dna_count",
            u32::try_from(required_strings(transform.get_attr_transform_plasmids(context)).len())
                .expect("verified string-vector length fits u32"),
        );
        let chemistry = transform.get_attr_transform_chemistry(context).unwrap();
        insert_chemistry(&mut parameters, &chemistry, STRAIN_CHEMISTRY_KEYS);
    } else if let Some(recover) = Operation::get_op::<RecoverOp>(operation, context) {
        insert_text(
            &mut parameters,
            "subject",
            required_string(recover.get_attr_recover_artifact(context)),
        );
        let magnitude = required_string(recover.get_attr_recover_duration_magnitude(context));
        let scalar = ExactDecimal::parse(&magnitude)
            .map_err(|error| pliron::input_error!(operation.deref(context).loc(), error))?;
        let source_unit = required_string(recover.get_attr_recover_duration_unit(context));
        let unit = match source_unit.as_str() {
            "h" => UnitIri::new("http://qudt.org/vocab/unit/HR").unwrap(),
            "min" => UnitIri::new("http://qudt.org/vocab/unit/MIN").unwrap(),
            _ => {
                return input_err!(
                    operation.deref(context).loc(),
                    "unsupported recovery duration unit '{source_unit}'"
                );
            }
        };
        parameters.insert(
            local("duration"),
            ProcedureValue::Scalar {
                value: PropertyValue::new(ScalarValue::Real(scalar), Some(unit)).unwrap(),
            },
        );
        insert_integer(
            &mut parameters,
            "replicates",
            u32_value(&recover.get_attr_recover_replicates(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "initial_volume_ul",
            u32_value(&recover.get_attr_recover_initial_volume_ul(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "recovery_aliquot_volume_ul",
            u32_value(
                &recover
                    .get_attr_recover_medium_aliquot_volume_ul(context)
                    .unwrap(),
            ),
        );
        insert_integer(
            &mut parameters,
            "recovery_volume_ul",
            u32_value(&recover.get_attr_recover_medium_volume_ul(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "recovery_temperature_c",
            u32_value(&recover.get_attr_recover_temperature_c(context).unwrap()),
        );
    } else if let Some(dilute) = Operation::get_op::<DiluteOp>(operation, context) {
        insert_text(
            &mut parameters,
            "subject",
            required_string(dilute.get_attr_dilute_artifact(context)),
        );
        insert_integer(
            &mut parameters,
            "serial_dilutions",
            u32_value(&dilute.get_attr_dilute_serial_dilutions(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "replicates",
            u32_value(&dilute.get_attr_dilute_replicates(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "initial_volume_ul",
            u32_value(&dilute.get_attr_dilute_initial_volume_ul(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "medium_volume_ul",
            u32_value(&dilute.get_attr_dilute_medium_volume_ul(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "culture_volume_ul",
            u32_value(&dilute.get_attr_dilute_culture_volume_ul(context).unwrap()),
        );
    } else if let Some(plate) = Operation::get_op::<PlateOp>(operation, context) {
        insert_text(
            &mut parameters,
            "subject",
            required_string(plate.get_attr_plate_artifact(context)),
        );
        insert_text(
            &mut parameters,
            "selection",
            required_string(plate.get_attr_plate_selection(context)),
        );
        insert_integer(
            &mut parameters,
            "replicates",
            u32_value(&plate.get_attr_plate_replicates(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "culture_replicates",
            u32_value(&plate.get_attr_plate_culture_replicates(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "serial_dilutions",
            u32_value(&plate.get_attr_plate_serial_dilutions(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "medium_volume_ul",
            u32_value(&plate.get_attr_plate_medium_volume_ul(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "culture_volume_ul",
            u32_value(&plate.get_attr_plate_culture_volume_ul(context).unwrap()),
        );
        insert_integer(
            &mut parameters,
            "colony_volume_ul",
            u32_value(&plate.get_attr_plate_colony_volume_ul(context).unwrap()),
        );
    }
    Ok(IntentInstance {
        operation: semantic_operation,
        parameters,
    })
}

fn append_candidate(
    context: &mut Context,
    choice: &ChoiceOp,
    candidate_index: usize,
    choice_id: &str,
    method: &MethodDefinition,
    parameters: &BTreeMap<LocalId, ProcedureValue>,
) -> Result<()> {
    let candidate_inputs = choice
        .candidate_region(context, candidate_index)
        .deref(context)
        .get_head()
        .expect("method.choice construction creates a candidate block")
        .deref(context)
        .arguments()
        .collect::<Vec<_>>();
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
            .map(|output| port_type(context, &output.port_type))
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

        let semantic_node_id =
            LocalId::new(&node_id).expect("qualified Method task identity is stable");
        let normalized_program = normalize_task(&ProcedureTaskInstance {
            id: &semantic_node_id,
            operation: &task.operation,
            input_count: task.inputs.len(),
            outputs: &output_names,
            parameters: &resolved_parameters,
            materials: &resolved_materials,
        })
        .map_err(|error| pliron::input_error!(operation_location(choice, context), error))?;
        if let Some(program) = &normalized_program {
            task_op.set_semantic_program(context, program);
        }

        if let Some(program) = &normalized_program {
            let [policy] = task.requirements.as_slice() else {
                return input_err!(
                    operation_location(choice, context),
                    "normalized Procedure task '{}' must declare exactly one execution policy requirement before capability derivation",
                    task.id
                );
            };
            if !policy.constraints.is_empty() {
                return input_err!(
                    operation_location(choice, context),
                    "normalized Procedure task '{}' execution policy cannot carry capability constraints; the canonical program derives them",
                    task.id
                );
            }
            let formula = program
                .validate()
                .expect("compiler normalization returns a validated Procedure program")
                .capability_formula();
            for clause in formula.all_of {
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
        } else {
            for requirement in &task.requirements {
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
    expected: &[crate::method::MethodInput],
    operands: &[Value],
) -> Result<()> {
    if expected.len() != operands.len() {
        return input_err!(
            operation.deref(context).loc(),
            "method signature expects {} inputs, but Intent operation has {}",
            expected.len(),
            operands.len()
        );
    }
    for (expected, actual) in expected.iter().zip(operands) {
        if port_type_readonly(context, &expected.port_type) != actual.get_type(context) {
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
    method.parameters.iter().all(|parameter| {
        actual
            .get(&parameter.name)
            .is_some_and(|value| value.value_type() == parameter.value_type)
    })
}

fn verify_results(
    context: &Context,
    operation: Ptr<Operation>,
    expected: &[crate::method::TaskOutput],
) -> Result<()> {
    let actual = operation.deref(context).results().collect::<Vec<_>>();
    if expected.len() != actual.len() {
        return input_err!(
            operation.deref(context).loc(),
            "method signature yields {} results, but Intent operation has {}",
            expected.len(),
            actual.len()
        );
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let actual_type = converted_type_readonly(context, actual.get_type(context));
        if port_type_readonly(context, &expected.port_type) != actual_type {
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
    let material = {
        let ty = ty.deref(context);
        ty.downcast_ref::<WorkflowMaterialType>().copied()
    };
    material.map_or(ty, |material| {
        workflow_material_type_readonly(context, material)
    })
}

fn workflow_material_type(context: &mut Context, material: WorkflowMaterialType) -> TypeHandle {
    procedure_material_type(context, workflow_state(material))
}

fn workflow_material_type_readonly(
    context: &Context,
    material: WorkflowMaterialType,
) -> TypeHandle {
    ProcedureMaterialType::get(
        context,
        StringAttr::new(workflow_state(material).to_owned()),
    )
    .into()
}

fn workflow_state(material: WorkflowMaterialType) -> &'static str {
    match material {
        WorkflowMaterialType::PlasmidProduct => {
            "https://www.lab-compiler.org/ns/material-state#PlasmidProduct"
        }
        WorkflowMaterialType::StrainProduct => {
            "https://www.lab-compiler.org/ns/material-state#StrainProduct"
        }
        WorkflowMaterialType::CompetentCells => {
            "https://www.lab-compiler.org/ns/material-state#CompetentCells"
        }
        WorkflowMaterialType::TransformedCulture => {
            "https://www.lab-compiler.org/ns/material-state#TransformedCulture"
        }
        WorkflowMaterialType::RecoveredCulture => {
            "https://www.lab-compiler.org/ns/material-state#RecoveredCulture"
        }
        WorkflowMaterialType::DilutedCulture => {
            "https://www.lab-compiler.org/ns/material-state#DilutedCulture"
        }
        WorkflowMaterialType::Plate => "https://www.lab-compiler.org/ns/material-state#Plate",
    }
}

fn port_type(context: &mut Context, port_type: &PortType) -> TypeHandle {
    match port_type {
        PortType::Design => DesignType::get(context).into(),
        PortType::Material { state } => procedure_material_type(context, state.as_str()),
        PortType::Data { data_kind } => {
            ProcedureDataType::get(context, StringAttr::new(data_kind.to_string())).into()
        }
    }
}

fn port_type_readonly(context: &Context, port_type: &PortType) -> TypeHandle {
    match port_type {
        PortType::Design => DesignType::get(context).into(),
        PortType::Material { state } => {
            ProcedureMaterialType::get(context, StringAttr::new(state.to_string())).into()
        }
        PortType::Data { data_kind } => {
            ProcedureDataType::get(context, StringAttr::new(data_kind.to_string())).into()
        }
    }
}

fn procedure_material_type(context: &mut Context, state: &str) -> TypeHandle {
    ProcedureMaterialType::get(context, StringAttr::new(state.to_owned())).into()
}

fn insert_text(parameters: &mut BTreeMap<LocalId, ProcedureValue>, name: &str, value: String) {
    parameters.insert(
        local(name),
        ProcedureValue::Scalar {
            value: PropertyValue::unitless(ScalarValue::Text(value)),
        },
    );
}

fn insert_text_list(
    parameters: &mut BTreeMap<LocalId, ProcedureValue>,
    name: &str,
    values: Vec<String>,
) {
    parameters.insert(
        local(name),
        ProcedureValue::List {
            element_type: ScalarType::Text,
            values: values
                .into_iter()
                .map(|value| PropertyValue::unitless(ScalarValue::Text(value)))
                .collect(),
        },
    );
}

fn insert_integer(parameters: &mut BTreeMap<LocalId, ProcedureValue>, name: &str, value: u32) {
    parameters.insert(
        local(name),
        ProcedureValue::Scalar {
            value: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse(value.to_string()).unwrap(),
            )),
        },
    );
}

fn insert_chemistry(
    parameters: &mut BTreeMap<LocalId, ProcedureValue>,
    chemistry: &pliron::builtin::attributes::DictAttr,
    keys: &[&str],
) {
    for key in keys {
        insert_integer(parameters, key, quantity_entry(Some(chemistry), key, 0));
    }
}

fn required_string(value: Option<std::cell::Ref<'_, StringAttr>>) -> String {
    value
        .expect("verified Workflow operation carries its required string attribute")
        .as_str()
        .to_owned()
}

fn required_strings(value: Option<std::cell::Ref<'_, VecAttr>>) -> Vec<String> {
    value
        .expect("verified Workflow operation carries its required vector attribute")
        .0
        .iter()
        .map(|value: &AttrObj| {
            value
                .downcast_ref::<StringAttr>()
                .expect("verified Workflow vector contains strings")
                .as_str()
                .to_owned()
        })
        .collect()
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

fn local(value: &str) -> LocalId {
    LocalId::new(value).expect("built-in parameter names are stable local identities")
}
