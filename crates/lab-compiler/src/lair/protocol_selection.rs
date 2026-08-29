//! Dialect conversion from method-neutral Workflow intent to Protocol LAIR.

use pliron::attribute::AttrObj;
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::context::{Context, Ptr};
use pliron::irbuild::dialect_conversion::{
    DialectConversion, DialectConversionRewriter, OperandsInfo, apply_dialect_conversion,
};
use pliron::irbuild::inserter::Inserter;
use pliron::irbuild::rewriter::Rewriter;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;

use crate::lair::dialect::attributes::u32_value;
use crate::lair::dialect::protocol::{
    AssembleOp as ProtocolAssembleOp, AssemblyMethodAttr, DiluteOp as ProtocolDiluteOp,
    PlateOp as ProtocolPlateOp, ProvisionOp as ProtocolProvisionOp, RecoverOp as ProtocolRecoverOp,
    SynthesizeOp, TransformOp as ProtocolTransformOp,
};
use crate::lair::dialect::workflow::{
    DiluteOp, PlateOp, ProvisionOp, RealizeOp, RecoverOp, TransformOp,
};

/// Select the concrete biological procedure currently supported by the plasmid
/// build backends. This is a real dialect conversion: Workflow values are
/// replaced by Protocol values before Workflow operations are erased.
pub(crate) fn select_plasmid_build_protocol(
    context: &mut Context,
    root: Ptr<Operation>,
) -> Result<()> {
    apply_dialect_conversion(context, &mut PlasmidBuildSelection, root)?;
    Ok(())
}

struct PlasmidBuildSelection;

impl DialectConversion for PlasmidBuildSelection {
    fn can_convert_op(&self, ctx: &Context, op: Ptr<Operation>) -> bool {
        Operation::get_op::<RealizeOp>(op, ctx).is_some()
            || Operation::get_op::<ProvisionOp>(op, ctx).is_some()
            || Operation::get_op::<TransformOp>(op, ctx).is_some()
            || Operation::get_op::<RecoverOp>(op, ctx).is_some()
            || Operation::get_op::<DiluteOp>(op, ctx).is_some()
            || Operation::get_op::<PlateOp>(op, ctx).is_some()
    }

