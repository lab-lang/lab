//! Target-neutral workflow intent.
//!
//! Workflow operations preserve the checked source program's material dataflow
//! and build policy. They deliberately describe neither a concrete laboratory
//! procedure nor robot resources; protocol selection owns that transition.

use lab_capability::AbsoluteIri;
use pliron::builtin::attributes::{DictAttr, IntegerAttr, StringAttr, VecAttr};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::{pliron_op, pliron_type};
use pliron::location::Location;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::{TypeHandle, Typed};
use pliron::value::Value;
use pliron::{verify_err, verify_err_noloc};

use crate::design::ir::DesignType;
use crate::ir::attributes::{
    require_quantity_dict, require_string, require_string_vec, string_vec, u32_attr, u32_value,
    verify_u32_attr,
};
use crate::workflow::chemistry::{ASSEMBLY_CHEMISTRY_KEYS, STRAIN_CHEMISTRY_KEYS};

/// The namespace the states this compiler mints are named in.
pub const STATE_NS: &str = "https://www.lab-compiler.org/ns/material-state#";

/// One abstract material state visible in a source-level build workflow.
///
/// The state is an absolute IRI rather than one of a fixed set, because the set
/// was never fixed: `method::standard` already names `AssemblyReaction`,
/// `TransformationMixture`, and `RecoveryMixture`, none of which a closed
/// enumeration here admitted. A package that declares a facet names states this
/// dialect has not heard of, and that is the point.
#[pliron_type(
    name = "workflow.material",
    generate_get = true,
    format = "`<` $state `>`"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MaterialType {
    state: StringAttr,
}

impl MaterialType {
    /// The handle for a state this compiler mints, given its bare name.
    pub fn state(ctx: &Context, name: &str) -> TypeHandle {
        Self::get(ctx, StringAttr::new(format!("{STATE_NS}{name}"))).into()
    }

    /// The absolute IRI this state is named by.
    pub fn iri(&self) -> &str {
        self.state.as_str()
    }
}

impl Verify for MaterialType {
    fn verify(&self, _context: &Context) -> Result<()> {
        if AbsoluteIri::new(self.state.as_str()).is_err() {
            return verify_err_noloc!("workflow.material state must be an absolute IRI");
        }
        Ok(())
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
    /// Construct an artifact-realization Intent without assuming a laboratory method.
    ///
    /// The product's state is named for what is being realized. A plasmid
    /// arrives as `PlasmidProduct` and a medium as `MediumProduct`, and the
    /// caller says which because the caller read the declaration.
    pub fn new(
        ctx: &mut Context,
        design: Value,
        artifact_name: impl Into<String>,
        dependencies: Vec<String>,
        state: &str,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::state(ctx, state)],
                vec![design],
                vec![],
                0,
            ),
        };
        result.set_attr_realize_artifact(ctx, StringAttr::new(artifact_name.into()));
        result.set_attr_realize_dependencies(ctx, string_vec(dependencies));
        result
    }

    /// Construct an artifact-realization Intent carrying a complete Golden Gate recipe.
    #[allow(clippy::too_many_arguments)]
    pub fn golden_gate(
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
        let result = Self::new(ctx, design, artifact_name, dependencies, "PlasmidProduct");
        result.set_attr_realize_backbone(ctx, StringAttr::new(backbone.into()));
        result.set_attr_realize_components(ctx, string_vec(components));
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
        require_string_vec(
            self.get_attr_realize_dependencies(ctx).as_deref(),
            "realize_dependencies",
            self.loc(ctx),
        )?;
        let recipe_attributes = [
            self.get_attr_realize_backbone(ctx).is_some(),
            self.get_attr_realize_components(ctx).is_some(),
            self.get_attr_realize_restriction_enzyme(ctx).is_some(),
            self.get_attr_realize_assembly_replicates(ctx).is_some(),
            self.get_attr_realize_chemistry(ctx).is_some(),
        ];
        let present = recipe_attributes.iter().filter(|present| **present).count();
        if present != 0 && present != recipe_attributes.len() {
            return verify_err!(
                self.loc(ctx),
                "workflow.realize must carry either every Golden Gate recipe attribute or none"
            );
        }
        if present == recipe_attributes.len() {
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
        }
        // Which state the product arrives in is named for what is realized, so
        // the verifier checks that it is a material rather than which one. A
        // Golden Gate recipe still implies a plasmid, and says so.
        if present == recipe_attributes.len() {
            return require_material(
                self.get_result_product(ctx),
                "PlasmidProduct",
                self.loc(ctx),
                ctx,
            );
        }
        require_any_material(
            self.get_result_product(ctx),
            "workflow.realize product",
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
/// Request an inventory item as the material its kind arrives in.
pub struct ProvisionOp;

impl ProvisionOp {
    /// Fetch an item off the shelf in the state its kind arrives in.
    ///
    /// The state is stated by the caller rather than assumed here, so fetching
    /// an antibiotic yields an antibiotic. The provisioning Method reads it back
    /// from this result, which is how one signature serves every kind of thing a
    /// shelf holds.
    pub fn new(ctx: &mut Context, item: impl Into<String>, state: &str) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::state(ctx, state)],
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
        // Which state a fetched item is in depends on what was fetched, so the
        // verifier checks that it is a material rather than which one.
        require_any_material(
            self.get_result_material(ctx),
            "workflow.provision result",
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
                    MaterialType::state(ctx, "StrainProduct"),
                    MaterialType::state(ctx, "TransformedCulture"),
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
            "CompetentCells",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_strain(ctx),
            "StrainProduct",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_culture(ctx),
            "TransformedCulture",
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "workflow.recover",
    format,
    attributes = (
        recover_artifact: StringAttr,
        recover_duration_magnitude: StringAttr,
        recover_duration_unit: StringAttr,
        recover_replicates: IntegerAttr,
        recover_initial_volume_ul: IntegerAttr,
        recover_medium_aliquot_volume_ul: IntegerAttr,
        recover_medium_volume_ul: IntegerAttr,
        recover_temperature_c: IntegerAttr
    ),
    operands = (culture: MaterialType),
    results = (recovered: MaterialType)
)]
/// Request recovery for an explicit source-level duration.
pub struct RecoverOp;

impl RecoverOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        culture: Value,
        artifact: impl Into<String>,
        duration_magnitude: impl Into<String>,
        duration_unit: impl Into<String>,
        replicates: u8,
        initial_volume_ul: u32,
        medium_aliquot_volume_ul: u16,
        medium_volume_ul: u16,
        temperature_c: u16,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::state(ctx, "RecoveredCulture")],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_recover_artifact(ctx, StringAttr::new(artifact.into()));
        result.set_attr_recover_duration_magnitude(ctx, StringAttr::new(duration_magnitude.into()));
        result.set_attr_recover_duration_unit(ctx, StringAttr::new(duration_unit.into()));
        result.set_attr_recover_replicates(ctx, u32_attr(ctx, replicates.into()));
        result.set_attr_recover_initial_volume_ul(ctx, u32_attr(ctx, initial_volume_ul));
        result.set_attr_recover_medium_aliquot_volume_ul(
            ctx,
            u32_attr(ctx, medium_aliquot_volume_ul.into()),
        );
        result.set_attr_recover_medium_volume_ul(ctx, u32_attr(ctx, medium_volume_ul.into()));
        result.set_attr_recover_temperature_c(ctx, u32_attr(ctx, temperature_c.into()));
        result
    }
}

