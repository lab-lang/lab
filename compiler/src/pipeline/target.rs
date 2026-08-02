use crate::{AssemblyMethod, Capability, LabProfile};
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;

use crate::ir::attributes::u32_value;
use crate::ir::design::DesignPlasmidOp;

use super::CompilerError;

pub(crate) fn resolve_target(
    context: &Context,
    module: ModuleOp,
    lab: &LabProfile,
) -> Result<AssemblyMethod, CompilerError> {
    if lab.preferred_host().is_empty() {
        return Err(CompilerError::MissingPreferredHost);
    }

    let design = plasmid_design(context, module)?;
    let copies = design
        .get_attr_copies(context)
        .expect("verified design.plasmid has copies");
    let copies = u32_value(&copies);
    if copies != 1 {
        return Err(CompilerError::UnsupportedCopyCount(copies));
    }

    let exact_sequence_required: bool = design
        .get_attr_exact_sequence_required(context)
        .expect("verified design.plasmid has exact_sequence_required")
        .clone()
        .into();
    if !exact_sequence_required {
        return Err(CompilerError::UnsupportedDesign(
            "the initial plasmid pipeline requires exact-sequence acceptance".into(),
        ));
    }

    let assembly = lab
        .assembly_preference()
        .iter()
        .copied()
        .find(|method| lab.supports(method.required_capability()))
        .ok_or(CompilerError::NoAssemblyMethod)?;

    let mut required = vec![
        Capability::DnaSynthesis,
        Capability::ChemicalTransformation,
        Capability::CultureIncubation,
        Capability::AntibioticSelection,
        Capability::CloneScreening,
        Capability::PlasmidPurification,
        Capability::SangerSequencing,
    ];
    if design
        .get_attr_acceptance_minimum_concentration_ng_per_ul(context)
        .is_some()
        || design
            .get_attr_acceptance_minimum_volume_ul(context)
            .is_some()
    {
        required.push(Capability::DnaQuantification);
    }

    let missing = required
        .into_iter()
        .filter(|capability| !lab.supports(*capability))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(assembly)
    } else {
        Err(CompilerError::MissingCapabilities(missing))
    }
}

fn plasmid_design(context: &Context, module: ModuleOp) -> Result<DesignPlasmidOp, CompilerError> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| CompilerError::InvalidIr("Design IR module has no entry block".into()))?;
    let operation = block
        .deref(context)
        .get_head()
        .ok_or_else(|| CompilerError::InvalidIr("Design IR module is empty".into()))?;
    Operation::get_op::<DesignPlasmidOp>(operation, context).ok_or_else(|| {
        CompilerError::InvalidIr("Design IR module does not begin with design.plasmid".into())
    })
}
