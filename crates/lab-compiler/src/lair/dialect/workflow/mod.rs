//! Target-neutral workflow intent.
//!
//! Workflow operations preserve the checked source program's material dataflow
//! and build policy. They deliberately describe neither a concrete laboratory
//! procedure nor robot resources; protocol selection owns that transition.

use pliron::builtin::attributes::{DictAttr, IntegerAttr, StringAttr, VecAttr};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::{pliron_op, pliron_type};
use pliron::location::Location;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::{Type, TypeHandle, Typed};
use pliron::value::Value;
use pliron::verify_err;

use crate::lair::dialect::attributes::{
    require_quantity_dict, require_string, require_string_vec, string_vec, u32_attr, u32_value,
    verify_u32_attr,
};
use crate::lair::dialect::chemistry::{ASSEMBLY_CHEMISTRY_KEYS, STRAIN_CHEMISTRY_KEYS};
use crate::lair::dialect::design::DesignType;

/// Abstract material states visible in a source-level build workflow.
#[pliron_type(name = "workflow.material", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialType {
    PlasmidProduct,
    StrainProduct,
    CompetentCells,
    TransformedCulture,
    RecoveredCulture,
    DilutedCulture,
    Plate,
}

impl MaterialType {
    pub fn get(self, ctx: &Context) -> TypeHandle {
        Self::instantiate(self, ctx).into()
    }
}

#[pliron_op(
    name = "workflow.realize",
    format,
    attributes = (
        realize_artifact: StringAttr,
        realize_backbone: StringAttr,
        realize_components: VecAttr,
        realize_dependencies: VecAttr,
        realize_restriction_enzyme: StringAttr,
        realize_assembly_replicates: IntegerAttr,
        realize_chemistry: DictAttr
    ),
    operands = (design: DesignType),
    results = (product: MaterialType)
)]
/// Request realization of a design using abstract assembly inputs and policy.
pub struct RealizeOp;

impl RealizeOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        design: Value,
        artifact_name: impl Into<String>,
        backbone: impl Into<String>,
        components: Vec<String>,
        dependencies: Vec<String>,
        restriction_enzyme: impl Into<String>,
        assembly_replicates: u8,
        chemistry: DictAttr,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::PlasmidProduct.get(ctx)],
                vec![design],
                vec![],
                0,
            ),
        };
        result.set_attr_realize_artifact(ctx, StringAttr::new(artifact_name.into()));
        result.set_attr_realize_backbone(ctx, StringAttr::new(backbone.into()));
        result.set_attr_realize_components(ctx, string_vec(components));
        result.set_attr_realize_dependencies(ctx, string_vec(dependencies));
        result.set_attr_realize_restriction_enzyme(ctx, StringAttr::new(restriction_enzyme.into()));
        result.set_attr_realize_assembly_replicates(ctx, u32_attr(ctx, assembly_replicates.into()));
        result.set_attr_realize_chemistry(ctx, chemistry);
        result
    }
}

impl Verify for RealizeOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_realize_artifact(ctx).as_deref(),
            "realize_artifact",
            self.loc(ctx),
        )?;
        require_string(
            self.get_attr_realize_backbone(ctx).as_deref(),
            "realize_backbone",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_realize_components(ctx).as_deref(),
            "realize_components",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_realize_dependencies(ctx).as_deref(),
            "realize_dependencies",
            self.loc(ctx),
        )?;
        require_string(
            self.get_attr_realize_restriction_enzyme(ctx).as_deref(),
            "realize_restriction_enzyme",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_realize_assembly_replicates(ctx).as_deref(),
            "realize_assembly_replicates",
            self.loc(ctx),
            ctx,
        )?;
        require_quantity_dict(
            self.get_attr_realize_chemistry(ctx).as_deref(),
            "realize_chemistry",
            ASSEMBLY_CHEMISTRY_KEYS,
            self.loc(ctx),
        )?;
        require_material(
            self.get_result_product(ctx),
            MaterialType::PlasmidProduct,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "workflow.provision",
    format,
    attributes = (provision_item: StringAttr),
    results = (material: MaterialType)
)]
/// Request an inventory item as abstract competent-cell material.
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
        result.set_attr_provision_item(ctx, StringAttr::new(item.into()));
        result
    }
}

impl Verify for ProvisionOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_provision_item(ctx).as_deref(),
            "provision_item",
            self.loc(ctx),
        )?;
        require_material(
            self.get_result_material(ctx),
            MaterialType::CompetentCells,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "workflow.transform",
    format,
    attributes = (
        transform_artifact: StringAttr,
        transform_chassis: StringAttr,
        transform_plasmids: VecAttr,
        transform_dependencies: VecAttr,
        transform_replicates: IntegerAttr,
        transform_chemistry: DictAttr
    ),
    operands = (design: DesignType, cells: MaterialType),
    results = (strain: MaterialType, culture: MaterialType)
)]
/// Realize a strain design by introducing its plasmids into competent cells.
/// The transformation both produces the named artifact and leaves a culture to
/// recover, dilute, and plate, so the strain's identity is established by the
/// same operation that creates the physical material.
pub struct TransformOp;

