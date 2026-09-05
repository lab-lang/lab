//! Reference-bench default values for every profile field.

use crate::backend::resources::PlateCapacity;

use crate::backend::opentrons::ot2::profile::schema::{
    Pipette, TechniqueCalibration, TemperatureModule, Thermocycler, TipRacks,
};

pub(super) fn default_api_level() -> String {
    "2.21".to_owned()
}

pub(super) fn default_aspiration_rate() -> f64 {
    0.5
}

pub(super) fn default_dispense_rate() -> f64 {
    1.0
}

pub(super) fn default_tracked_source_volume_ul() -> u32 {
    10_000
}

pub(super) fn default_tracked_meniscus_offset_mm() -> f64 {
    10.0
}

pub(super) fn default_tracked_usable_depth_offset_mm() -> f64 {
    10.0
}

pub(super) fn default_tracked_minimum_height_mm() -> f64 {
    3.0
}

pub(super) fn default_above_liquid_offset_mm() -> f64 {
    2.0
}

pub(super) fn default_material_surface_offset_mm() -> f64 {
    -8.0
}

pub(super) fn default_touch_tip_radius() -> f64 {
    0.5
}

pub(super) fn default_touch_tip_vertical_offset_mm() -> f64 {
    -14.0
}

pub(super) fn default_touch_tip_speed_mm_s() -> f64 {
    20.0
}

pub(super) fn default_technique_calibration() -> TechniqueCalibration {
    TechniqueCalibration {
        aspiration_rate: default_aspiration_rate(),
        dispense_rate: default_dispense_rate(),
        tracked_source_volume_ul: default_tracked_source_volume_ul(),
        tracked_meniscus_offset_mm: default_tracked_meniscus_offset_mm(),
        tracked_usable_depth_offset_mm: default_tracked_usable_depth_offset_mm(),
        tracked_minimum_height_mm: default_tracked_minimum_height_mm(),
        above_liquid_offset_mm: default_above_liquid_offset_mm(),
        material_surface_offset_mm: default_material_surface_offset_mm(),
        touch_tip_radius: default_touch_tip_radius(),
        touch_tip_vertical_offset_mm: default_touch_tip_vertical_offset_mm(),
        touch_tip_speed_mm_s: default_touch_tip_speed_mm_s(),
    }
}

pub(super) fn default_small_pipette() -> Pipette {
    Pipette {
        model: "p20_single_gen2".to_owned(),
        mount: "left".to_owned(),
    }
}

pub(super) fn default_large_pipette() -> Pipette {
    Pipette {
        model: "p300_single_gen2".to_owned(),
        mount: "right".to_owned(),
    }
}

pub(super) fn default_sources() -> TemperatureModule {
    TemperatureModule {
        model: "temperature module gen2".to_owned(),
        slot: "1".to_owned(),
        labware: "opentrons_24_aluminumblock_nest_1.5ml_snapcap".to_owned(),
        capacity: plate_capacity(24),
    }
}

pub(super) fn default_work() -> Thermocycler {
    Thermocycler {
        model: "thermocycler module gen2".to_owned(),
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        capacity: plate_capacity(96),
    }
}

pub(super) fn default_plate_capacity() -> PlateCapacity {
    plate_capacity(96)
}

pub(super) fn default_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_tiprack_20ul".to_owned(),
        slots: vec!["2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_large_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_200ul".to_owned(),
        slots: vec!["6".to_owned()],
        capacity: default_plate_capacity(),
    }
}

/// A literal geometry this compiler ships as a default.
fn plate_capacity(capacity: usize) -> PlateCapacity {
    PlateCapacity::new(capacity).expect("built-in defaults declare addressable geometries")
}
