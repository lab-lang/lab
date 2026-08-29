//! Backend projection from one exact allocated Procedure graph into the mature dependency-build IR.
//!
//! This is intentionally downstream of Method and facility selection. It consumes only verifier-valid
//! allocated LAIR, never checked source or unresolved Workflow Intent, so an adapter cannot silently
//! choose a different Method while lowering a reviewed plan.

use std::collections::{BTreeMap, HashMap};

use lab_capability::{PropertyValue, ScalarValue};
use lab_method::ProcedureValue;
use pliron::attribute::AttrObj;
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{OneRegionInterface, SingleBlockRegionInterface};
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::identifier::Identifier;
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::value::Value;

use crate::lair::dialect::attributes::{quantity_dict, u32_value};
use crate::lair::dialect::chemistry::{ASSEMBLY_CHEMISTRY_KEYS, STRAIN_CHEMISTRY_KEYS};
use crate::lair::dialect::design::{DesignDnaSequenceOp, DesignPlasmidOp, DesignStrainOp};
use crate::lair::dialect::procedure::{ParameterOp, TaskOp};
use crate::lair::dialect::protocol::{
    AssembleOp, AssemblyMethodAttr, DiluteOp, PlateOp, ProvisionOp, RecoverOp, SynthesizeOp,
    TransformOp,
};
use crate::lair::stage::{IrStage, initialize_stage};

const PROCEDURE_NS: &str = "https://www.lab-compiler.org/ns/procedure#";
const SETUP_GOLDEN_GATE: &str = "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction";
const CYCLE_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#ThermalCycleGoldenGateReaction";
const PROVISION_MATERIAL: &str = "https://www.lab-compiler.org/ns/procedure#ProvisionMaterial";
const CHEMICAL_TRANSFORMATION: &str =
    "https://www.lab-compiler.org/ns/procedure#ChemicallyTransformCells";
const RECOVER_CULTURE: &str = "https://www.lab-compiler.org/ns/procedure#RecoverCulture";
const SERIAL_DILUTION: &str = "https://www.lab-compiler.org/ns/procedure#SeriallyDiluteCulture";
const PLATE_SELECTION: &str = "https://www.lab-compiler.org/ns/procedure#PlateCultureForSelection";

type TaskParameters = BTreeMap<String, BTreeMap<String, ProcedureValue>>;

