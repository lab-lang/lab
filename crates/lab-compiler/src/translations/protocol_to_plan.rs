use std::cell::Ref;

use crate::{AcceptanceCriterion, Concentration, Volume};
use crate::{AcceptanceObligation, ExecutablePlan, OperationKind, PlanStep, PlanValue, ValueKind};
use pliron::builtin::attributes::{IntegerAttr, StringAttr};
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::common_traits::Named;
use pliron::context::{Context, Ptr};
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;
use pliron::r#type::Typed;
use pliron::value::Value;

use crate::ir::attributes::u32_value;
use crate::ir::design::{DesignPlasmidOp, DesignType};
use crate::ir::protocol::{
    AcceptOp, AssembleOp, EvidenceType, GrowOp, MaterialType, ProvisionOp, PurifyOp, QuantifyOp,
    RecoverOp, SampleOp, ScreenOp, SelectOp, SequenceOp, SynthesizeOp, TransformOp,
};

use crate::pipeline::CompilerError;

pub(crate) fn lower_protocol_to_plan(
    context: &Context,
    module: ModuleOp,
    lab_profile: &str,
) -> Result<ExecutablePlan, CompilerError> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| invalid_ir("Protocol module has no entry block"))?;

    let mut artifact = None;
    let mut initial_values = Vec::new();
    let mut steps = Vec::new();
    let mut acceptance = Vec::new();

    for operation in block.deref(context).iter(context) {
        if let Some(design) = Operation::get_op::<DesignPlasmidOp>(operation, context) {
            let name = required_attribute(
                design.get_attr_artifact_name(context),
                "design.plasmid",
                "artifact_name",
            )?;
            initial_values.push(PlanValue::design(value_name(
                &name,
                design.get_result_design(context),
                context,
            )?));
            artifact = Some(name);
            continue;
        }

        let artifact_name = artifact
            .as_deref()
            .ok_or_else(|| invalid_ir("a Protocol operation appears before design.plasmid"))?;

        if let Some(op) = Operation::get_op::<ProvisionOp>(operation, context) {
            let item = required_attribute(op.get_attr_item(context), "protocol.provision", "item")?;
            steps.push(
                plan_step(
                    "provision_cells",
                    OperationKind::Provision,
                    operation,
                    artifact_name,
                    context,
                )?
                .with_parameter("inventory item", format!("{item} competent cells")),
            );
        } else if Operation::get_op::<SynthesizeOp>(operation, context).is_some() {
            steps.push(plan_step(
                "synthesize",
                OperationKind::Synthesize,
                operation,
                artifact_name,
                context,
            )?);
        } else if let Some(op) = Operation::get_op::<AssembleOp>(operation, context) {
            let method = op
                .get_attr_assembly_method(context)
                .ok_or_else(|| invalid_ir("protocol.assemble has no assembly_method attribute"))?
                .as_str()
                .to_owned();
            steps.push(
                plan_step(
                    "assemble",
                    OperationKind::Assemble,
                    operation,
                    artifact_name,
                    context,
                )?
                .with_parameter("method", method),
            );
        } else if let Some(op) = Operation::get_op::<TransformOp>(operation, context) {
            let host = required_attribute(op.get_attr_host(context), "protocol.transform", "host")?;
            steps.push(
                plan_step(
                    "transform",
                    OperationKind::Transform,
                    operation,
                    artifact_name,
                    context,
                )?
                .with_parameter("host", host),
            );
        } else if Operation::get_op::<RecoverOp>(operation, context).is_some() {
            steps.push(plan_step(
                "recover",
                OperationKind::Recover,
                operation,
                artifact_name,
                context,
            )?);
        } else if Operation::get_op::<SelectOp>(operation, context).is_some() {
            steps.push(plan_step(
                "select",
                OperationKind::Select,
                operation,
                artifact_name,
                context,
            )?);
        } else if let Some(op) = Operation::get_op::<ScreenOp>(operation, context) {
            let method = required_attribute(
                op.get_attr_screening_method(context),
                "protocol.screen",
                "screening_method",
            )?;
            steps.push(
                plan_step(
                    "screen",
                    OperationKind::Screen,
                    operation,
                    artifact_name,
                    context,
                )?
                .with_parameter("method", method),
            );
        } else if Operation::get_op::<GrowOp>(operation, context).is_some() {
            steps.push(plan_step(
                "grow",
                OperationKind::Grow,
                operation,
                artifact_name,
                context,
            )?);
        } else if Operation::get_op::<PurifyOp>(operation, context).is_some() {
            steps.push(plan_step(
                "purify",
                OperationKind::Purify,
                operation,
                artifact_name,
                context,
            )?);
        } else if let Some(op) = Operation::get_op::<SampleOp>(operation, context) {
            let purpose =
                required_attribute(op.get_attr_purpose(context), "protocol.sample", "purpose")?;
            steps.push(
                plan_step(
                    "sample_sequence",
                    OperationKind::Sample,
                    operation,
                    artifact_name,
                    context,
                )?
                .with_parameter("purpose", purpose),
            );
        } else if let Some(op) = Operation::get_op::<SequenceOp>(operation, context) {
            let evidence_value =
                value_name(artifact_name, op.get_result_evidence(context), context)?;
            steps.push(plan_step(
                "sequence",
                OperationKind::Sequence,
                operation,
                artifact_name,
                context,
            )?);
            acceptance.push(AcceptanceObligation {
                criterion: AcceptanceCriterion::ExactSequence,
                evidence_step: "sequence".into(),
                evidence_value,
            });
        } else if let Some(op) = Operation::get_op::<QuantifyOp>(operation, context) {
            steps.push(plan_step(
                "quantify",
                OperationKind::Quantify,
                operation,
                artifact_name,
                context,
            )?);

            if let Some(attribute) = op.get_attr_minimum_concentration_ng_per_ul(context) {
                let threshold = integer_threshold(&attribute);
                acceptance.push(AcceptanceObligation {
                    criterion: AcceptanceCriterion::MinimumConcentration {
                        concentration: Concentration::nanograms_per_microliter(threshold),
                    },
                    evidence_step: "quantify".into(),
                    evidence_value: value_name(
                        artifact_name,
                        op.get_result_concentration(context),
                        context,
                    )?,
                });
            }
            if let Some(attribute) = op.get_attr_minimum_volume_ul(context) {
                let threshold = integer_threshold(&attribute);
                acceptance.push(AcceptanceObligation {
                    criterion: AcceptanceCriterion::MinimumVolume {
                        volume: Volume::microliters(threshold),
                    },
                    evidence_step: "quantify".into(),
                    evidence_value: value_name(
                        artifact_name,
                        op.get_result_volume(context),
                        context,
                    )?,
                });
            }
        } else if Operation::get_op::<AcceptOp>(operation, context).is_some() {
            steps.push(plan_step(
                "accept",
                OperationKind::Accept,
                operation,
                artifact_name,
                context,
            )?);
        } else {
            return Err(invalid_ir(format!(
                "unsupported operation '{}' while lowering Protocol IR to a plan",
                Operation::get_opid(operation, context)
            )));
        }
    }

    let artifact = artifact.ok_or_else(|| invalid_ir("Protocol module has no design.plasmid"))?;
    Ok(ExecutablePlan {
        artifact,
        lab_profile: lab_profile.into(),
        initial_values,
        steps,
        acceptance,
    })
}

