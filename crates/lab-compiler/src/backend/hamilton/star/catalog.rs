//! The vendored Hamilton deck-geometry catalog: carriers, labware, and the
//! volume-to-height models planning needs to place every command's liquid
//! heights. This is the one place STAR deck coordinates live.
//!
//! Geometry composes as deck origin → carrier at a rail → site offset →
//! labware layout → well or tip position. A carrier at rail `n` sits at
//! `x = 100 + (n − 1) × 22.5` mm with its origin at `y = 63`, `z = 100` mm
//! — the deck constants the STAR firmware itself addresses. Every carrier
//! and labware dimension below derives from PyLabRobot's Hamilton resource
//! definitions (MIT License, Copyright (c) 2022 PyLabRobot), the same
//! de-facto specification `hamilton-star` credits for the wire
//! protocol; the specific source resources are named on each definition.

use hamilton_star::catalog::{TIP_300UL_FILTER, TIP_1000UL_FILTER, TipType};
use hamilton_star::commands::system::{TipPickupMethod, TipSizeCode};
use hamilton_star::units::Millimeters;

/// Deck x of rail 1, in millimeters.
pub const RAIL_ONE_X: f64 = 100.0;
/// Rail pitch, in millimeters.
pub const RAIL_PITCH: f64 = 22.5;
/// Deck y of a carrier's origin, in millimeters.
pub const CARRIER_ORIGIN_Y: f64 = 63.0;
/// Deck z of a carrier's origin, in millimeters.
pub const CARRIER_ORIGIN_Z: f64 = 100.0;

/// One carrier site: the origin of the labware it holds, relative to the
/// carrier's origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SiteOffset {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

const fn site(x: f64, y: f64, z: f64) -> SiteOffset {
    SiteOffset { x, y, z }
}

/// A catalog carrier: its footprint in rails and its site offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarrierDefinition {
    /// The name a profile uses to place this carrier.
    pub id: &'static str,
    /// Hamilton's own model designation, for the operator documents.
    pub hamilton_model: &'static str,
    /// How many rails the carrier occupies.
    pub width_rails: u32,
    pub sites: &'static [SiteOffset],
}

/// Tip carrier for five 96-tip racks in landscape (Hamilton TIP_CAR_480_A00,
/// cat. no. 182085; PyLabRobot `TIP_CAR_480_A00`).
pub const TIP_CARRIER_480: CarrierDefinition = CarrierDefinition {
    id: "tip_carrier_480",
    hamilton_model: "TIP_CAR_480_A00",
    width_rails: 6,
    sites: &[
        site(6.2, 10.0, 114.95),
        site(6.2, 106.0, 114.95),
        site(6.2, 202.0, 114.95),
        site(6.2, 298.0, 114.95),
        site(6.2, 394.0, 114.95),
    ],
};

/// Plate carrier for five SBS plates (Hamilton PLT_CAR_L5AC_A00, cat. no.
/// 182090; PyLabRobot `PLT_CAR_L5AC_A00`). The site z is the pedestal top a
/// non-skirted plate rests on.
pub const PLATE_CARRIER_L5: CarrierDefinition = CarrierDefinition {
    id: "plate_carrier_l5",
    hamilton_model: "PLT_CAR_L5AC_A00",
    width_rails: 6,
    sites: &[
        site(4.0, 8.5, 86.15),
        site(4.0, 104.5, 86.15),
        site(4.0, 200.5, 86.15),
        site(4.0, 296.5, 86.15),
        site(4.0, 392.5, 86.15),
    ],
};

/// Sample carrier for 24 tubes (Hamilton SMP_CAR_24_A00, cat. no. 173400;
/// PyLabRobot `Tube_CAR_24_A00`). One site holding the 24-position strip;
/// the strip's linear layout lives in the tube-rack labware definition.
pub const TUBE_CARRIER_24: CarrierDefinition = CarrierDefinition {
    id: "tube_carrier_24",
    hamilton_model: "SMP_CAR_24_A00",
    width_rails: 1,
    sites: &[site(3.0, 9.0, 11.2)],
};

