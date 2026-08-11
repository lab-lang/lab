//! Nominal extents for scene geometry.
//!
//! The planning catalog carries anchor points — rails, site offsets, well
//! centers — because that is what motion needs. Rendering needs bounding
//! boxes, so this table states them separately, as nominal visualization
//! dimensions: good enough to recognize the bench at a glance, never used
//! for planning. Positions stay exact either way.

use lab_compiler::backend::hamilton::star::catalog::{
    self, CarrierDefinition, LabwareDefinition, LabwareLayout,
};

/// The SLAS/ANSI microplate footprint every plate and tip rack shares.
pub const SLAS_FOOTPRINT_X_MM: f64 = 127.76;
pub const SLAS_FOOTPRINT_Y_MM: f64 = 85.48;

/// Nominal STAR carrier length, front to back.
pub const CARRIER_LENGTH_MM: f64 = 497.0;
/// Nominal carrier tray thickness.
pub const CARRIER_HEIGHT_MM: f64 = 18.0;
/// Nominal deck plate depth and thickness.
pub const DECK_DEPTH_MM: f64 = 600.0;
pub const DECK_THICKNESS_MM: f64 = 15.0;

/// A carrier's footprint: exact width from its rail span, nominal length
/// and height.
pub fn carrier_extent(definition: &CarrierDefinition) -> [f64; 3] {
    [
        f64::from(definition.width_rails) * catalog::RAIL_PITCH,
        CARRIER_LENGTH_MM,
        CARRIER_HEIGHT_MM,
    ]
}

/// A labware's footprint and height. Known catalog ids get stated
/// dimensions; anything else derives a footprint from its layout pitch so
/// new labware never renders as nothing.
pub fn labware_extent(definition: &LabwareDefinition) -> [f64; 3] {
    match definition.id {
        "tip_rack_50ul" => [SLAS_FOOTPRINT_X_MM, SLAS_FOOTPRINT_Y_MM, 60.0],
        "tip_rack_300ul" => [SLAS_FOOTPRINT_X_MM, SLAS_FOOTPRINT_Y_MM, 72.0],
        "tip_rack_1000ul" => [SLAS_FOOTPRINT_X_MM, SLAS_FOOTPRINT_Y_MM, 95.0],
        "pcr_plate_96" => [SLAS_FOOTPRINT_X_MM, SLAS_FOOTPRINT_Y_MM, 16.1],
        "sample_tubes_24" => [SLAS_FOOTPRINT_X_MM, SLAS_FOOTPRINT_Y_MM, 62.0],
        "trough_60ml" => [35.0, 120.0, 45.0],
        _ => derived_extent(definition),
    }
}

fn derived_extent(definition: &LabwareDefinition) -> [f64; 3] {
    let height = definition
        .vessel()
        .map(|(bottom, depth, _, _)| bottom + depth + 3.0)
        .unwrap_or(50.0);
    match definition.layout {
        LabwareLayout::Grid {
            rows,
            columns,
            pitch,
            ..
        } => [
            (columns as f64 + 1.0) * pitch,
            (rows as f64 + 1.0) * pitch,
            height,
        ],
        LabwareLayout::Linear {
            positions, spacing, ..
        } => [(positions as f64 + 1.0) * spacing, 40.0, height],
        LabwareLayout::Single { .. } => [40.0, 120.0, height],
    }
}

/// A well's cylinder diameter: exact when the catalog's height model is a
/// cylinder, otherwise a nominal fraction of the well pitch.
pub fn well_diameter(definition: &LabwareDefinition) -> f64 {
    use lab_compiler::backend::hamilton::star::catalog::{HeightModel, LabwareRole};
    if let LabwareRole::Vessel {
        height_model: HeightModel::Cylinder { diameter },
        ..
    } = definition.role
    {
        return diameter;
    }
    match definition.layout {
        LabwareLayout::Grid { pitch, .. } => pitch * 0.7,
        LabwareLayout::Linear { spacing, .. } => spacing * 0.7,
        LabwareLayout::Single { .. } => 25.0,
    }
}

/// A well's cylinder height: its vessel depth, or a tip's nominal length.
pub fn well_height(definition: &LabwareDefinition) -> f64 {
    definition
        .vessel()
        .map(|(_, depth, _, _)| depth)
        .unwrap_or(45.0)
}
