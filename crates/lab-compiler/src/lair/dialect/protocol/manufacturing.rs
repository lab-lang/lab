// Construction APIs are consumed by LAIR transformations, which may be supplied
// independently of the compiler's source frontend.
#![allow(dead_code)]

use pliron::builtin::attributes::{DictAttr, IntegerAttr, StringAttr, VecAttr};
use pliron::builtin::op_interfaces::NOpdsInterface;
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::value::Value;

use crate::lair::dialect::attributes::{
    require_quantity_dict, require_string_vec, string_vec, u32_attr, u32_value, verify_u32_attr,
};
use crate::lair::dialect::chemistry::{ASSEMBLY_CHEMISTRY_KEYS, STRAIN_CHEMISTRY_KEYS};
use crate::lair::dialect::design::DesignType;
use crate::lair::dialect::protocol::validation::{require_attr, require_material};
use crate::lair::dialect::protocol::{AssemblyMethodAttr, MaterialType};

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
    attributes = (
        assembly_method: AssemblyMethodAttr,
        assembly_artifact: StringAttr,
        assembly_backbone: StringAttr,
        assembly_components: VecAttr,
        assembly_dependencies: VecAttr,
        assembly_restriction_enzyme: StringAttr,
        assembly_replicates: IntegerAttr,
        assembly_chemistry: DictAttr
    ),
    operands = (input: MaterialType),
    results = (construct: MaterialType)
)]
/// Assemble linear DNA into a circular construct using a selected strategy.
pub struct AssembleOp;

impl AssembleOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        input: Value,
        method: AssemblyMethodAttr,
        artifact_name: impl Into<String>,
        backbone: impl Into<String>,
        components: Vec<String>,
        dependencies: Vec<String>,
        restriction_enzyme: impl Into<String>,
        replicates: u8,
        chemistry: DictAttr,
    ) -> Self {
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
        result.set_attr_assembly_artifact(ctx, StringAttr::new(artifact_name.into()));
        result.set_attr_assembly_backbone(ctx, StringAttr::new(backbone.into()));
        result.set_attr_assembly_components(ctx, string_vec(components));
        result.set_attr_assembly_dependencies(ctx, string_vec(dependencies));
        result
            .set_attr_assembly_restriction_enzyme(ctx, StringAttr::new(restriction_enzyme.into()));
        result.set_attr_assembly_replicates(ctx, u32_attr(ctx, replicates.into()));
        result.set_attr_assembly_chemistry(ctx, chemistry);
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
        require_nonempty_string(
            self.get_attr_assembly_artifact(ctx).as_deref(),
            "assembly_artifact",
            self.loc(ctx),
        )?;
        require_nonempty_string(
            self.get_attr_assembly_backbone(ctx).as_deref(),
            "assembly_backbone",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_assembly_components(ctx).as_deref(),
            "assembly_components",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_assembly_dependencies(ctx).as_deref(),
            "assembly_dependencies",
            self.loc(ctx),
        )?;
        require_nonempty_string(
            self.get_attr_assembly_restriction_enzyme(ctx).as_deref(),
            "assembly_restriction_enzyme",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_assembly_replicates(ctx).as_deref(),
            "assembly_replicates",
            self.loc(ctx),
            ctx,
        )?;
        require_quantity_dict(
            self.get_attr_assembly_chemistry(ctx).as_deref(),
            "assembly_chemistry",
            ASSEMBLY_CHEMISTRY_KEYS,
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
    attributes = (
        transformation_artifact: StringAttr,
        host: StringAttr,
        transformation_plasmids: VecAttr,
        transformation_dependencies: VecAttr,
        transformation_replicates: IntegerAttr,
        transformation_chemistry: DictAttr
    ),
    operands = (design: DesignType, cells: MaterialType),
    results = (strain: MaterialType, culture: MaterialType)
)]
/// Introduce a strain's plasmids into competent host cells, producing the named
/// engineered organism and the culture that carries it.
pub struct TransformOp;

