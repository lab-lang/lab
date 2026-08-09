//! Provenance analysis over verifier-valid Protocol LAIR.

use std::collections::{BTreeSet, HashMap};

use pliron::attribute::AttrObj;
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;

use crate::ProtocolLairProgram;
use crate::backend::error::PlanningError;
use crate::lair::dialect::attributes::{quantity_entry, u32_value};
use crate::lair::dialect::design::{DesignPlasmidOp, DesignStrainOp};
use crate::lair::dialect::protocol::{
    AssembleOp, DiluteOp, PlateOp, ProvisionOp, RecoverOp, SynthesizeOp, TransformOp,
};

/// Provenance traces through typed Protocol operations, grouped by the artifact
/// each one realizes. These store operation handles rather than a copied recipe,
/// so every planning decision is read from the verifier-valid Protocol module.
pub(in crate::backend) struct ProtocolTraces {
    pub(in crate::backend) assemblies: Vec<AssemblyTrace>,
    pub(in crate::backend) strains: Vec<StrainTrace>,
}

impl ProtocolTraces {
    pub(in crate::backend) fn is_empty(&self) -> bool {
        self.assemblies.is_empty() && self.strains.is_empty()
    }
}

/// A plasmid artifact and the assembly that produces it.
pub(in crate::backend) struct AssemblyTrace {
    design: DesignPlasmidOp,
    assemble: AssembleOp,
}

impl AssemblyTrace {
    pub(in crate::backend) fn artifact(&self, context: &Context) -> String {
        required_string(self.assemble.get_attr_assembly_artifact(context).as_deref())
    }

    pub(in crate::backend) fn sequence(&self, context: &Context) -> String {
        required_string(self.design.get_attr_sequence(context).as_deref())
    }

    pub(in crate::backend) fn backbone(&self, context: &Context) -> String {
        required_string(self.assemble.get_attr_assembly_backbone(context).as_deref())
    }

