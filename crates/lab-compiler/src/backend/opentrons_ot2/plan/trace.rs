//! Provenance analysis over verifier-valid Protocol LAIR.

use std::collections::{BTreeSet, HashMap};

use pliron::attribute::AttrObj;
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;

use super::Ot2PlanningError;
use crate::ProtocolLairProgram;
use crate::lair::dialect::attributes::u32_value;
use crate::lair::dialect::design::DesignPlasmidOp;
use crate::lair::dialect::protocol::{
    AssembleOp, DiluteOp, PlateOp, ProvisionOp, RecoverOp, SynthesizeOp, TransformOp,
};

/// A provenance trace through typed Protocol operations. This stores operation
/// handles, not a copied recipe representation, so every planning decision is
/// read from the verifier-valid Protocol module.
pub(super) struct ProtocolTrace {
    design: DesignPlasmidOp,
    assemble: AssembleOp,
    transform: Option<TransformOp>,
    recover: Option<RecoverOp>,
    dilute: Option<DiluteOp>,
    plate: Option<PlateOp>,
}

impl ProtocolTrace {
    fn incomplete(design: DesignPlasmidOp, assemble: AssembleOp) -> Self {
        Self {
            design,
            assemble,
            transform: None,
            recover: None,
            dilute: None,
            plate: None,
        }
    }

    fn require_complete(&self, context: &pliron::context::Context) -> Result<(), Ot2PlanningError> {
        if self.transform.is_none()
            || self.recover.is_none()
            || self.dilute.is_none()
            || self.plate.is_none()
        {
            return Err(Ot2PlanningError::InvalidProtocol(format!(
                "artifact '{}' does not contain the complete assemble -> transform -> recover -> dilute -> plate material chain",
                self.artifact(context)
            )));
        }
        Ok(())
    }

    pub(super) fn artifact(&self, context: &pliron::context::Context) -> String {
        required_string(self.assemble.get_attr_assembly_artifact(context).as_deref())
    }

    pub(super) fn sequence(&self, context: &pliron::context::Context) -> String {
        required_string(self.design.get_attr_sequence(context).as_deref())
    }

    pub(super) fn backbone(&self, context: &pliron::context::Context) -> String {
        required_string(self.assemble.get_attr_assembly_backbone(context).as_deref())
    }

    pub(super) fn components(&self, context: &pliron::context::Context) -> Vec<String> {
        required_strings(
            self.assemble
                .get_attr_assembly_components(context)
                .as_deref(),
        )
    }

    pub(super) fn dependencies(&self, context: &pliron::context::Context) -> Vec<String> {
        required_strings(
            self.assemble
                .get_attr_assembly_dependencies(context)
                .as_deref(),
        )
    }

    pub(super) fn restriction_enzyme(&self, context: &pliron::context::Context) -> String {
        required_string(
            self.assemble
                .get_attr_assembly_restriction_enzyme(context)
                .as_deref(),
        )
    }

    pub(super) fn host(&self, context: &pliron::context::Context) -> String {
        required_string(
            self.transform
                .as_ref()
                .expect("complete Protocol trace has transformation")
                .get_attr_host(context)
                .as_deref(),
        )
    }

    pub(super) fn selection(&self, context: &pliron::context::Context) -> String {
        required_string(
            self.plate
                .as_ref()
                .expect("complete Protocol trace has plating")
                .get_attr_plating_selection(context)
                .as_deref(),
        )
    }

    pub(super) fn assembly_replicates(&self, context: &pliron::context::Context) -> u8 {
        required_count(
            self.assemble
                .get_attr_assembly_replicates(context)
                .as_deref(),
        )
    }

    pub(super) fn transformation_replicates(&self, context: &pliron::context::Context) -> u8 {
        required_count(
            self.transform
                .as_ref()
                .expect("complete Protocol trace has transformation")
                .get_attr_transformation_replicates(context)
                .as_deref(),
        )
    }

    pub(super) fn plating_replicates(&self, context: &pliron::context::Context) -> u8 {
        required_count(
            self.plate
                .as_ref()
                .expect("complete Protocol trace has plating")
                .get_attr_plating_replicates(context)
                .as_deref(),
        )
    }

    pub(super) fn serial_dilutions(&self, context: &pliron::context::Context) -> u8 {
        required_count(
            self.dilute
                .as_ref()
                .expect("complete Protocol trace has dilution")
                .get_attr_serial_dilutions(context)
                .as_deref(),
        )
    }
}