pub(crate) fn project_dependency_build_protocol(
    allocated_context: &Context,
    allocated_module: ModuleOp,
) -> Result<(Context, ModuleOp), String> {
    let allocated_block = allocated_module
        .get_region(allocated_context)
        .deref(allocated_context)
        .get_head()
        .ok_or_else(|| "allocated LAIR has no builtin.module entry block".to_owned())?;
    let operations = allocated_block
        .deref(allocated_context)
        .iter(allocated_context)
        .collect::<Vec<_>>();
    let tasks = operations
        .iter()
        .filter_map(|operation| Operation::get_op::<TaskOp>(*operation, allocated_context))
        .collect::<Vec<_>>();
    let parameters = collect_parameters(allocated_context, &operations)?;

    let mut protocol_context = Context::new();
    let protocol_module = ModuleOp::new(
        &mut protocol_context,
        Identifier::try_from("allocated_dependency_build")
            .expect("static allocated module name is valid"),
    );
    initialize_stage(
        &mut protocol_context,
        protocol_module,
        IrStage::MethodSelectedProtocol,
    );
    let mut translated = HashMap::<Value, Value>::new();

    for operation in &operations {
        if let Some(sequence) =
            Operation::get_op::<DesignDnaSequenceOp>(*operation, allocated_context)
        {
            let selected = DesignDnaSequenceOp::new(
                &mut protocol_context,
                required_string(
                    sequence.get_attr_sequence_name(allocated_context),
                    "design.dna_sequence sequence_name",
                )?,
                required_string(
                    sequence.get_attr_elements(allocated_context),
                    "design.dna_sequence elements",
                )?,
            );
            translated.insert(
                sequence.get_result_sequence(allocated_context),
                selected.get_result_sequence(&protocol_context),
            );
            protocol_module.append_operation(&mut protocol_context, selected.get_operation(), 0);
            continue;
        }
        if let Some(design) = Operation::get_op::<DesignPlasmidOp>(*operation, allocated_context) {
            let sequence = translated_value(
                &translated,
                design.get_operand_sequence(allocated_context),
                "design.plasmid sequence",
            )?;
            let copies_attribute = design
                .get_attr_copies(allocated_context)
                .ok_or_else(|| "design.plasmid is missing copies".to_owned())?;
            let copies = u16::try_from(u32_value(&copies_attribute))
                .map_err(|_| "design.plasmid copies exceed u16".to_owned())?;
            let exact_sequence_attribute = design
                .get_attr_exact_sequence_required(allocated_context)
                .ok_or_else(|| "design.plasmid is missing exact_sequence_required".to_owned())?;
            let exact_sequence_required = bool::from((*exact_sequence_attribute).clone());
            let minimum_concentration = design
                .get_attr_acceptance_minimum_concentration_ng_per_ul(allocated_context)
                .as_deref()
                .map(u32_value);
            let minimum_volume = design
                .get_attr_acceptance_minimum_volume_ul(allocated_context)
                .as_deref()
                .map(u32_value);
            let selected = DesignPlasmidOp::new(
                &mut protocol_context,
                required_string(
                    design.get_attr_artifact_name(allocated_context),
                    "design.plasmid artifact_name",
                )?,
                sequence,
                copies,
                exact_sequence_required,
                minimum_concentration,
                minimum_volume,
            );
            translated.insert(
                design.get_result_design(allocated_context),
                selected.get_result_design(&protocol_context),
            );
            protocol_module.append_operation(&mut protocol_context, selected.get_operation(), 0);
            continue;
        }
        if let Some(design) = Operation::get_op::<DesignStrainOp>(*operation, allocated_context) {
            let selected = DesignStrainOp::new(
                &mut protocol_context,
                required_string(
                    design.get_attr_strain_artifact_name(allocated_context),
                    "design.strain artifact_name",
                )?,
                required_string(
                    design.get_attr_strain_chassis(allocated_context),
                    "design.strain chassis",
                )?,
                required_strings(
                    design.get_attr_strain_plasmids(allocated_context),
                    "design.strain plasmids",
                )?,
                required_string(
                    design.get_attr_strain_selection(allocated_context),
                    "design.strain selection",
                )?,
            );
            translated.insert(
                design.get_result_design(allocated_context),
                selected.get_result_design(&protocol_context),
            );
            protocol_module.append_operation(&mut protocol_context, selected.get_operation(), 0);
            continue;
        }

        let Some(task) = Operation::get_op::<TaskOp>(*operation, allocated_context) else {
            continue;
        };
        let node = task.node_id(allocated_context);
        let semantic_operation = task.semantic_operation(allocated_context);
        match semantic_operation.as_str() {
            SETUP_GOLDEN_GATE => {
                let task_operation = task.get_operation().deref(allocated_context);
                let design = translated_value(
                    &translated,
                    task_operation.get_operand(0),
                    &format!("Procedure task '{node}' design"),
                )?;
                let setup_result = task_operation.get_result(0);
                let cycling =
                    consuming_task(allocated_context, &tasks, setup_result, CYCLE_GOLDEN_GATE)
                        .ok_or_else(|| {
                            format!(
                                "Procedure task '{node}' has no selected thermal-cycling consumer"
                            )
                        })?;
                let cycling_node = cycling.node_id(allocated_context);
                let synthesize = SynthesizeOp::new(&mut protocol_context, design);
                let linear = synthesize.get_result_material(&protocol_context);
                protocol_module.append_operation(
                    &mut protocol_context,
                    synthesize.get_operation(),
                    0,
                );
                let chemistry =
                    assembly_chemistry(&protocol_context, &parameters, &node, &cycling_node)?;
                let selected = AssembleOp::new(
                    &mut protocol_context,
                    linear,
                    AssemblyMethodAttr::GoldenGate,
                    scalar_text(&parameters, &node, "artifact")?,
                    scalar_text(&parameters, &node, "backbone")?,
                    text_list(&parameters, &node, "components")?,
                    text_list(&parameters, &node, "dependencies")?,
                    scalar_text(&parameters, &node, "restriction_enzyme")?,
                    scalar_u8(&parameters, &node, "assembly_replicates")?,
                    chemistry,
                );
                translated.insert(
                    setup_result,
                    selected.get_result_construct(&protocol_context),
                );
                protocol_module.append_operation(
                    &mut protocol_context,
                    selected.get_operation(),
                    0,
                );
            }
            CYCLE_GOLDEN_GATE => {
                let operation = task.get_operation().deref(allocated_context);
                let construct = translated_value(
                    &translated,
                    operation.get_operand(0),
                    &format!("Procedure task '{node}' reaction"),
                )?;
                translated.insert(operation.get_result(0), construct);
            }
            PROVISION_MATERIAL => {
                let selected = ProvisionOp::competent_cells(
                    &mut protocol_context,
                    scalar_text(&parameters, &node, "item")?,
                );
                translated.insert(
                    task.get_operation().deref(allocated_context).get_result(0),
                    selected.get_result_material(&protocol_context),
                );
                protocol_module.append_operation(
                    &mut protocol_context,
                    selected.get_operation(),
                    0,
                );
            }
            CHEMICAL_TRANSFORMATION => {
                let operation = task.get_operation().deref(allocated_context);
                let design = translated_value(
                    &translated,
                    operation.get_operand(0),
                    &format!("Procedure task '{node}' design"),
                )?;
                let cells = translated_value(
                    &translated,
                    operation.get_operand(1),
                    &format!("Procedure task '{node}' cells"),
                )?;
                let chemistry =
                    task_chemistry(&protocol_context, &parameters, &node, STRAIN_CHEMISTRY_KEYS)?;
                let selected = TransformOp::new(
                    &mut protocol_context,
                    design,
                    cells,
                    scalar_text(&parameters, &node, "artifact")?,
                    scalar_text(&parameters, &node, "chassis")?,
                    text_list(&parameters, &node, "plasmids")?,
                    text_list(&parameters, &node, "dependencies")?,
                    scalar_u8(&parameters, &node, "replicates")?,
                    chemistry,
                );
                translated.insert(
                    operation.get_result(0),
                    selected.get_result_strain(&protocol_context),
                );
                translated.insert(
                    operation.get_result(1),
                    selected.get_result_culture(&protocol_context),
                );
                protocol_module.append_operation(
                    &mut protocol_context,
                    selected.get_operation(),
                    0,
                );
            }
            RECOVER_CULTURE => {
                let operation = task.get_operation().deref(allocated_context);
                let culture = translated_value(
                    &translated,
                    operation.get_operand(0),
                    &format!("Procedure task '{node}' culture"),
                )?;
                let (magnitude, unit) = scalar_quantity(&parameters, &node, "duration")?;
                let selected = RecoverOp::new(&mut protocol_context, culture, magnitude, unit);
                translated.insert(
                    operation.get_result(0),
                    selected.get_result_recovered(&protocol_context),
                );
                protocol_module.append_operation(
                    &mut protocol_context,
                    selected.get_operation(),
                    0,
                );
            }
            SERIAL_DILUTION => {
                let operation = task.get_operation().deref(allocated_context);
                let culture = translated_value(
                    &translated,
                    operation.get_operand(0),
                    &format!("Procedure task '{node}' culture"),
                )?;
                let selected = DiluteOp::new(
                    &mut protocol_context,
                    culture,
                    scalar_u8(&parameters, &node, "serial_dilutions")?,
                );
                translated.insert(
                    operation.get_result(0),
                    selected.get_result_diluted(&protocol_context),
                );
                protocol_module.append_operation(
                    &mut protocol_context,
                    selected.get_operation(),
                    0,
                );
            }
            PLATE_SELECTION => {
                let operation = task.get_operation().deref(allocated_context);
                let culture = translated_value(
                    &translated,
                    operation.get_operand(0),
                    &format!("Procedure task '{node}' culture"),
                )?;
                let selected = PlateOp::new(
                    &mut protocol_context,
                    culture,
                    scalar_text(&parameters, &node, "selection")?,
                    scalar_u8(&parameters, &node, "replicates")?,
                );
                translated.insert(
                    operation.get_result(0),
                    selected.get_result_plate(&protocol_context),
                );
                protocol_module.append_operation(
                    &mut protocol_context,
                    selected.get_operation(),
                    0,
                );
            }
            other if other.starts_with(PROCEDURE_NS) => {
                return Err(format!(
                    "selected Procedure task '{node}' uses operation '{other}', which the dependency-build adapter ABI does not support"
                ));
            }
            other => {
                return Err(format!(
                    "selected Procedure task '{node}' uses unknown operation '{other}'"
                ));
            }
        }
    }

    Ok((protocol_context, protocol_module))
}

