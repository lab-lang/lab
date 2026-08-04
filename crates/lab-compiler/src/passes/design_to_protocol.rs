use crate::{AssemblyMethod, LabProfile};
use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::identifier::Identifier;
use pliron::irbuild::inserter::{IRInserter, Inserter};
use pliron::irbuild::listener::DummyListener;
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;
use pliron::pass::{AnalysisManager, Pass, PassResult};
use pliron::result::Result as PlironResult;
use pliron::value::Value;

use crate::ir::attributes::u32_value;
use crate::ir::design::DesignPlasmidOp;
use crate::ir::protocol::{
    AcceptOp, AssembleOp, AssemblyMethodAttr, GrowOp, ProvisionOp, PurifyOp, QuantifyOp, RecoverOp,
    SampleOp, ScreenOp, SelectOp, SequenceOp, SynthesizeOp, TransformOp,
};

use crate::pipeline::CompilerError;

/// Lower a verified Design IR module to target-selected Protocol operations.
#[derive(Clone, Debug)]
pub(crate) struct LowerDesignToProtocolPass {
    lab: LabProfile,
    assembly: AssemblyMethod,
}

impl LowerDesignToProtocolPass {
    pub(crate) fn new(lab: LabProfile, assembly: AssemblyMethod) -> Self {
        Self { lab, assembly }
    }
}

impl Pass for LowerDesignToProtocolPass {
    fn name(&self) -> &str {
        "design-to-protocol"
    }

    fn run(
        &mut self,
        operation: pliron::context::Ptr<Operation>,
        context: &mut Context,
        _analyses: &mut AnalysisManager,
    ) -> PlironResult<PassResult> {
        let module = Operation::get_op::<ModuleOp>(operation, context).ok_or_else(|| {
            pliron::input_error_noloc!("design-to-protocol requires builtin.module")
        })?;
        lower_design_to_protocol(context, module, &self.lab, self.assembly)
            .map_err(|error| pliron::input_error_noloc!(error))?;

        let mut result = PassResult::default();
        result.ir_changed = pliron::irbuild::IRStatus::Changed;
        Ok(result)
    }
}