/// Reagent carrier for five 60 mL troughs (Hamilton RGT_CAR5X60, cat. no.
/// 53646-01; PyLabRobot `Trough_CAR_5R60_A00`).
pub const TROUGH_CARRIER_5: CarrierDefinition = CarrierDefinition {
    id: "trough_carrier_5",
    hamilton_model: "RGT_CAR5X60",
    width_rails: 1,
    sites: &[
        site(1.5, 7.0, 63.5),
        site(1.5, 103.0, 63.5),
        site(1.5, 199.0, 63.5),
        site(1.5, 295.0, 63.5),
        site(1.5, 391.0, 63.5),
    ],
};

/// Every carrier the catalog offers, for profile resolution and validation
/// messages.
pub const CARRIERS: [&CarrierDefinition; 4] = [
    &TIP_CARRIER_480,
    &PLATE_CARRIER_L5,
    &TUBE_CARRIER_24,
    &TROUGH_CARRIER_5,
];

/// Looks a catalog carrier up by its profile-facing id.
pub fn carrier(id: &str) -> Option<&'static CarrierDefinition> {
    CARRIERS.iter().copied().find(|carrier| carrier.id == id)
}

/// How a labware's positions map to the shared column-major well names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabwareLayout {
    /// An SBS grid: row A carries the highest y, columns advance in x. The
    /// coordinates are the center of well A1 relative to the site origin.
    Grid {
        rows: usize,
        columns: usize,
        a1_x: f64,
        a1_y: f64,
        pitch: f64,
    },
    /// A linear strip advancing in y (the 24-tube carrier). Positions are
    /// addressed with the column-major grid names of the same capacity, so
    /// the shared allocator's `A1` is position 0, `B1` position 1, and so
    /// on down the strip.
    Linear {
        positions: usize,
        first_x: f64,
        first_y: f64,
        spacing: f64,
    },
    /// A single vessel (a trough); every access hits its center.
    Single { x: f64, y: f64 },
}

/// The model converting a held volume to a liquid height above the vessel
/// bottom, in millimeters. Each is documented on the labware that uses it;
/// where the model approximates (a cylinder standing in for a conical
/// tube), the planning margins absorb the error and the aspirate floor
/// clamps the result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeightModel {
    /// A vertical cylinder: `h = 4V / (π d²)`, µL as mm³.
    Cylinder { diameter: f64 },
    /// A fitted quartic `h(v) = c0 + c1·v + c2·v² + c3·v³ + c4·v⁴` from
    /// measured calibration data.
    Fitted4 { coefficients: [f64; 5] },
    /// Piecewise-linear interpolation over measured `(volume µL, height
    /// mm)` calibration points, extrapolating on the nearest segment.
    Table { points: &'static [(f64, f64)] },
}

impl HeightModel {
    /// The liquid height for a held volume, floored at zero.
    pub fn height_at(&self, volume_ul: f64) -> f64 {
        let height = match self {
            HeightModel::Cylinder { diameter } => {
                4.0 * volume_ul / (std::f64::consts::PI * diameter * diameter)
            }
            HeightModel::Fitted4 { coefficients } => {
                let [c0, c1, c2, c3, c4] = coefficients;
                c0 + c1 * volume_ul
                    + c2 * volume_ul.powi(2)
                    + c3 * volume_ul.powi(3)
                    + c4 * volume_ul.powi(4)
            }
            HeightModel::Table { points } => interpolate_table(points, volume_ul),
        };
        height.max(0.0)
    }
}

fn interpolate_table(points: &[(f64, f64)], volume: f64) -> f64 {
    let segment = points
        .windows(2)
        .find(|pair| volume <= pair[1].0)
        .unwrap_or_else(|| &points[points.len() - 2..]);
    let (v0, h0) = segment[0];
    let (v1, h1) = segment[1];
    if v1 == v0 {
        return h0;
    }
    h0 + (volume - v0) * (h1 - h0) / (v1 - v0)
}