impl Verify for RecoverOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_recover_artifact(ctx).as_deref(),
            "recover_artifact",
            self.loc(ctx),
        )?;
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
        for (value, name) in [
            (self.get_attr_recover_replicates(ctx), "recover_replicates"),
            (
                self.get_attr_recover_initial_volume_ul(ctx),
                "recover_initial_volume_ul",
            ),
            (
                self.get_attr_recover_medium_aliquot_volume_ul(ctx),
                "recover_medium_aliquot_volume_ul",
            ),
            (
                self.get_attr_recover_medium_volume_ul(ctx),
                "recover_medium_volume_ul",
            ),
            (
                self.get_attr_recover_temperature_c(ctx),
                "recover_temperature_c",
            ),
        ] {
            require_count(value.as_deref(), name, self.loc(ctx), ctx)?;
        }
        require_material(
            self.get_operand_culture(ctx),
            "TransformedCulture",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_recovered(ctx),
            "RecoveredCulture",
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "workflow.dilute",
    format,
    attributes = (
        dilute_artifact: StringAttr,
        dilute_serial_dilutions: IntegerAttr,
        dilute_replicates: IntegerAttr,
        dilute_initial_volume_ul: IntegerAttr,
        dilute_medium_volume_ul: IntegerAttr,
        dilute_culture_volume_ul: IntegerAttr
    ),
    operands = (culture: MaterialType),
    results = (diluted: MaterialType)
)]
/// Request a serial dilution policy for recovered culture.
pub struct DiluteOp;

impl DiluteOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        culture: Value,
        artifact: impl Into<String>,
        serial_dilutions: u8,
        replicates: u8,
        initial_volume_ul: u32,
        medium_volume_ul: u16,
        culture_volume_ul: u16,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::state(ctx, "DilutedCulture")],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_dilute_artifact(ctx, StringAttr::new(artifact.into()));
        result.set_attr_dilute_serial_dilutions(ctx, u32_attr(ctx, serial_dilutions.into()));
        result.set_attr_dilute_replicates(ctx, u32_attr(ctx, replicates.into()));
        result.set_attr_dilute_initial_volume_ul(ctx, u32_attr(ctx, initial_volume_ul));
        result.set_attr_dilute_medium_volume_ul(ctx, u32_attr(ctx, medium_volume_ul.into()));
        result.set_attr_dilute_culture_volume_ul(ctx, u32_attr(ctx, culture_volume_ul.into()));
        result
    }
}