fn plan_step(
    id: &str,
    kind: OperationKind,
    operation: Ptr<Operation>,
    artifact: &str,
    context: &Context,
) -> Result<PlanStep, CompilerError> {
    let operation_ref = operation.deref(context);
    let inputs = operation_ref
        .operands()
        .map(|value| value_name(artifact, value, context))
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = operation_ref
        .results()
        .map(|value| plan_value(artifact, value, context))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlanStep::new(id, kind, inputs, outputs))
}

fn plan_value(artifact: &str, value: Value, context: &Context) -> Result<PlanValue, CompilerError> {
    let handle = value.get_type(context);
    let ty = handle.deref(context);
    let kind = if ty.downcast_ref::<DesignType>().is_some() {
        ValueKind::Design
    } else if ty.downcast_ref::<MaterialType>().is_some() {
        ValueKind::Material
    } else if ty.downcast_ref::<EvidenceType>().is_some() {
        ValueKind::Evidence
    } else {
        return Err(invalid_ir(
            "Protocol operation produced an unknown value type",
        ));
    };
    Ok(PlanValue::new(value_name(artifact, value, context)?, kind))
}

fn value_name(artifact: &str, value: Value, context: &Context) -> Result<String, CompilerError> {
    let name = value
        .given_name(context)
        .ok_or_else(|| invalid_ir("Protocol value has no semantic name"))?;
    Ok(format!("{artifact}.{name}"))
}

fn required_attribute(
    attribute: Option<Ref<'_, StringAttr>>,
    operation: &str,
    name: &str,
) -> Result<String, CompilerError> {
    attribute
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| invalid_ir(format!("{operation} has no {name} attribute")))
}

fn integer_threshold(attribute: &IntegerAttr) -> u32 {
    u32_value(attribute)
}

fn invalid_ir(message: impl Into<String>) -> CompilerError {
    CompilerError::InvalidIr(message.into())
}

#[cfg(test)]
mod tests {
    use crate::{
        AcceptanceCriterion, ArtifactSpec, AssemblyMethod, DnaSequence, LabProfile, PlasmidSpec,
        Topology,
    };

    use crate::ir::protocol::AssemblyMethodAttr;
    use crate::passes::lower_design_to_protocol;
    use crate::translations::lower_specification_to_design;

    use super::*;

    #[test]
    fn plan_is_lowered_from_protocol_attributes() {
        let specification = ArtifactSpec::plasmid(
            "p_ir_source",
            PlasmidSpec::new(DnaSequence::new("ACGT").unwrap(), Topology::Circular).unwrap(),
            1,
            vec![AcceptanceCriterion::ExactSequence],
        )
        .unwrap();
        let lab = LabProfile::reference();
        let mut context = Context::new();
        let module = lower_specification_to_design(&mut context, &specification).unwrap();
        lower_design_to_protocol(&mut context, module, &lab, AssemblyMethod::Gibson).unwrap();
        let block = module
            .get_region(&context)
            .deref(&context)
            .get_head()
            .unwrap();
        let assemble = block
            .deref(&context)
            .iter(&context)
            .find_map(|operation| Operation::get_op::<AssembleOp>(operation, &context))
            .unwrap();
        assemble.set_attr_assembly_method(&context, AssemblyMethodAttr::GoldenGate);

        let plan = lower_protocol_to_plan(&context, module, lab.name()).unwrap();
        let assemble_step = plan
            .steps
            .iter()
            .find(|step| step.id == "assemble")
            .unwrap();

        assert_eq!(
            assemble_step.parameters.get("method").map(String::as_str),
            Some("Golden Gate")
        );
    }
}