/// What a labware is for: holding liquid or feeding tips.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabwareRole {
    /// A liquid vessel; heights derive from the model and the held volume.
    Vessel {
        /// The liquid bottom above the site origin, mm (material
        /// thickness).
        bottom_above_site: f64,
        /// Vessel depth from liquid bottom to rim, mm.
        depth: f64,
        /// The volume planning may draw against, µL.
        working_volume_ul: f64,
        height_model: HeightModel,
    },
    /// A tip rack; `spot_z_offset` is where a tip's point rests, relative
    /// to the site origin, and `tip` is the driver-crate tip the rack
    /// feeds.
    TipRack { spot_z_offset: f64, tip: TipType },
}

/// A catalog labware.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabwareDefinition {
    /// The name a profile uses to place this labware.
    pub id: &'static str,
    /// The manufacturer designation, for the operator documents.
    pub display: &'static str,
    /// Addressable positions, matching the shared column-major naming.
    pub capacity: usize,
    pub layout: LabwareLayout,
    pub role: LabwareRole,
}

/// The Hamilton 50 µL filter tip (cat. no. 235948; PyLabRobot
/// `hamilton_tip_50uL_filter`): 50.4 mm, 60 µL maximal, standard collar.
pub const TIP_50UL_FILTER: TipType = TipType {
    total_length: Millimeters(50.4),
    max_volume: 60.0,
    size: TipSizeCode::Standard,
    has_filter: true,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 96 × 50 µL filter tips (Hamilton cat. no. 235829; PyLabRobot
/// `hamilton_96_tiprack_50uL_filter`).
pub const TIP_RACK_50UL: LabwareDefinition = LabwareDefinition {
    id: "tip_rack_50ul_filter",
    display: "Hamilton 96 × 50 µL filter tips",
    capacity: 96,
    layout: LabwareLayout::Grid {
        rows: 8,
        columns: 12,
        a1_x: 11.7,
        a1_y: 72.8,
        pitch: 9.0,
    },
    role: LabwareRole::TipRack {
        spot_z_offset: -40.5,
        tip: TIP_50UL_FILTER,
    },
};

/// 96 × 300 µL filter tips (Hamilton STF, cat. no. 235830; PyLabRobot
/// `hamilton_96_tiprack_300uL_filter`).
pub const TIP_RACK_300UL: LabwareDefinition = LabwareDefinition {
    id: "tip_rack_300ul_filter",
    display: "Hamilton 96 × 300 µL filter tips (STF)",
    capacity: 96,
    layout: LabwareLayout::Grid {
        rows: 8,
        columns: 12,
        a1_x: 11.7,
        a1_y: 72.8,
        pitch: 9.0,
    },
    role: LabwareRole::TipRack {
        spot_z_offset: -50.5,
        tip: TIP_300UL_FILTER,
    },
};

/// 96 × 1000 µL filter tips (Hamilton HTF, cat. no. 235905; PyLabRobot
/// `hamilton_96_tiprack_1000uL_filter`).
pub const TIP_RACK_1000UL: LabwareDefinition = LabwareDefinition {
    id: "tip_rack_1000ul_filter",
    display: "Hamilton 96 × 1000 µL filter tips (HTF)",
    capacity: 96,
    layout: LabwareLayout::Grid {
        rows: 8,
        columns: 12,
        a1_x: 11.7,
        a1_y: 72.8,
        pitch: 9.0,
    },
    role: LabwareRole::TipRack {
        spot_z_offset: -83.5,
        tip: TIP_1000UL_FILTER,
    },
};

/// Eppendorf twin.tec 96-well 250 µL PCR plate, cat. no. 0030133374
/// (PyLabRobot `Eppendorf_96_wellplate_250ul_Vb`, including its measured
/// height-from-volume fit). Non-skirted: it rests on the plate carrier's
/// pedestal top.
pub const PCR_PLATE_96: LabwareDefinition = LabwareDefinition {
    id: "pcr_plate_96",
    display: "Eppendorf twin.tec 96-well 250 µL PCR plate",
    capacity: 96,
    layout: LabwareLayout::Grid {
        rows: 8,
        columns: 12,
        a1_x: 9.5,
        a1_y: 74.0,
        pitch: 9.0,
    },
    role: LabwareRole::Vessel {
        bottom_above_site: 1.2,
        depth: 19.5,
        working_volume_ul: 250.0,
        height_model: HeightModel::Fitted4 {
            coefficients: [
                0.118078503,
                0.133333914,
                -0.000802726227,
                3.29761957e-6,
                -5.29119614e-9,
            ],
        },
    },
};

/// 24 conical-bottom sample tubes (14.5 × 60 mm) in the SMP_CAR_24 strip.
/// The cylinder model approximates the conical bottom; it underestimates
/// low-volume heights, which only ever drives the aspirate position deeper,
/// where the bottom standoff clamps it.
pub const SAMPLE_TUBES_24: LabwareDefinition = LabwareDefinition {
    id: "sample_tubes_24",
    display: "24 × 14.5 × 60 mm conical sample tubes",
    capacity: 24,
    layout: LabwareLayout::Linear {
        positions: 24,
        first_x: 9.0,
        first_y: 9.0,
        spacing: 20.0,
    },
    role: LabwareRole::Vessel {
        bottom_above_site: 0.0,
        depth: 55.0,
        working_volume_ul: 4000.0,
        height_model: HeightModel::Cylinder { diameter: 12.4 },
    },
};

/// Calibration points for the Hamilton 60 mL trough, from PyLabRobot's
/// ztouch-probed `hamilton_1_trough_60mL_Vb` height/volume table.
const TROUGH_60ML_HEIGHTS: [(f64, f64); 12] = [
    (0.0, 0.0),
    (500.0, 2.2),
    (1_000.0, 3.5),
    (2_000.0, 4.7),
    (4_000.0, 6.3),
    (6_000.0, 7.5),
    (10_000.0, 10.4),
    (20_000.0, 18.0),
    (30_000.0, 25.3),
    (45_000.0, 35.6),
    (60_000.0, 45.7),
    (80_000.0, 58.5),
];

/// Hamilton 60 mL V-bottom trough, cat. no. 56694-01 (PyLabRobot
/// `hamilton_1_trough_60mL_Vb`).
pub const TROUGH_60ML: LabwareDefinition = LabwareDefinition {
    id: "trough_60ml",
    display: "Hamilton 60 mL trough",
    capacity: 1,
    layout: LabwareLayout::Single { x: 9.5, y: 45.0 },
    role: LabwareRole::Vessel {
        bottom_above_site: 1.0,
        depth: 60.0,
        working_volume_ul: 60_000.0,
        height_model: HeightModel::Table {
            points: &TROUGH_60ML_HEIGHTS,
        },
    },
};

/// Every labware the catalog offers.
pub const LABWARE: [&LabwareDefinition; 6] = [
    &TIP_RACK_50UL,
    &TIP_RACK_300UL,
    &TIP_RACK_1000UL,
    &PCR_PLATE_96,
    &SAMPLE_TUBES_24,
    &TROUGH_60ML,
];

/// Looks a catalog labware up by its profile-facing id.
pub fn labware(id: &str) -> Option<&'static LabwareDefinition> {
    LABWARE.iter().copied().find(|labware| labware.id == id)
}

/// A resolved deck position: the well (or tip-spot) center in deck
/// millimeters, and the liquid bottom (or tip-spot plane) z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckPosition {
    pub x: f64,
    pub y: f64,
    /// For a vessel: the liquid bottom z. For a tip rack: the plane a
    /// tip's point rests on.
    pub z: f64,
}