    fn rewrite(
        &mut self,
        ctx: &mut Context,
        rewriter: &mut DialectConversionRewriter,
        op: Ptr<Operation>,
        _operands_info: &OperandsInfo,
    ) -> Result<()> {
        if let Some(realize) = Operation::get_op::<RealizeOp>(op, ctx) {
            let synthesize = SynthesizeOp::new(ctx, realize.get_operand_design(ctx));
            rewriter.insert_operation(ctx, synthesize.get_operation());
            let artifact_name = required_string(realize.get_attr_realize_artifact(ctx).as_deref());
            let backbone = required_string(realize.get_attr_realize_backbone(ctx).as_deref());
            let components = required_strings(realize.get_attr_realize_components(ctx).as_deref());
            let dependencies =
                required_strings(realize.get_attr_realize_dependencies(ctx).as_deref());
            let restriction_enzyme =
                required_string(realize.get_attr_realize_restriction_enzyme(ctx).as_deref());
            let replicates =
                required_count(realize.get_attr_realize_assembly_replicates(ctx).as_deref());
            let chemistry = realize
                .get_attr_realize_chemistry(ctx)
                .as_deref()
                .cloned()
                .expect("verified Workflow realize carries its chemistry");
            let input = synthesize.get_result_material(ctx);
            let assemble = ProtocolAssembleOp::new(
                ctx,
                input,
                AssemblyMethodAttr::GoldenGate,
                artifact_name,
                backbone,
                components,
                dependencies,
                restriction_enzyme,
                replicates,
                chemistry,
            );
            rewriter.insert_operation(ctx, assemble.get_operation());
            rewriter.replace_value_uses_with(
                ctx,
                realize.get_result_product(ctx),
                assemble.get_result_construct(ctx),
            );
            rewriter.erase_operation(ctx, op);
            return Ok(());
        }

        if let Some(provision) = Operation::get_op::<ProvisionOp>(op, ctx) {
            let item = required_string(provision.get_attr_provision_item(ctx).as_deref());
            let selected = ProtocolProvisionOp::competent_cells(ctx, item);
            rewriter.insert_operation(ctx, selected.get_operation());
            rewriter.replace_value_uses_with(
                ctx,
                provision.get_result_material(ctx),
                selected.get_result_material(ctx),
            );
            rewriter.erase_operation(ctx, op);
            return Ok(());
        }

        if let Some(transform) = Operation::get_op::<TransformOp>(op, ctx) {
            let cells = transform.get_operand_cells(ctx);
            let artifact = required_string(transform.get_attr_transform_artifact(ctx).as_deref());
            let host = required_string(transform.get_attr_transform_chassis(ctx).as_deref());
            let plasmids = required_strings(transform.get_attr_transform_plasmids(ctx).as_deref());
            let dependencies =
                required_strings(transform.get_attr_transform_dependencies(ctx).as_deref());
            let replicates =
                required_count(transform.get_attr_transform_replicates(ctx).as_deref());
            let chemistry = transform
                .get_attr_transform_chemistry(ctx)
                .as_deref()
                .cloned()
                .expect("verified Workflow transform carries its chemistry");
            let design = transform.get_operand_design(ctx);
            let selected = ProtocolTransformOp::new(
                ctx,
                design,
                cells,
                artifact,
                host,
                plasmids,
                dependencies,
                replicates,
                chemistry,
            );
            rewriter.insert_operation(ctx, selected.get_operation());
            rewriter.replace_value_uses_with(
                ctx,
                transform.get_result_strain(ctx),
                selected.get_result_strain(ctx),
            );
            rewriter.replace_value_uses_with(
                ctx,
                transform.get_result_culture(ctx),
                selected.get_result_culture(ctx),
            );
            rewriter.erase_operation(ctx, op);
            return Ok(());
        }

        if let Some(recover) = Operation::get_op::<RecoverOp>(op, ctx) {
            let duration_magnitude =
                required_string(recover.get_attr_recover_duration_magnitude(ctx).as_deref());
            let duration_unit =
                required_string(recover.get_attr_recover_duration_unit(ctx).as_deref());
            let selected = ProtocolRecoverOp::new(
                ctx,
                recover.get_operand_culture(ctx),
                duration_magnitude,
                duration_unit,
            );
            rewriter.insert_operation(ctx, selected.get_operation());
            rewriter.replace_value_uses_with(
                ctx,
                recover.get_result_recovered(ctx),
                selected.get_result_recovered(ctx),
            );
            rewriter.erase_operation(ctx, op);
            return Ok(());
        }

        if let Some(dilute) = Operation::get_op::<DiluteOp>(op, ctx) {
            let serial_dilutions =
                required_count(dilute.get_attr_dilute_serial_dilutions(ctx).as_deref());
            let selected =
                ProtocolDiluteOp::new(ctx, dilute.get_operand_culture(ctx), serial_dilutions);
            rewriter.insert_operation(ctx, selected.get_operation());
            rewriter.replace_value_uses_with(
                ctx,
                dilute.get_result_diluted(ctx),
                selected.get_result_diluted(ctx),
            );
            rewriter.erase_operation(ctx, op);
            return Ok(());
        }

        if let Some(plate) = Operation::get_op::<PlateOp>(op, ctx) {
            let selection = required_string(plate.get_attr_plate_selection(ctx).as_deref());
            let replicates = required_count(plate.get_attr_plate_replicates(ctx).as_deref());
            let selected =
                ProtocolPlateOp::new(ctx, plate.get_operand_culture(ctx), selection, replicates);
            rewriter.insert_operation(ctx, selected.get_operation());
            rewriter.erase_operation(ctx, op);
        }
        Ok(())
    }
}

fn required_string(attribute: Option<&StringAttr>) -> String {
    attribute
        .expect("verified Workflow operation has required string attribute")
        .as_str()
        .to_owned()
}

fn required_strings(attribute: Option<&VecAttr>) -> Vec<String> {
    attribute
        .expect("verified Workflow operation has required vector attribute")
        .0
        .iter()
        .map(|value: &AttrObj| {
            value
                .downcast_ref::<StringAttr>()
                .expect("verified Workflow string vector contains strings")
                .as_str()
                .to_owned()
        })
        .collect()
}

fn required_count(attribute: Option<&pliron::builtin::attributes::IntegerAttr>) -> u8 {
    u8::try_from(u32_value(
        attribute.expect("verified Workflow operation has required count attribute"),
    ))
    .expect("source workflow count was checked as u8")
}
