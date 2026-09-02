//! Module-level LAIR metadata operations.

use pliron::builtin::attributes::StringAttr;
use pliron::builtin::op_interfaces::{NOpdsInterface, NResultsInterface};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::verify_err;

/// A round-trippable declaration of the structural contract the enclosing module satisfies.
#[pliron_op(
    name = "lair.stage",
    format,
    attributes = (stage: StringAttr),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct StageOp;

impl StageOp {
    pub(crate) fn new(context: &mut Context, stage: impl Into<String>) -> Self {
        let operation = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: operation };
        result.set(context, stage);
        result
    }

    pub(crate) fn set(&self, context: &mut Context, stage: impl Into<String>) {
        self.set_attr_stage(context, StringAttr::new(stage.into()));
    }
}

impl Verify for StageOp {
    fn verify(&self, context: &Context) -> Result<()> {
        if self
            .get_attr_stage(context)
            .is_none_or(|stage| stage.as_str().is_empty())
        {
            return verify_err!(self.loc(context), "lair.stage requires a non-empty stage");
        }
        Ok(())
    }
}