/// Parses a column-major well name (`A1`..) against a capacity's grid,
/// returning `(row, column)` zero-based.
pub fn parse_well(name: &str, rows: usize, columns: usize) -> Option<(usize, usize)> {
    let mut chars = name.chars();
    let row_letter = chars.next()?;
    let row = (row_letter as usize).checked_sub('A' as usize)?;
    let column: usize = chars.as_str().parse().ok()?;
    (row < rows && (1..=columns).contains(&column)).then_some((row, column - 1))
}

/// The `(rows, columns)` grid the shared well naming uses for a capacity.
pub fn grid_for_capacity(capacity: usize) -> Option<(usize, usize)> {
    match capacity {
        15 => Some((3, 5)),
        24 => Some((4, 6)),
        48 => Some((6, 8)),
        96 => Some((8, 12)),
        384 => Some((16, 24)),
        1 => Some((1, 1)),
        _ => None,
    }
}

impl LabwareDefinition {
    /// The named position's offset from the site origin: `(x, y)` center
    /// in millimeters.
    fn position_offset(&self, well: &str) -> Option<(f64, f64)> {
        match self.layout {
            LabwareLayout::Grid {
                rows,
                columns,
                a1_x,
                a1_y,
                pitch,
            } => {
                let (row, column) = parse_well(well, rows, columns)?;
                Some((a1_x + column as f64 * pitch, a1_y - row as f64 * pitch))
            }
            LabwareLayout::Linear {
                positions,
                first_x,
                first_y,
                spacing,
            } => {
                let (rows, columns) = grid_for_capacity(positions)?;
                let (row, column) = parse_well(well, rows, columns)?;
                let index = column * rows + row;
                Some((first_x, first_y + index as f64 * spacing))
            }
            LabwareLayout::Single { x, y } => (well == "A1").then_some((x, y)),
        }
    }