fn collect_parameters(
    context: &Context,
    operations: &[pliron::context::Ptr<Operation>],
) -> Result<TaskParameters, String> {
    let mut parameters = TaskParameters::new();
    for operation in operations {
        let Some(parameter) = Operation::get_op::<ParameterOp>(*operation, context) else {
            continue;
        };
        let node = parameter.procedure_node(context);
        let full_id = parameter.parameter_id(context);
        let prefix = format!("{node}::parameter::");
        let name = full_id.strip_prefix(&prefix).ok_or_else(|| {
            format!("Procedure parameter '{full_id}' is not namespaced beneath its task '{node}'")
        })?;
        if name.is_empty() || name.contains("::") {
            return Err(format!(
                "Procedure parameter '{full_id}' does not end in one local parameter name"
            ));
        }
        let (_, _, value) = parameter.semantic_parameter(context);
        if parameters
            .entry(node.clone())
            .or_default()
            .insert(name.to_owned(), value)
            .is_some()
        {
            return Err(format!(
                "Procedure task '{node}' repeats parameter '{name}'"
            ));
        }
    }
    Ok(parameters)
}

fn consuming_task<'a>(
    context: &Context,
    tasks: &'a [TaskOp],
    value: Value,
    operation: &str,
) -> Option<&'a TaskOp> {
    tasks.iter().find(|task| {
        task.semantic_operation(context).as_str() == operation
            && task
                .get_operation()
                .deref(context)
                .operands()
                .any(|operand| operand == value)
    })
}

