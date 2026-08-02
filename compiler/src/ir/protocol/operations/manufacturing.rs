use pliron::builtin::attributes::StringAttr;
use pliron::builtin::op_interfaces::NOpdsInterface;
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::value::Value;

use super::shared::{require_attr, require_material};
use crate::ir::design::DesignType;
use crate::ir::protocol::{AssemblyMethodAttr, MaterialType};

#[pliron_op(
    name = "protocol.provision",
    format,
    attributes = (item: StringAttr),
    interfaces = [NOpdsInterface<0>],
    results = (material: MaterialType)
)]
/// Provision competent host cells from laboratory inventory.
pub struct ProvisionOp;

impl ProvisionOp {
    pub fn competent_cells(ctx: &mut Context, item: impl Into<String>) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::CompetentCells.get(ctx)],
                vec![],
                vec![],
                0,
            ),
        };
        result.set_attr_item(ctx, StringAttr::new(item.into()));
        result
    }
}

impl Verify for ProvisionOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_attr(self.get_attr_item(ctx).is_some(), "item", self.loc(ctx))?;
        require_material(
            self.get_result_material(ctx),
            MaterialType::CompetentCells,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.synthesize",
    format,
    operands = (design: DesignType),
    results = (material: MaterialType)
)]
/// Materialize a design as linear DNA suitable for assembly.
pub struct SynthesizeOp;

impl SynthesizeOp {
    pub fn new(ctx: &mut Context, design: Value) -> Self {
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::LinearDna.get(ctx)],
                vec![design],
                vec![],
                0,
            ),
        }
    }
}

impl Verify for SynthesizeOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_result_material(ctx),
            MaterialType::LinearDna,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.assemble",
    format,
    attributes = (assembly_method: AssemblyMethodAttr),
    operands = (input: MaterialType),
    results = (construct: MaterialType)
)]
/// Assemble linear DNA into a circular construct using a selected strategy.
pub struct AssembleOp;

impl AssembleOp {
    pub fn new(ctx: &mut Context, input: Value, method: AssemblyMethodAttr) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::CircularDna.get(ctx)],
                vec![input],
                vec![],
                0,
            ),
        };
        result.set_attr_assembly_method(ctx, method);
        result
    }
}

impl Verify for AssembleOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_attr(
            self.get_attr_assembly_method(ctx).is_some(),
            "assembly_method",
            self.loc(ctx),
        )?;
        require_material(
            self.get_operand_input(ctx),
            MaterialType::LinearDna,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_construct(ctx),
            MaterialType::CircularDna,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.transform",
    format,
    attributes = (host: StringAttr),
    operands = (construct: MaterialType, cells: MaterialType),
    results = (culture: MaterialType)
)]
/// Introduce a circular construct into competent host cells.
pub struct TransformOp;

impl TransformOp {
    pub fn new(ctx: &mut Context, construct: Value, cells: Value, host: impl Into<String>) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::TransformedCulture.get(ctx)],
                vec![construct, cells],
                vec![],
                0,
            ),
        };
        result.set_attr_host(ctx, StringAttr::new(host.into()));
        result
    }
}

impl Verify for TransformOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_attr(self.get_attr_host(ctx).is_some(), "host", self.loc(ctx))?;
        require_material(
            self.get_operand_construct(ctx),
            MaterialType::CircularDna,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_operand_cells(ctx),
            MaterialType::CompetentCells,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_culture(ctx),
            MaterialType::TransformedCulture,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.recover",
    format,
    operands = (culture: MaterialType),
    results = (recovered: MaterialType)
)]
/// Recover transformed cells before selection.
pub struct RecoverOp;

impl RecoverOp {
    pub fn new(ctx: &mut Context, culture: Value) -> Self {
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::RecoveredCulture.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        }
    }
}

impl Verify for RecoverOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_operand_culture(ctx),
            MaterialType::TransformedCulture,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_recovered(ctx),
            MaterialType::RecoveredCulture,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.select",
    format,
    operands = (culture: MaterialType),
    results = (colonies: MaterialType)
)]
/// Select transformed colonies under the construct's selection conditions.
pub struct SelectOp;

impl SelectOp {
    pub fn new(ctx: &mut Context, culture: Value) -> Self {
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::ColonyPool.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        }
    }
}

impl Verify for SelectOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_operand_culture(ctx),
            MaterialType::RecoveredCulture,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_colonies(ctx),
            MaterialType::ColonyPool,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.screen",
    format,
    attributes = (screening_method: StringAttr),
    operands = (colonies: MaterialType),
    results = (clone: MaterialType)
)]
/// Screen a colony pool to choose a candidate clone.
pub struct ScreenOp;

impl ScreenOp {
    pub fn new(ctx: &mut Context, colonies: Value, method: impl Into<String>) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::SelectedClone.get(ctx)],
                vec![colonies],
                vec![],
                0,
            ),
        };
        result.set_attr_screening_method(ctx, StringAttr::new(method.into()));
        result
    }
}

impl Verify for ScreenOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_attr(
            self.get_attr_screening_method(ctx).is_some(),
            "screening_method",
            self.loc(ctx),
        )?;
        require_material(
            self.get_operand_colonies(ctx),
            MaterialType::ColonyPool,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_clone(ctx),
            MaterialType::SelectedClone,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.grow",
    format,
    operands = (clone: MaterialType),
    results = (culture: MaterialType)
)]
/// Propagate a selected clone as a culture.
pub struct GrowOp;

impl GrowOp {
    pub fn new(ctx: &mut Context, clone: Value) -> Self {
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::CloneCulture.get(ctx)],
                vec![clone],
                vec![],
                0,
            ),
        }
    }
}

impl Verify for GrowOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_operand_clone(ctx),
            MaterialType::SelectedClone,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_culture(ctx),
            MaterialType::CloneCulture,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "protocol.purify",
    format,
    operands = (culture: MaterialType),
    results = (plasmid: MaterialType)
)]
/// Purify plasmid material from a clone culture.
pub struct PurifyOp;

impl PurifyOp {
    pub fn new(ctx: &mut Context, culture: Value) -> Self {
        Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::PurifiedPlasmid.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        }
    }
}

impl Verify for PurifyOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_material(
            self.get_operand_culture(ctx),
            MaterialType::CloneCulture,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_plasmid(ctx),
            MaterialType::PurifiedPlasmid,
            self.loc(ctx),
            ctx,
        )
    }
}
