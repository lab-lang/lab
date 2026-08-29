//! Facility-independent projection of validated method definitions into LAIR candidate regions.

use std::collections::BTreeMap;

use lab_capability::{
    ExactDecimal, ExactInteger, PropertyConstraint, PropertyValue, ScalarValue, UnitIri,
};
use lab_method::{
    IntentOperationId, LocalId, MethodDefinition, MethodRegistry, PortType, ScalarType,
    ScalarValueExpression, ValueReference,
};
use pliron::builtin::attributes::StringAttr;
use pliron::context::{Context, Ptr};
use pliron::input_err;
use pliron::irbuild::dialect_conversion::{
    DialectConversion, DialectConversionRewriter, OperandsInfo, apply_dialect_conversion,
};
use pliron::irbuild::inserter::Inserter;
use pliron::irbuild::rewriter::Rewriter;
use pliron::location::Located;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::{TypeHandle, Typed};
use pliron::value::Value;

use crate::lair::dialect::attributes::{quantity_entry, u32_value};
use crate::lair::dialect::capability::{ConstraintOp, RequirementOp};
use crate::lair::dialect::chemistry::{ASSEMBLY_CHEMISTRY_KEYS, STRAIN_CHEMISTRY_KEYS};
use crate::lair::dialect::design::DesignType;
use crate::lair::dialect::method::{ChoiceOp, ChoicePorts, YieldOp};
use crate::lair::dialect::procedure::{
    DataType as ProcedureDataType, MaterialType as ProcedureMaterialType, ParameterOp, TaskOp,
};
use crate::lair::dialect::workflow::{
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
        let candidates = self.registry.methods_for(&instance.operation);
        if candidates.is_empty() {
            return input_err!(
                operation.deref(context).loc(),
                "no method definition refines Intent operation '{}'",
                instance.operation
            );
        }
        let signature = candidates[0]
            .validate()
            .expect("MethodRegistry contains only validated definitions");
        let operands = operation.deref(context).operands().collect::<Vec<_>>();
        verify_inputs(context, operation, &signature.inputs, &operands)?;
        verify_parameters(
            operation,
            context,
            &signature.parameters,
            &instance.parameters,
        )?;
        verify_results(context, operation, &signature.outputs)?;

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
        );

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            append_candidate(
                context,
                &choice,
                candidate_index,
                &choice_id,
                candidate,
                &operands,
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

struct IntentInstance {
    operation: IntentOperationId,
    parameters: BTreeMap<LocalId, PropertyValue>,
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
            "restriction_enzyme",
            required_string(realize.get_attr_realize_restriction_enzyme(context)),
        );
        insert_integer(
            &mut parameters,
            "assembly_replicates",
            u32_value(
                &realize
                    .get_attr_realize_assembly_replicates(context)
                    .unwrap(),
            ),
        );
        let chemistry = realize.get_attr_realize_chemistry(context).unwrap();
        insert_chemistry(&mut parameters, &chemistry, ASSEMBLY_CHEMISTRY_KEYS);
    } else if let Some(provision) = Operation::get_op::<ProvisionOp>(operation, context) {
        insert_text(
            &mut parameters,
            "item",
            required_string(provision.get_attr_provision_item(context)),
        );
    } else if let Some(transform) = Operation::get_op::<TransformOp>(operation, context) {
        insert_integer(
            &mut parameters,
            "replicates",
            u32_value(&transform.get_attr_transform_replicates(context).unwrap()),
        );
        let chemistry = transform.get_attr_transform_chemistry(context).unwrap();
        insert_chemistry(&mut parameters, &chemistry, STRAIN_CHEMISTRY_KEYS);
    } else if let Some(recover) = Operation::get_op::<RecoverOp>(operation, context) {
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
            PropertyValue::new(ScalarValue::Real(scalar), Some(unit)).unwrap(),
        );
    } else if let Some(dilute) = Operation::get_op::<DiluteOp>(operation, context) {
        insert_integer(
            &mut parameters,
            "serial_dilutions",
            u32_value(&dilute.get_attr_dilute_serial_dilutions(context).unwrap()),
        );
    } else if let Some(plate) = Operation::get_op::<PlateOp>(operation, context) {
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
    operands: &[Value],
    parameters: &BTreeMap<LocalId, PropertyValue>,
) -> Result<()> {
    let mut values = method
        .inputs
        .iter()
        .zip(operands)
        .map(|(input, value)| {
            (
                ValueReference::Input {
                    input: input.name.clone(),
                },
                *value,
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
        let task_op = TaskOp::new(
            context,
            &node_id,
            &task.operation,
            task_operands,
            task_results,
            &task
                .outputs
                .iter()
                .map(|output| output.name.clone())
                .collect::<Vec<_>>(),
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

        for parameter in &task.parameters {
            let parameter_id = format!("{node_id}::parameter::{}", parameter.id);
            let value = resolve_value(
                operation_location(choice, context),
                &parameter.value,
                parameters,
            )?;
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

        for requirement in &task.requirements {
            let requirement_id = format!("{node_id}::requirement::{}", requirement.id);
            let requirement_op = RequirementOp::new(
                context,
                &requirement_id,
                &node_id,
                &requirement.capability_kind,
                requirement.minimum_qualification,
                requirement.accepted_control_modes.iter().copied(),
            );
            choice.append_candidate_operation(
                context,
                candidate_index,
                requirement_op.get_operation(),
            );
            for constraint in &requirement.constraints {
                let required = resolve_value(
                    operation_location(choice, context),
                    &constraint.required,
                    parameters,
                )?;
                let constraint = PropertyConstraint {
                    property_kind: constraint.property_kind.clone(),
                    relation: constraint.relation,
                    required,
                };
                let constraint_op = ConstraintOp::new(context, &requirement_id, &constraint);
                choice.append_candidate_operation(
                    context,
                    candidate_index,
                    constraint_op.get_operation(),
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

fn resolve_value(
    location: pliron::location::Location,
    expression: &ScalarValueExpression,
    parameters: &BTreeMap<LocalId, PropertyValue>,
) -> Result<PropertyValue> {
    match expression {
        ScalarValueExpression::Literal { value } => Ok(value.clone()),
        ScalarValueExpression::IntentParameter { parameter, unit } => {
            let Some(source) = parameters.get(parameter) else {
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

fn verify_inputs(
    context: &Context,
    operation: Ptr<Operation>,
    expected: &[lab_method::MethodInput],
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

fn verify_parameters(
    operation: Ptr<Operation>,
    context: &Context,
    expected: &[lab_method::MethodParameter],
    actual: &BTreeMap<LocalId, PropertyValue>,
) -> Result<()> {
    if expected.len() != actual.len() {
        return input_err!(
            operation.deref(context).loc(),
            "method signature expects {} parameters, but Intent operation provides {}",
            expected.len(),
            actual.len()
        );
    }
    for parameter in expected {
        let Some(value) = actual.get(&parameter.name) else {
            return input_err!(
                operation.deref(context).loc(),
                "Intent parameter '{}' required by the method signature is unavailable",
                parameter.name
            );
        };
        if ScalarType::of(&value.value) != parameter.scalar_type {
            return input_err!(
                operation.deref(context).loc(),
                "Intent parameter '{}' does not match its method scalar type",
                parameter.name
            );
        }
    }
    Ok(())
}

fn verify_results(
    context: &Context,
    operation: Ptr<Operation>,
    expected: &[lab_method::TaskOutput],
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

fn insert_text(parameters: &mut BTreeMap<LocalId, PropertyValue>, name: &str, value: String) {
    parameters.insert(
        local(name),
        PropertyValue::unitless(ScalarValue::Text(value)),
    );
}

fn insert_integer(parameters: &mut BTreeMap<LocalId, PropertyValue>, name: &str, value: u32) {
    parameters.insert(
        local(name),
        PropertyValue::unitless(ScalarValue::Integer(
            ExactInteger::parse(value.to_string()).unwrap(),
        )),
    );
}

fn insert_chemistry(
    parameters: &mut BTreeMap<LocalId, PropertyValue>,
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