pub(super) fn analyze_protocol(
    program: &ProtocolLairProgram,
    selected_artifacts: Option<&BTreeSet<String>>,
) -> Result<Vec<ProtocolTrace>, Ot2PlanningError> {
    let context = program.context();
    let block = program
        .module()
        .get_region(context)
        .deref(context)
        .get_head()
        .expect("verified builtin.module has an entry block");
    let mut traces = Vec::<ProtocolTrace>::new();
    let mut circular_to_construct = HashMap::new();
    let mut transformed_to_construct = HashMap::new();
    let mut recovered_to_construct = HashMap::new();
    let mut diluted_to_construct = HashMap::new();

    for operation in block.deref(context).iter(context) {
        if let Some(assemble) = Operation::get_op::<AssembleOp>(operation, context) {
            let artifact = required_string(assemble.get_attr_assembly_artifact(context).as_deref());
            if selected_artifacts.is_some_and(|selected| !selected.contains(&artifact)) {
                continue;
            }
            let synthesize = assemble
                .get_operand_input(context)
                .defining_op()
                .and_then(|defining| Operation::get_op::<SynthesizeOp>(defining, context))
                .ok_or_else(|| {
                    Ot2PlanningError::InvalidProtocol(format!(
                        "assembly for artifact '{artifact}' is not fed by protocol.synthesize"
                    ))
                })?;
            let design = synthesize
                .get_operand_design(context)
                .defining_op()
                .and_then(|defining| Operation::get_op::<DesignPlasmidOp>(defining, context))
                .ok_or_else(|| {
                    Ot2PlanningError::InvalidProtocol(format!(
                        "assembly for artifact '{artifact}' cannot be traced to design.plasmid"
                    ))
                })?;
            let design_name = required_string(design.get_attr_artifact_name(context).as_deref());
            if design_name != artifact {
                return Err(Ot2PlanningError::InvalidProtocol(format!(
                    "protocol artifact '{artifact}' consumes design '{design_name}'"
                )));
            }
            let index = traces.len();
            traces.push(ProtocolTrace::incomplete(design, assemble));
            circular_to_construct.insert(assemble.get_result_construct(context), index);
            continue;
        }
        if let Some(transform) = Operation::get_op::<TransformOp>(operation, context) {
            let Some(index) = circular_to_construct
                .get(&transform.get_operand_construct(context))
                .copied()
            else {
                continue;
            };
            let provision = transform
                .get_operand_cells(context)
                .defining_op()
                .and_then(|defining| Operation::get_op::<ProvisionOp>(defining, context))
                .ok_or_else(|| {
                    Ot2PlanningError::InvalidProtocol(format!(
                        "transformation for artifact '{}' does not consume provisioned cells",
                        traces[index].artifact(context)
                    ))
                })?;
            let provisioned_host = required_string(provision.get_attr_item(context).as_deref());
            let selected_host = required_string(transform.get_attr_host(context).as_deref());
            if provisioned_host != selected_host {
                return Err(Ot2PlanningError::InvalidProtocol(format!(
                    "transformation for artifact '{}' selects host '{selected_host}' but consumes '{provisioned_host}'",
                    traces[index].artifact(context)
                )));
            }
            traces[index].transform = Some(transform);
            transformed_to_construct.insert(transform.get_result_culture(context), index);
            continue;
        }
        if let Some(recover) = Operation::get_op::<RecoverOp>(operation, context) {
            let Some(index) = transformed_to_construct
                .get(&recover.get_operand_culture(context))
                .copied()
            else {
                continue;
            };
            traces[index].recover = Some(recover);
            recovered_to_construct.insert(recover.get_result_recovered(context), index);
            continue;
        }
        if let Some(dilute) = Operation::get_op::<DiluteOp>(operation, context) {
            let Some(index) = recovered_to_construct
                .get(&dilute.get_operand_culture(context))
                .copied()
            else {
                continue;
            };
            traces[index].dilute = Some(dilute);
            diluted_to_construct.insert(dilute.get_result_diluted(context), index);
            continue;
        }
        if let Some(plate) = Operation::get_op::<PlateOp>(operation, context) {
            let Some(index) = diluted_to_construct
                .get(&plate.get_operand_culture(context))
                .copied()
            else {
                continue;
            };
            traces[index].plate = Some(plate);
        }
    }

    if traces.is_empty() {
        return Err(Ot2PlanningError::InvalidProtocol(
            "the selected Protocol module contains no build artifacts".into(),
        ));
    }
    for trace in &traces {
        trace.require_complete(context)?;
    }
    Ok(traces)
}

fn required_string(attribute: Option<&StringAttr>) -> String {
    attribute
        .expect("verified Protocol operation has required string attribute")
        .as_str()
        .to_owned()
}

fn required_strings(attribute: Option<&VecAttr>) -> Vec<String> {
    attribute
        .expect("verified Protocol operation has required vector attribute")
        .0
        .iter()
        .map(|value: &AttrObj| {
            value
                .downcast_ref::<StringAttr>()
                .expect("verified Protocol string vector contains strings")
                .as_str()
                .to_owned()
        })
        .collect()
}

fn required_count(attribute: Option<&pliron::builtin::attributes::IntegerAttr>) -> u8 {
    u8::try_from(u32_value(
        attribute.expect("verified Protocol operation has required count attribute"),
    ))
    .expect("Protocol count originated from checked u8 source data")
}