pub(crate) fn lower_design_to_protocol(
    context: &mut Context,
    module: ModuleOp,
    lab: &LabProfile,
    assembly: AssemblyMethod,
) -> Result<(), CompilerError> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| invalid_ir("Design IR module has no entry block"))?;
    let design_operation = block
        .deref(context)
        .get_head()
        .ok_or_else(|| invalid_ir("Design IR module has no design operation"))?;
    let design = Operation::get_op::<DesignPlasmidOp>(design_operation, context)
        .ok_or_else(|| invalid_ir("Design IR module does not begin with design.plasmid"))?;

    let design_value = design.get_result_design(context);
    let exact_sequence_required: bool = design
        .get_attr_exact_sequence_required(context)
        .expect("verified design.plasmid has exact_sequence_required")
        .clone()
        .into();
    let minimum_concentration = design
        .get_attr_acceptance_minimum_concentration_ng_per_ul(context)
        .as_deref()
        .map(u32_value);
    let minimum_volume = design
        .get_attr_acceptance_minimum_volume_ul(context)
        .as_deref()
        .map(u32_value);

    if !exact_sequence_required {
        return Err(invalid_ir(
            "the initial plasmid lowering requires exact-sequence acceptance",
        ));
    }

    let mut inserter = IRInserter::<DummyListener>::new_at_block_end(block);

    let cells = ProvisionOp::competent_cells(context, lab.preferred_host());
    let cells_value = cells.get_result_material(context);
    name_value(context, cells_value, "competent_cells")?;
    inserter.append_op(context, &cells);

    let synthesize = SynthesizeOp::new(context, design_value);
    let fragments = synthesize.get_result_material(context);
    name_value(context, fragments, "dna_fragments")?;
    inserter.append_op(context, &synthesize);

    let assembly = match assembly {
        AssemblyMethod::Gibson => AssemblyMethodAttr::Gibson,
        AssemblyMethod::GoldenGate => AssemblyMethodAttr::GoldenGate,
    };
    let assemble = AssembleOp::new(context, fragments, assembly);
    let construct = assemble.get_result_construct(context);
    name_value(context, construct, "assembled_construct")?;
    inserter.append_op(context, &assemble);

    let transform = TransformOp::new(context, construct, cells_value, lab.preferred_host());
    let transformed = transform.get_result_culture(context);
    name_value(context, transformed, "transformed_culture")?;
    inserter.append_op(context, &transform);

    let recover = RecoverOp::new(context, transformed);
    let recovered = recover.get_result_recovered(context);
    name_value(context, recovered, "recovered_culture")?;
    inserter.append_op(context, &recover);

    let select = SelectOp::new(context, recovered);
    let colonies = select.get_result_colonies(context);
    name_value(context, colonies, "selected_colonies")?;
    inserter.append_op(context, &select);

    let screen = ScreenOp::new(context, colonies, "colony screening");
    let clone = screen.get_result_clone(context);
    name_value(context, clone, "screened_clone")?;
    inserter.append_op(context, &screen);

    let grow = GrowOp::new(context, clone);
    let culture = grow.get_result_culture(context);
    name_value(context, culture, "clone_culture")?;
    inserter.append_op(context, &grow);

    let purify = PurifyOp::new(context, culture);
    let plasmid = purify.get_result_plasmid(context);
    name_value(context, plasmid, "purified_plasmid")?;
    inserter.append_op(context, &purify);

    let sample = SampleOp::new(context, plasmid, "sequence verification");
    let retained = sample.get_result_retained(context);
    let sequence_aliquot = sample.get_result_aliquot(context);
    name_value(context, retained, "retained_after_sequencing")?;
    name_value(context, sequence_aliquot, "sequencing_aliquot")?;
    inserter.append_op(context, &sample);

    let sequence = SequenceOp::new(context, sequence_aliquot);
    let sequence_evidence = sequence.get_result_evidence(context);
    name_value(context, sequence_evidence, "sequence_evidence")?;
    inserter.append_op(context, &sequence);

    let mut accepted_material = retained;
    let mut evidence = vec![sequence_evidence];
    if minimum_concentration.is_some() || minimum_volume.is_some() {
        let quantify = QuantifyOp::new(context, retained, minimum_concentration, minimum_volume);
        accepted_material = quantify.get_result_retained(context);
        let concentration_evidence = quantify.get_result_concentration(context);
        let volume_evidence = quantify.get_result_volume(context);
        name_value(context, accepted_material, "retained_after_quantification")?;
        name_value(context, concentration_evidence, "concentration_evidence")?;
        name_value(context, volume_evidence, "volume_evidence")?;
        inserter.append_op(context, &quantify);
        if minimum_concentration.is_some() {
            evidence.push(concentration_evidence);
        }
        if minimum_volume.is_some() {
            evidence.push(volume_evidence);
        }
    }

    let accept = AcceptOp::new(context, accepted_material, evidence);
    name_value(
        context,
        accept.get_result_artifact(context),
        "validated_artifact",
    )?;
    inserter.append_op(context, &accept);

    Ok(())
}

fn name_value(context: &Context, value: Value, name: &str) -> Result<(), CompilerError> {
    value.set_name(context, Some(identifier(name)?));
    Ok(())
}

fn identifier(value: &str) -> Result<Identifier, CompilerError> {
    Identifier::try_from(value).map_err(|_| CompilerError::InvalidIdentifier(value.into()))
}

fn invalid_ir(message: impl Into<String>) -> CompilerError {
    CompilerError::InvalidIr(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::attributes::{u32_attr, u32_value};
    use crate::ir::protocol::QuantifyOp;
    use crate::translations::lower_specification_to_design;
    use crate::{
        AcceptanceCriterion, ArtifactSpec, AssemblyMethod, DnaSequence, LabProfile, PlasmidSpec,
        Topology,
    };

    #[test]
    fn protocol_is_lowered_from_design_ir_acceptance_attributes() {
        let specification = ArtifactSpec::plasmid(
            "p_design_source",
            PlasmidSpec::new(DnaSequence::new("ACGT").unwrap(), Topology::Circular).unwrap(),
            1,
            vec![AcceptanceCriterion::ExactSequence],
        )
        .unwrap();
        let mut context = Context::new();
        let module = lower_specification_to_design(&mut context, &specification).unwrap();
        let block = module
            .get_region(&context)
            .deref(&context)
            .get_head()
            .unwrap();
        let design_operation = block.deref(&context).get_head().unwrap();
        let design = Operation::get_op::<DesignPlasmidOp>(design_operation, &context).unwrap();
        design.set_attr_acceptance_minimum_volume_ul(&context, u32_attr(&context, 42));

        lower_design_to_protocol(
            &mut context,
            module,
            &LabProfile::reference(),
            AssemblyMethod::Gibson,
        )
        .unwrap();

        let quantify = block
            .deref(&context)
            .iter(&context)
            .find_map(|operation| Operation::get_op::<QuantifyOp>(operation, &context))
            .unwrap();
        let threshold = quantify.get_attr_minimum_volume_ul(&context).unwrap();
        assert_eq!(u32_value(&threshold), 42);
    }
}