impl TransformOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        design: Value,
        cells: Value,
        artifact: impl Into<String>,
        host: impl Into<String>,
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
                    MaterialType::EngineeredStrain.get(ctx),
                    MaterialType::TransformedCulture.get(ctx),
                ],
                vec![design, cells],
                vec![],
                0,
            ),
        };
        result.set_attr_transformation_artifact(ctx, StringAttr::new(artifact.into()));
        result.set_attr_host(ctx, StringAttr::new(host.into()));
        result.set_attr_transformation_plasmids(ctx, string_vec(plasmids));
        result.set_attr_transformation_dependencies(ctx, string_vec(dependencies));
        result.set_attr_transformation_replicates(ctx, u32_attr(ctx, replicates.into()));
        result.set_attr_transformation_chemistry(ctx, chemistry);
        result
    }
}

impl Verify for TransformOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_attr(
            self.get_attr_transformation_artifact(ctx).is_some(),
            "transformation_artifact",
            self.loc(ctx),
        )?;
        require_attr(self.get_attr_host(ctx).is_some(), "host", self.loc(ctx))?;
        require_string_vec(
            self.get_attr_transformation_plasmids(ctx).as_deref(),
            "transformation_plasmids",
            self.loc(ctx),
        )?;
        require_string_vec(
            self.get_attr_transformation_dependencies(ctx).as_deref(),
            "transformation_dependencies",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_transformation_replicates(ctx).as_deref(),
            "transformation_replicates",
            self.loc(ctx),
            ctx,
        )?;
        require_quantity_dict(
            self.get_attr_transformation_chemistry(ctx).as_deref(),
            "transformation_chemistry",
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
            MaterialType::EngineeredStrain,
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
    attributes = (recovery_duration_magnitude: StringAttr, recovery_duration_unit: StringAttr),
    operands = (culture: MaterialType),
    results = (recovered: MaterialType)
)]
/// Recover transformed cells before selection.
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
        result
            .set_attr_recovery_duration_magnitude(ctx, StringAttr::new(duration_magnitude.into()));
        result.set_attr_recovery_duration_unit(ctx, StringAttr::new(duration_unit.into()));
        result
    }
}

impl Verify for RecoverOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_nonempty_string(
            self.get_attr_recovery_duration_magnitude(ctx).as_deref(),
            "recovery_duration_magnitude",
            self.loc(ctx),
        )?;
        require_nonempty_string(
            self.get_attr_recovery_duration_unit(ctx).as_deref(),
            "recovery_duration_unit",
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
    name = "protocol.dilute",
    format,
    attributes = (serial_dilutions: IntegerAttr),
    operands = (culture: MaterialType),
    results = (diluted: MaterialType)
)]
/// Apply a selected serial-dilution procedure to recovered culture.
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
        result.set_attr_serial_dilutions(ctx, u32_attr(ctx, serial_dilutions.into()));
        result
    }
}

impl Verify for DiluteOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_count(
            self.get_attr_serial_dilutions(ctx).as_deref(),
            "serial_dilutions",
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
    name = "protocol.plate",
    format,
    attributes = (plating_selection: StringAttr, plating_replicates: IntegerAttr),
    operands = (culture: MaterialType),
    results = (plate: MaterialType)
)]
/// Plate diluted culture under the selected antibiotic condition.
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
                vec![MaterialType::SelectionPlate.get(ctx)],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_plating_selection(ctx, StringAttr::new(selection.into()));
        result.set_attr_plating_replicates(ctx, u32_attr(ctx, replicates.into()));
        result
    }
}

impl Verify for PlateOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_nonempty_string(
            self.get_attr_plating_selection(ctx).as_deref(),
            "plating_selection",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_plating_replicates(ctx).as_deref(),
            "plating_replicates",
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
            MaterialType::SelectionPlate,
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

fn require_nonempty_string(
    value: Option<&StringAttr>,
    name: &str,
    location: pliron::location::Location,
) -> Result<()> {
    if value.is_none_or(|value| value.as_str().is_empty()) {
        return pliron::verify_err!(
            location,
            "protocol operation requires non-empty attribute {name}"
        );
    }
    Ok(())
}

fn require_count(
    value: Option<&IntegerAttr>,
    name: &str,
    location: pliron::location::Location,
    ctx: &Context,
) -> Result<()> {
    let Some(value) = value else {
        return pliron::verify_err!(location, "protocol operation is missing attribute {name}");
    };
    verify_u32_attr(value, name, location.clone(), ctx)?;
    if u32_value(value) == 0 {
        return pliron::verify_err!(location, "protocol attribute {name} must be non-zero");
    }
    Ok(())
}
