use pliron::builtin::attributes::{IntegerAttr, StringAttr};
use pliron::builtin::op_interfaces::OneResultInterface;
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::value::Value;
use pliron::verify_err;

use super::shared::{require_any_evidence, require_attr, require_evidence, require_material};
use crate::ir::attributes::{u32_attr, verify_u32_attr};
use crate::ir::protocol::{EvidenceType, MaterialType};

#[pliron_op(
    name = "protocol.sample",
    format,
    attributes = (purpose: StringAttr),
    operands = (source: MaterialType),
    results = (retained: MaterialType, aliquot: MaterialType)
)]
/// Split purified material into a retained sample and an assay aliquot.
pub struct SampleOp;

impl SampleOp {
    pub fn new(ctx: &mut Context, source: Value, purpose: impl Into<String>) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![
                    MaterialType::PurifiedPlasmid.get(ctx),
                    MaterialType::AssayAliquot.get(ctx),
                ],
                vec![source],
                vec![],
                0,
            ),
        };
        result.set_attr_purpose(ctx, StringAttr::new(purpose.into()));
        result
    }
}

impl Verify for SampleOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_attr(
            self.get_attr_purpose(ctx).is_some(),
            "purpose",
            self.loc(ctx),
        )?;
        require_material(
            self.get_operand_source(ctx),
            MaterialType::PurifiedPlasmid,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_retained(ctx),
            MaterialType::PurifiedPlasmid,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_aliquot(ctx),
            MaterialType::AssayAliquot,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.sequence",
    format,
    operands = (aliquot: MaterialType),
    results = (evidence: EvidenceType)
)]
/// Produce sequence-identity evidence from an assay aliquot.
pub struct SequenceOp;

impl SequenceOp {
    pub fn new(ctx: &mut Context, aliquot: Value) -> Self {
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![EvidenceType::SequenceIdentity.get(ctx)],
                vec![aliquot],
                vec![],
                0,
            ),
        }
    }
}

impl Verify for SequenceOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_operand_aliquot(ctx),
            MaterialType::AssayAliquot,
            self.loc(ctx),
            ctx,
        )?;
        require_evidence(
            self.get_result_evidence(ctx),
            EvidenceType::SequenceIdentity,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.quantify",
    format,
    attributes = (
        minimum_concentration_ng_per_ul: IntegerAttr,
        minimum_volume_ul: IntegerAttr
    ),
    operands = (source: MaterialType),
    results = (
        retained: MaterialType,
        concentration: EvidenceType,
        volume: EvidenceType
    )
)]
/// Measure concentration and volume evidence while retaining the material.
pub struct QuantifyOp;

impl QuantifyOp {
    pub fn new(
        ctx: &mut Context,
        source: Value,
        minimum_concentration_ng_per_ul: Option<u32>,
        minimum_volume_ul: Option<u32>,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![
                    MaterialType::PurifiedPlasmid.get(ctx),
                    EvidenceType::Concentration.get(ctx),
                    EvidenceType::Volume.get(ctx),
                ],
                vec![source],
                vec![],
                0,
            ),
        };
        if let Some(value) = minimum_concentration_ng_per_ul {
            result.set_attr_minimum_concentration_ng_per_ul(ctx, u32_attr(ctx, value));
        }
        if let Some(value) = minimum_volume_ul {
            result.set_attr_minimum_volume_ul(ctx, u32_attr(ctx, value));
        }
        result
    }
}

impl Verify for QuantifyOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_operand_source(ctx),
            MaterialType::PurifiedPlasmid,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_retained(ctx),
            MaterialType::PurifiedPlasmid,
            self.loc(ctx),
            ctx,
        )?;
        require_evidence(
            self.get_result_concentration(ctx),
            EvidenceType::Concentration,
            self.loc(ctx),
            ctx,
        )?;
        require_evidence(
            self.get_result_volume(ctx),
            EvidenceType::Volume,
            self.loc(ctx),
            ctx,
        )?;
        for (name, attribute) in [
            (
                "minimum_concentration_ng_per_ul",
                self.get_attr_minimum_concentration_ng_per_ul(ctx),
            ),
            ("minimum_volume_ul", self.get_attr_minimum_volume_ul(ctx)),
        ] {
            if let Some(attribute) = attribute {
                verify_u32_attr(&attribute, name, self.loc(ctx), ctx)?;
            }
        }
        Ok(())
    }
}

#[pliron_op(
    name = "protocol.accept",
    format,
    interfaces = [OneResultInterface],
    results = (artifact: MaterialType)
)]
/// Accept material as a validated artifact using explicit evidence operands.
pub struct AcceptOp;

impl AcceptOp {
    pub fn new(
        ctx: &mut Context,
        material: Value,
        evidence: impl IntoIterator<Item = Value>,
    ) -> Self {
        let mut operands = vec![material];
        operands.extend(evidence);
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::ValidatedPlasmid.get(ctx)],
                operands,
                vec![],
                0,
            ),
        }
    }
}

impl Verify for AcceptOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        let operation = self.get_operation().deref(ctx);
        if operation.get_num_operands() < 2 {
            return verify_err!(
                self.loc(ctx),
                "protocol.accept requires a material and at least one evidence operand"
            );
        }
        require_material(
            operation.get_operand(0),
            MaterialType::PurifiedPlasmid,
            self.loc(ctx),
            ctx,
        )?;
        for index in 1..operation.get_num_operands() {
            require_any_evidence(operation.get_operand(index), self.loc(ctx), ctx)?;
        }
        require_material(
            self.get_result_artifact(ctx),
            MaterialType::ValidatedPlasmid,
            self.loc(ctx),
            ctx,
        )
    }
}