impl TransformOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        design: Value,
        cells: Value,
        artifact_name: impl Into<String>,
        chassis: impl Into<String>,
        plasmids: Vec<String>,
        dependencies: Vec<String>,
        replicates: u8,
        chemistry: DictAttr,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![
                    MaterialType::StrainProduct.get(ctx),
                    MaterialType::TransformedCulture.get(ctx),
                ],
                vec![design, cells],
                vec![],
                0,
            ),
        };
        result.set_attr_transform_artifact(ctx, StringAttr::new(artifact_name.into()));
        result.set_attr_transform_chassis(ctx, StringAttr::new(chassis.into()));
        result.set_attr_transform_plasmids(ctx, string_vec(plasmids));
        result.set_attr_transform_dependencies(ctx, string_vec(dependencies));
        result.set_attr_transform_replicates(ctx, u32_attr(ctx, replicates.into()));
        result.set_attr_transform_chemistry(ctx, chemistry);
        result
    }
}

impl Verify for TransformOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_transform_artifact(ctx).as_deref(),
            "transform_artifact",
            self.loc(ctx),
        )?;
        require_string(
            self.get_attr_transform_chassis(ctx).as_deref(),
            "transform_chassis",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_transform_plasmids(ctx).as_deref(),
            "transform_plasmids",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_transform_dependencies(ctx).as_deref(),
            "transform_dependencies",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_transform_replicates(ctx).as_deref(),
            "transform_replicates",
            self.loc(ctx),
            ctx,
        )?;
        require_quantity_dict(
            self.get_attr_transform_chemistry(ctx).as_deref(),
            "transform_chemistry",
            STRAIN_CHEMISTRY_KEYS,
            self.loc(ctx),
        )?;
        require_material(
            self.get_operand_cells(ctx),
            MaterialType::CompetentCells,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_strain(ctx),
            MaterialType::StrainProduct,
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
    name = "workflow.recover",
    format,
    attributes = (recover_duration_magnitude: StringAttr, recover_duration_unit: StringAttr),
    operands = (culture: MaterialType),
    results = (recovered: MaterialType)
)]
/// Request recovery for an explicit source-level duration.
pub struct RecoverOp;

impl RecoverOp {
    pub fn new(
        ctx: &mut Context,
        culture: Value,
        duration_magnitude: impl Into<String>,
        duration_unit: impl Into<String>,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::RecoveredCulture.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_recover_duration_magnitude(ctx, StringAttr::new(duration_magnitude.into()));
        result.set_attr_recover_duration_unit(ctx, StringAttr::new(duration_unit.into()));
        result
    }
}

impl Verify for RecoverOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_recover_duration_magnitude(ctx).as_deref(),
            "recover_duration_magnitude",
            self.loc(ctx),
        )?;
        require_string(
            self.get_attr_recover_duration_unit(ctx).as_deref(),
            "recover_duration_unit",
            self.loc(ctx),
        )?;
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
    name = "workflow.dilute",
    format,
    attributes = (dilute_serial_dilutions: IntegerAttr),
    operands = (culture: MaterialType),
    results = (diluted: MaterialType)
)]
/// Request a serial dilution policy for recovered culture.
pub struct DiluteOp;

impl DiluteOp {
    pub fn new(ctx: &mut Context, culture: Value, serial_dilutions: u8) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::DilutedCulture.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_dilute_serial_dilutions(ctx, u32_attr(ctx, serial_dilutions.into()));
        result
    }
}

impl Verify for DiluteOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_count(
            self.get_attr_dilute_serial_dilutions(ctx).as_deref(),
            "dilute_serial_dilutions",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_operand_culture(ctx),
            MaterialType::RecoveredCulture,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_diluted(ctx),
            MaterialType::DilutedCulture,
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "workflow.plate",
    format,
    attributes = (plate_selection: StringAttr, plate_replicates: IntegerAttr),
    operands = (culture: MaterialType),
    results = (plate: MaterialType)
)]
/// Request plating under an explicit selection condition.
pub struct PlateOp;

impl PlateOp {
    pub fn new(
        ctx: &mut Context,
        culture: Value,
        selection: impl Into<String>,
        replicates: u8,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::Plate.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_plate_selection(ctx, StringAttr::new(selection.into()));
        result.set_attr_plate_replicates(ctx, u32_attr(ctx, replicates.into()));
        result
    }
}

impl Verify for PlateOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_plate_selection(ctx).as_deref(),
            "plate_selection",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_plate_replicates(ctx).as_deref(),
            "plate_replicates",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_operand_culture(ctx),
            MaterialType::DilutedCulture,
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_plate(ctx),
            MaterialType::Plate,
            self.loc(ctx),
            ctx,
        )
    }
}

fn require_count(
    value: Option<&IntegerAttr>,
    name: &str,
    location: Location,
    ctx: &Context,
) -> Result<()> {
    let Some(value) = value else {
        return verify_err!(location, "workflow operation is missing attribute {name}");
    };
    verify_u32_attr(value, name, location.clone(), ctx)?;
    if u32_value(value) == 0 {
        return verify_err!(location, "workflow attribute {name} must be non-zero");
    }
    Ok(())
}

fn require_material(
    value: Value,
    expected: MaterialType,
    location: Location,
    ctx: &Context,
) -> Result<()> {
    let handle = value.get_type(ctx);
    let ty = handle.deref(ctx);
    let Some(actual) = ty.downcast_ref::<MaterialType>() else {
        return verify_err!(location, "expected Workflow material type {expected:?}");
    };
    if *actual != expected {
        return verify_err!(
            location,
            "expected Workflow material type {expected:?}, found {actual:?}"
        );
    }
    Ok(())
}
