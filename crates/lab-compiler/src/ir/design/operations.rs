use pliron::builtin::attributes::{BoolAttr, IntegerAttr, StringAttr};
use pliron::builtin::op_interfaces::NOpdsInterface;
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::verify_err;

use crate::ir::attributes::{u32_attr, u32_value, verify_u32_attr};
use crate::ir::design::{DesignType, TopologyAttr};

#[pliron_op(
    name = "design.plasmid",
    format,
    attributes = (
        artifact_name: StringAttr,
        sequence: StringAttr,
        topology: TopologyAttr,
        copies: IntegerAttr,
        exact_sequence_required: BoolAttr,
        acceptance_minimum_concentration_ng_per_ul: IntegerAttr,
        acceptance_minimum_volume_ul: IntegerAttr
    ),
    interfaces = [NOpdsInterface<0>],
    results = (design: DesignType)
)]
/// Declare a target-neutral circular plasmid design and its acceptance intent.
pub struct DesignPlasmidOp;

impl DesignPlasmidOp {
    pub fn new(
        ctx: &mut Context,
        artifact_name: impl Into<String>,
        sequence: impl Into<String>,
        copies: u16,
        exact_sequence_required: bool,
        minimum_concentration_ng_per_ul: Option<u32>,
        minimum_volume_ul: Option<u32>,
    ) -> Self {
        let op = Operation::new(
            ctx,
            Self::get_concrete_op_info(),
            vec![DesignType::get(ctx).into()],
            vec![],
            vec![],
            0,
        );
        let result = Self { op };
        result.set_attr_artifact_name(ctx, StringAttr::new(artifact_name.into()));
        result.set_attr_sequence(ctx, StringAttr::new(sequence.into()));
        result.set_attr_topology(ctx, TopologyAttr::Circular);
        result.set_attr_copies(ctx, u32_attr(ctx, copies.into()));
        result.set_attr_exact_sequence_required(ctx, BoolAttr::new(exact_sequence_required));
        if let Some(value) = minimum_concentration_ng_per_ul {
            result.set_attr_acceptance_minimum_concentration_ng_per_ul(ctx, u32_attr(ctx, value));
        }
        if let Some(value) = minimum_volume_ul {
            result.set_attr_acceptance_minimum_volume_ul(ctx, u32_attr(ctx, value));
        }
        result
    }
}

impl Verify for DesignPlasmidOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        for (name, present) in [
            ("artifact_name", self.get_attr_artifact_name(ctx).is_some()),
            ("sequence", self.get_attr_sequence(ctx).is_some()),
            ("topology", self.get_attr_topology(ctx).is_some()),
            ("copies", self.get_attr_copies(ctx).is_some()),
            (
                "exact_sequence_required",
                self.get_attr_exact_sequence_required(ctx).is_some(),
            ),
        ] {
            if !present {
                return verify_err!(self.loc(ctx), "design.plasmid is missing attribute {name}");
            }
        }
        let artifact_name = self
            .get_attr_artifact_name(ctx)
            .expect("presence checked above");
        if pliron::identifier::Identifier::try_from(artifact_name.as_str()).is_err() {
            return verify_err!(
                self.loc(ctx),
                "design.plasmid artifact_name must be an identifier"
            );
        }

        let sequence = self.get_attr_sequence(ctx).expect("presence checked above");
        if sequence.as_str().is_empty()
            || sequence
                .as_str()
                .bytes()
                .any(|base| !matches!(base, b'A' | b'C' | b'G' | b'T'))
        {
            return verify_err!(
                self.loc(ctx),
                "design.plasmid sequence must be non-empty, uppercase, and unambiguous DNA"
            );
        }

        let topology = self.get_attr_topology(ctx).expect("presence checked above");
        if *topology != TopologyAttr::Circular {
            return verify_err!(self.loc(ctx), "design.plasmid topology must be circular");
        }

        let copies = self.get_attr_copies(ctx).expect("presence checked above");
        verify_u32_attr(&copies, "design.plasmid copies", self.loc(ctx), ctx)?;
        if u32_value(&copies) == 0 {
            return verify_err!(self.loc(ctx), "design.plasmid copies must be non-zero");
        }

        for (name, attribute) in [
            (
                "acceptance_minimum_concentration_ng_per_ul",
                self.get_attr_acceptance_minimum_concentration_ng_per_ul(ctx),
            ),
            (
                "acceptance_minimum_volume_ul",
                self.get_attr_acceptance_minimum_volume_ul(ctx),
            ),
        ] {
            if let Some(attribute) = attribute {
                verify_u32_attr(&attribute, name, self.loc(ctx), ctx)?;
            }
        }
        Ok(())
    }
}