    /// The liquid-geometry role, when this labware is a vessel.
    pub fn vessel(&self) -> Option<(f64, f64, f64, HeightModel)> {
        match self.role {
            LabwareRole::Vessel {
                bottom_above_site,
                depth,
                working_volume_ul,
                height_model,
            } => Some((bottom_above_site, depth, working_volume_ul, height_model)),
            LabwareRole::TipRack { .. } => None,
        }
    }

    /// The tip the rack feeds, when this labware is a tip rack.
    pub fn tip(&self) -> Option<TipType> {
        match self.role {
            LabwareRole::TipRack { tip, .. } => Some(tip),
            LabwareRole::Vessel { .. } => None,
        }
    }
}

/// Resolves a well (or tip spot) to deck millimeters: carrier at its rail,
/// site, labware layout, and the role's z reference.
pub fn well_position(
    carrier: &CarrierDefinition,
    rail: u32,
    site_index: usize,
    labware: &LabwareDefinition,
    well: &str,
) -> Option<DeckPosition> {
    let site = carrier.sites.get(site_index)?;
    let (well_x, well_y) = labware.position_offset(well)?;
    let z = match labware.role {
        LabwareRole::Vessel {
            bottom_above_site, ..
        } => CARRIER_ORIGIN_Z + site.z + bottom_above_site,
        LabwareRole::TipRack { spot_z_offset, .. } => CARRIER_ORIGIN_Z + site.z + spot_z_offset,
    };
    Some(DeckPosition {
        x: RAIL_ONE_X + f64::from(rail - 1) * RAIL_PITCH + site.x + well_x,
        y: CARRIER_ORIGIN_Y + site.y + well_y,
        z,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_rack_a1_on_site_two_reproduces_the_golden_tp_coordinates() {
        // The driver crate's golden TP frame picks tips at x 117.9 mm,
        // y 241.8 mm from a 300 µL rack: rail 1 (x = 100), site index 1
        // (y offset 106), A1 at (11.7, 72.8) within the rack.
        let position = well_position(&TIP_CARRIER_480, 1, 1, &TIP_RACK_300UL, "A1")
            .expect("site 2 exists and A1 is a rack position");
        assert!(
            (position.x - 117.9).abs() < 1e-9,
            "x = 100 + 6.2 + 11.7 = 117.9, got {}",
            position.x
        );
        assert!(
            (position.y - 241.8).abs() < 1e-9,
            "y = 63 + 106 + 72.8 = 241.8, got {}",
            position.y
        );
        assert!(
            (position.z - 164.45).abs() < 1e-9,
            "the tip point rests at 100 + 114.95 − 50.5 = 164.45 mm, got {}",
            position.z
        );
    }

    #[test]
    fn b1_sits_nine_millimeters_in_front_of_a1() {
        let a1 = well_position(&TIP_CARRIER_480, 1, 1, &TIP_RACK_300UL, "A1").expect("A1 resolves");
        let b1 = well_position(&TIP_CARRIER_480, 1, 1, &TIP_RACK_300UL, "B1").expect("B1 resolves");
        assert!(
            (a1.y - b1.y - 9.0).abs() < 1e-9,
            "the golden TP frame's second channel sits at 232.8 = 241.8 − 9"
        );
    }

    #[test]
    fn pcr_plate_a1_resolves_through_the_plate_carrier() {
        // Rail 14, site 1: x = 100 + 13 × 22.5 + 4.0 + 9.5 = 406.0;
        // y = 63 + 8.5 + 74.0 = 145.5; bottom z = 100 + 86.15 + 1.2.
        let position = well_position(&PLATE_CARRIER_L5, 14, 0, &PCR_PLATE_96, "A1")
            .expect("A1 resolves on the first plate site");
        assert!((position.x - 406.0).abs() < 1e-9, "got x {}", position.x);
        assert!((position.y - 145.5).abs() < 1e-9, "got y {}", position.y);
        assert!((position.z - 187.35).abs() < 1e-9, "got z {}", position.z);
    }

    #[test]
    fn tube_positions_advance_down_the_strip_in_column_major_name_order() {
        let a1 = well_position(&TUBE_CARRIER_24, 8, 0, &SAMPLE_TUBES_24, "A1")
            .expect("A1 is tube position 0");
        let b1 = well_position(&TUBE_CARRIER_24, 8, 0, &SAMPLE_TUBES_24, "B1")
            .expect("B1 is tube position 1");
        let a2 = well_position(&TUBE_CARRIER_24, 8, 0, &SAMPLE_TUBES_24, "A2")
            .expect("A2 is tube position 4");
        assert!(
            (b1.y - a1.y - 20.0).abs() < 1e-9,
            "adjacent name positions are one 20 mm tube apart"
        );
        assert!(
            (a2.y - a1.y - 80.0).abs() < 1e-9,
            "A2 follows D1: four 20 mm positions down the strip"
        );
        assert!(
            (a1.x - (100.0 + 7.0 * 22.5 + 3.0 + 9.0)).abs() < 1e-9,
            "tube centers sit 12 mm into the one-rail carrier at rail 8"
        );
    }

    #[test]
    fn the_pcr_plate_height_fit_matches_its_calibration_data() {
        let (_, _, _, model) = PCR_PLATE_96.vessel().expect("the plate holds liquid");
        let at_120 = model.height_at(120.0);
        assert!(
            (at_120 - 9.177).abs() < 0.05,
            "the calibration table observed 9.05 mm and predicted 9.177 mm at 120 µL, got {at_120}"
        );
        assert_eq!(
            model.height_at(0.0),
            0.118078503,
            "an empty well reads the fit's intercept"
        );
    }

    #[test]
    fn the_trough_table_interpolates_between_calibration_points() {
        let (_, _, _, model) = TROUGH_60ML.vessel().expect("the trough holds liquid");
        let mid = model.height_at(3_000.0);
        assert!(
            (mid - 5.5).abs() < 1e-9,
            "3 mL interpolates halfway between the 2 mL and 4 mL points, got {mid}"
        );
    }

    #[test]
    fn cylinder_heights_scale_linearly_with_volume() {
        let model = HeightModel::Cylinder { diameter: 12.4 };
        let one_ml = model.height_at(1000.0);
        assert!(
            (one_ml - 8.281).abs() < 0.01,
            "1 mL in a 12.4 mm cylinder stands 8.28 mm, got {one_ml}"
        );
        assert!(
            (model.height_at(2000.0) - 2.0 * one_ml).abs() < 1e-9,
            "a cylinder's height is linear in volume"
        );
    }

    #[test]
    fn unknown_wells_and_sites_resolve_to_none() {
        assert_eq!(
            well_position(&PLATE_CARRIER_L5, 14, 0, &PCR_PLATE_96, "I1"),
            None,
            "row I does not exist on an 8-row plate"
        );
        assert_eq!(
            well_position(&PLATE_CARRIER_L5, 14, 5, &PCR_PLATE_96, "A1"),
            None,
            "the plate carrier has five sites"
        );
        assert_eq!(
            well_position(&TROUGH_CARRIER_5, 3, 0, &TROUGH_60ML, "B1"),
            None,
            "a trough has only its center, addressed as A1"
        );
    }
}
