use crate::{AcceptanceCriterion, Artifact, ArtifactSpec};
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::identifier::Identifier;
use pliron::irbuild::inserter::{IRInserter, Inserter};
use pliron::irbuild::listener::DummyListener;
use pliron::linked_list::ContainsLinkedList;
use pliron::value::Value;

use crate::ir::design::DesignPlasmidOp;

use crate::pipeline::CompilerError;

pub(crate) fn lower_specification_to_design(
    context: &mut Context,
    specification: &ArtifactSpec,
) -> Result<ModuleOp, CompilerError> {
    let module_name = identifier(specification.name())?;
    let module = ModuleOp::new(context, module_name);
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .expect("ModuleOp always creates one block");
    let mut inserter = IRInserter::<DummyListener>::new_at_block_end(block);

    let Artifact::Plasmid(plasmid) = specification.artifact();
    let exact_sequence_required = specification
        .acceptance()
        .contains(&AcceptanceCriterion::ExactSequence);
    let minimum_concentration =
        specification
            .acceptance()
            .iter()
            .find_map(|criterion| match criterion {
                AcceptanceCriterion::MinimumConcentration { concentration } => {
                    Some(concentration.as_nanograms_per_microliter())
                }
                _ => None,
            });
    let minimum_volume = specification
        .acceptance()
        .iter()
        .find_map(|criterion| match criterion {
            AcceptanceCriterion::MinimumVolume { volume } => Some(volume.as_microliters()),
            _ => None,
        });

    let design = DesignPlasmidOp::new(
        context,
        specification.name(),
        plasmid.sequence().as_str(),
        specification.copies().get(),
        exact_sequence_required,
        minimum_concentration,
        minimum_volume,
    );
    name_value(context, design.get_result_design(context), "design")?;
    inserter.append_op(context, &design);

    Ok(module)
}

fn identifier(value: &str) -> Result<Identifier, CompilerError> {
    Identifier::try_from(value).map_err(|_| CompilerError::InvalidIdentifier(value.into()))
}

fn name_value(ctx: &Context, value: Value, name: &str) -> Result<(), CompilerError> {
    value.set_name(ctx, Some(identifier(name)?));
    Ok(())
}