    pub(in crate::backend) fn components(&self, context: &Context) -> Vec<String> {
        required_strings(
            self.assemble
                .get_attr_assembly_components(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn dependencies(&self, context: &Context) -> Vec<String> {
        required_strings(
            self.assemble
                .get_attr_assembly_dependencies(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn restriction_enzyme(&self, context: &Context) -> String {
        required_string(
            self.assemble
                .get_attr_assembly_restriction_enzyme(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn assembly_replicates(&self, context: &Context) -> u8 {
        required_count(
            self.assemble
                .get_attr_assembly_replicates(context)
                .as_deref(),
        )
    }

    /// One reaction parameter, read from the verified chemistry dictionary.
    pub(in crate::backend) fn chemistry(&self, context: &Context, key: &str) -> u16 {
        chemistry_entry(
            self.assemble
                .get_attr_assembly_chemistry(context)
                .as_deref(),
            key,
        )
    }
}

/// A strain artifact and the transformation, recovery, dilution, and plating
/// that produce it.
pub(in crate::backend) struct StrainTrace {
    design: DesignStrainOp,
    transform: TransformOp,
    recover: Option<RecoverOp>,
    dilute: Option<DiluteOp>,
    plate: Option<PlateOp>,
}

impl StrainTrace {
    fn incomplete(design: DesignStrainOp, transform: TransformOp) -> Self {
        Self {
            design,
            transform,
            recover: None,
            dilute: None,
            plate: None,
        }
    }

    fn require_complete(&self, context: &Context) -> Result<(), PlanningError> {
        if self.recover.is_none() || self.dilute.is_none() || self.plate.is_none() {
            return Err(PlanningError::InvalidProtocol(format!(
                "artifact '{}' does not contain the complete transform -> recover -> dilute -> plate material chain",
                self.artifact(context)
            )));
        }
        Ok(())
    }

    pub(in crate::backend) fn artifact(&self, context: &Context) -> String {
        required_string(
            self.transform
                .get_attr_transformation_artifact(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn host(&self, context: &Context) -> String {
        required_string(self.transform.get_attr_host(context).as_deref())
    }

    /// Plasmid designs the strain carries.
    pub(in crate::backend) fn plasmids(&self, context: &Context) -> Vec<String> {
        required_strings(self.design.get_attr_strain_plasmids(context).as_deref())
    }

    /// Artifacts whose materials the transformation consumes.
    pub(in crate::backend) fn dependencies(&self, context: &Context) -> Vec<String> {
        required_strings(
            self.transform
                .get_attr_transformation_dependencies(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn selection(&self, context: &Context) -> String {
        required_string(
            self.plate
                .as_ref()
                .expect("complete Protocol trace has plating")
                .get_attr_plating_selection(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn transformation_replicates(&self, context: &Context) -> u8 {
        required_count(
            self.transform
                .get_attr_transformation_replicates(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn plating_replicates(&self, context: &Context) -> u8 {
        required_count(
            self.plate
                .as_ref()
                .expect("complete Protocol trace has plating")
                .get_attr_plating_replicates(context)
                .as_deref(),
        )
    }

    pub(in crate::backend) fn serial_dilutions(&self, context: &Context) -> u8 {
        required_count(
            self.dilute
                .as_ref()
                .expect("complete Protocol trace has dilution")
                .get_attr_serial_dilutions(context)
                .as_deref(),
        )
    }

    /// One reaction parameter, read from the verified chemistry dictionary.
    pub(in crate::backend) fn chemistry(&self, context: &Context, key: &str) -> u16 {
        chemistry_entry(
            self.transform
                .get_attr_transformation_chemistry(context)
                .as_deref(),
            key,
        )
    }
}

fn chemistry_entry(dict: Option<&pliron::builtin::attributes::DictAttr>, key: &str) -> u16 {
    u16::try_from(quantity_entry(dict, key, 0))
        .expect("chemistry originated from checked u16 source data")
}

pub(in crate::backend) fn analyze_protocol(
    program: &ProtocolLairProgram,
    selected_artifacts: Option<&BTreeSet<String>>,
) -> Result<ProtocolTraces, PlanningError> {
    let context = program.context();
    let block = program
        .module()
        .get_region(context)
        .deref(context)
        .get_head()
        .expect("verified builtin.module has an entry block");
    let mut assemblies = Vec::<AssemblyTrace>::new();
    let mut strains = Vec::<StrainTrace>::new();
    let mut transformed_to_strain = HashMap::new();
    let mut recovered_to_strain = HashMap::new();
    let mut diluted_to_strain = HashMap::new();

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
                    PlanningError::InvalidProtocol(format!(
                        "assembly for artifact '{artifact}' is not fed by protocol.synthesize"
                    ))
                })?;
            let design = synthesize
                .get_operand_design(context)
                .defining_op()
                .and_then(|defining| Operation::get_op::<DesignPlasmidOp>(defining, context))
                .ok_or_else(|| {
                    PlanningError::InvalidProtocol(format!(
                        "assembly for artifact '{artifact}' cannot be traced to design.plasmid"
                    ))
                })?;
            let design_name = required_string(design.get_attr_artifact_name(context).as_deref());
            if design_name != artifact {
                return Err(PlanningError::InvalidProtocol(format!(
                    "protocol artifact '{artifact}' consumes design '{design_name}'"
                )));
            }
            assemblies.push(AssemblyTrace { design, assemble });
            continue;
        }
        if let Some(transform) = Operation::get_op::<TransformOp>(operation, context) {
            let artifact = required_string(
                transform
                    .get_attr_transformation_artifact(context)
                    .as_deref(),
            );
            if selected_artifacts.is_some_and(|selected| !selected.contains(&artifact)) {
                continue;
            }
            let design = transform
                .get_operand_design(context)
                .defining_op()
                .and_then(|defining| Operation::get_op::<DesignStrainOp>(defining, context))
                .ok_or_else(|| {
                    PlanningError::InvalidProtocol(format!(
                        "transformation for artifact '{artifact}' cannot be traced to design.strain"
                    ))
                })?;
            let design_name =
                required_string(design.get_attr_strain_artifact_name(context).as_deref());
            if design_name != artifact {
                return Err(PlanningError::InvalidProtocol(format!(
                    "protocol artifact '{artifact}' consumes design '{design_name}'"
                )));
            }
            let provision = transform
                .get_operand_cells(context)
                .defining_op()
                .and_then(|defining| Operation::get_op::<ProvisionOp>(defining, context))
                .ok_or_else(|| {
                    PlanningError::InvalidProtocol(format!(
                        "transformation for artifact '{artifact}' does not consume provisioned cells"
                    ))
                })?;
            let provisioned_host = required_string(provision.get_attr_item(context).as_deref());
            let selected_host = required_string(transform.get_attr_host(context).as_deref());
            if provisioned_host != selected_host {
                return Err(PlanningError::InvalidProtocol(format!(
                    "transformation for artifact '{artifact}' selects host '{selected_host}' but consumes '{provisioned_host}'"
                )));
            }
            let index = strains.len();
            strains.push(StrainTrace::incomplete(design, transform));
            transformed_to_strain.insert(transform.get_result_culture(context), index);
            continue;
        }
        if let Some(recover) = Operation::get_op::<RecoverOp>(operation, context) {
            let Some(index) = transformed_to_strain
                .get(&recover.get_operand_culture(context))
                .copied()
            else {
                continue;
            };
            strains[index].recover = Some(recover);
            recovered_to_strain.insert(recover.get_result_recovered(context), index);
            continue;
        }
        if let Some(dilute) = Operation::get_op::<DiluteOp>(operation, context) {
            let Some(index) = recovered_to_strain
                .get(&dilute.get_operand_culture(context))
                .copied()
            else {
                continue;
            };
            strains[index].dilute = Some(dilute);
            diluted_to_strain.insert(dilute.get_result_diluted(context), index);
            continue;
        }
        if let Some(plate) = Operation::get_op::<PlateOp>(operation, context) {
            let Some(index) = diluted_to_strain
                .get(&plate.get_operand_culture(context))
                .copied()
            else {
                continue;
            };
            strains[index].plate = Some(plate);
        }
    }

    let traces = ProtocolTraces {
        assemblies,
        strains,
    };
    if traces.is_empty() {
        return Err(PlanningError::InvalidProtocol(
            "the selected Protocol module contains no build artifacts".into(),
        ));
    }
    for trace in &traces.strains {
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