fn assembly_chemistry(
    context: &Context,
    parameters: &TaskParameters,
    setup: &str,
    cycling: &str,
) -> Result<pliron::builtin::attributes::DictAttr, String> {
    let entries = ASSEMBLY_CHEMISTRY_KEYS
        .iter()
        .map(|key| {
            let node = if parameter(parameters, setup, key).is_ok() {
                setup
            } else {
                cycling
            };
            Ok((*key, scalar_u32(parameters, node, key)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(quantity_dict(&entries, context))
}

fn task_chemistry(
    context: &Context,
    parameters: &TaskParameters,
    node: &str,
    keys: &[&'static str],
) -> Result<pliron::builtin::attributes::DictAttr, String> {
    let entries = keys
        .iter()
        .map(|key| Ok((*key, scalar_u32(parameters, node, key)?)))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(quantity_dict(&entries, context))
}

fn parameter<'a>(
    parameters: &'a TaskParameters,
    node: &str,
    name: &str,
) -> Result<&'a ProcedureValue, String> {
    parameters
        .get(node)
        .and_then(|parameters| parameters.get(name))
        .ok_or_else(|| format!("Procedure task '{node}' is missing parameter '{name}'"))
}

fn scalar_property<'a>(
    parameters: &'a TaskParameters,
    node: &str,
    name: &str,
) -> Result<&'a PropertyValue, String> {
    let ProcedureValue::Scalar { value } = parameter(parameters, node, name)? else {
        return Err(format!(
            "Procedure task '{node}' parameter '{name}' must be scalar"
        ));
    };
    Ok(value)
}

fn scalar_text(parameters: &TaskParameters, node: &str, name: &str) -> Result<String, String> {
    let property = scalar_property(parameters, node, name)?;
    let ScalarValue::Text(value) = &property.value else {
        return Err(format!(
            "Procedure task '{node}' parameter '{name}' must be text"
        ));
    };
    if property.unit.is_some() {
        return Err(format!(
            "Procedure task '{node}' text parameter '{name}' cannot have a unit"
        ));
    }
    Ok(value.clone())
}

fn text_list(parameters: &TaskParameters, node: &str, name: &str) -> Result<Vec<String>, String> {
    let ProcedureValue::List {
        element_type: lab_method::ScalarType::Text,
        values,
    } = parameter(parameters, node, name)?
    else {
        return Err(format!(
            "Procedure task '{node}' parameter '{name}' must be a text list"
        ));
    };
    values
        .iter()
        .map(|value| {
            let ScalarValue::Text(value) = &value.value else {
                return Err(format!(
                    "Procedure task '{node}' parameter '{name}' contains a non-text value"
                ));
            };
            if value.is_empty() {
                return Err(format!(
                    "Procedure task '{node}' parameter '{name}' contains an empty value"
                ));
            }
            Ok(value.clone())
        })
        .collect()
}

fn scalar_u32(parameters: &TaskParameters, node: &str, name: &str) -> Result<u32, String> {
    let value = scalar_property(parameters, node, name)?;
    let ScalarValue::Integer(value) = &value.value else {
        return Err(format!(
            "Procedure task '{node}' parameter '{name}' must be an integer"
        ));
    };
    value.as_str().parse::<u32>().map_err(|_| {
        format!("Procedure task '{node}' parameter '{name}' must fit unsigned 32-bit range")
    })
}

fn scalar_u8(parameters: &TaskParameters, node: &str, name: &str) -> Result<u8, String> {
    u8::try_from(scalar_u32(parameters, node, name)?).map_err(|_| {
        format!("Procedure task '{node}' parameter '{name}' must fit unsigned 8-bit range")
    })
}

fn scalar_quantity(
    parameters: &TaskParameters,
    node: &str,
    name: &str,
) -> Result<(String, String), String> {
    let value = scalar_property(parameters, node, name)?;
    let magnitude = match &value.value {
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Real(value) => value.to_string(),
        _ => {
            return Err(format!(
                "Procedure task '{node}' parameter '{name}' must be numeric"
            ));
        }
    };
    let unit = value
        .unit
        .as_ref()
        .ok_or_else(|| format!("Procedure task '{node}' parameter '{name}' requires a unit"))?;
    let unit = match unit.as_str() {
        "http://qudt.org/vocab/unit/HR" => "h".to_owned(),
        "http://qudt.org/vocab/unit/MIN" => "min".to_owned(),
        other => other.to_owned(),
    };
    Ok((magnitude, unit))
}

fn translated_value(
    translated: &HashMap<Value, Value>,
    value: Value,
    owner: &str,
) -> Result<Value, String> {
    translated
        .get(&value)
        .copied()
        .ok_or_else(|| format!("{owner} has no value in the allocated adapter projection"))
}

fn required_string(
    value: Option<std::cell::Ref<'_, StringAttr>>,
    name: &str,
) -> Result<String, String> {
    value
        .map(|value| value.as_str().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is missing"))
}

fn required_strings(
    value: Option<std::cell::Ref<'_, VecAttr>>,
    name: &str,
) -> Result<Vec<String>, String> {
    value
        .ok_or_else(|| format!("{name} is missing"))?
        .0
        .iter()
        .map(|value: &AttrObj| {
            value
                .downcast_ref::<StringAttr>()
                .map(|value| value.as_str().to_owned())
                .ok_or_else(|| format!("{name} contains a non-string value"))
        })
        .collect()
}