impl Verify for DiluteOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_dilute_artifact(ctx).as_deref(),
            "dilute_artifact",
            self.loc(ctx),
        )?;
        require_count(
            self.get_attr_dilute_serial_dilutions(ctx).as_deref(),
            "dilute_serial_dilutions",
            self.loc(ctx),
            ctx,
        )?;
        require_count(
            self.get_attr_dilute_replicates(ctx).as_deref(),
            "dilute_replicates",
            self.loc(ctx),
            ctx,
        )?;
        require_count(
            self.get_attr_dilute_initial_volume_ul(ctx).as_deref(),
            "dilute_initial_volume_ul",
            self.loc(ctx),
            ctx,
        )?;
        require_count(
            self.get_attr_dilute_medium_volume_ul(ctx).as_deref(),
            "dilute_medium_volume_ul",
            self.loc(ctx),
            ctx,
        )?;
        require_count(
            self.get_attr_dilute_culture_volume_ul(ctx).as_deref(),
            "dilute_culture_volume_ul",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_operand_culture(ctx),
            "RecoveredCulture",
            self.loc(ctx),
            ctx,
        )?;
        require_material(
            self.get_result_diluted(ctx),
            "DilutedCulture",
            self.loc(ctx),
            ctx,
        )
    }
}

#[pliron_op(
    name = "workflow.plate",
    format,
    attributes = (
        plate_artifact: StringAttr,
        plate_selection: StringAttr,
        plate_replicates: IntegerAttr,
        plate_culture_replicates: IntegerAttr,
        plate_serial_dilutions: IntegerAttr,
        plate_medium_volume_ul: IntegerAttr,
        plate_culture_volume_ul: IntegerAttr,
        plate_colony_volume_ul: IntegerAttr
    ),
    operands = (culture: MaterialType),
    results = (plate: MaterialType)
)]
/// Request plating under an explicit selection condition.
pub struct PlateOp;

impl PlateOp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut Context,
        culture: Value,
        artifact: impl Into<String>,
        selection: impl Into<String>,
        replicates: u8,
        culture_replicates: u8,
        serial_dilutions: u8,
        medium_volume_ul: u16,
        culture_volume_ul: u16,
        colony_volume_ul: u16,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                vec![MaterialType::state(ctx, "Plate")],
                vec![culture],
                vec![],
                0,
            ),
        };
        result.set_attr_plate_artifact(ctx, StringAttr::new(artifact.into()));
        result.set_attr_plate_selection(ctx, StringAttr::new(selection.into()));
        result.set_attr_plate_replicates(ctx, u32_attr(ctx, replicates.into()));
        result.set_attr_plate_culture_replicates(ctx, u32_attr(ctx, culture_replicates.into()));
        result.set_attr_plate_serial_dilutions(ctx, u32_attr(ctx, serial_dilutions.into()));
        result.set_attr_plate_medium_volume_ul(ctx, u32_attr(ctx, medium_volume_ul.into()));
        result.set_attr_plate_culture_volume_ul(ctx, u32_attr(ctx, culture_volume_ul.into()));
        result.set_attr_plate_colony_volume_ul(ctx, u32_attr(ctx, colony_volume_ul.into()));
        result
    }
}

impl Verify for PlateOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_plate_artifact(ctx).as_deref(),
            "plate_artifact",
            self.loc(ctx),
        )?;
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
        for (value, name) in [
            (
                self.get_attr_plate_culture_replicates(ctx),
                "plate_culture_replicates",
            ),
            (
                self.get_attr_plate_serial_dilutions(ctx),
                "plate_serial_dilutions",
            ),
            (
                self.get_attr_plate_medium_volume_ul(ctx),
                "plate_medium_volume_ul",
            ),
            (
                self.get_attr_plate_culture_volume_ul(ctx),
                "plate_culture_volume_ul",
            ),
            (
                self.get_attr_plate_colony_volume_ul(ctx),
                "plate_colony_volume_ul",
            ),
        ] {
            require_count(value.as_deref(), name, self.loc(ctx), ctx)?;
        }
        require_material(
            self.get_operand_culture(ctx),
            "DilutedCulture",
            self.loc(ctx),
            ctx,
        )?;
        require_material(self.get_result_plate(ctx), "Plate", self.loc(ctx), ctx)
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

/// Verify that a value is a material, whatever state it is in.
fn require_any_material(value: Value, what: &str, location: Location, ctx: &Context) -> Result<()> {
    let handle = value.get_type(ctx);
    let ty = handle.deref(ctx);
    if ty.downcast_ref::<MaterialType>().is_none() {
        return verify_err!(location, "{what} must be a Workflow material");
    }
    Ok(())
}

/// Verify that a value is a material in the named state.
///
/// The state is written as a bare name and compared as the IRI it stands for,
/// so a verifier reads as the state a person would say out loud while the
/// comparison stays exact.
fn require_material(value: Value, expected: &str, location: Location, ctx: &Context) -> Result<()> {
    let handle = value.get_type(ctx);
    let ty = handle.deref(ctx);
    let Some(actual) = ty.downcast_ref::<MaterialType>() else {
        return verify_err!(location, "expected Workflow material state '{expected}'");
    };
    let expected_iri = format!("{STATE_NS}{expected}");
    if actual.iri() != expected_iri {
        return verify_err!(
            location,
            "expected Workflow material state '{expected_iri}', found '{}'",
            actual.iri()
        );
    }
    Ok(())
}
