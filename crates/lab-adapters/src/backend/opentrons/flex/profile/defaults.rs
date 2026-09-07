//! Reference-bench default values for every Flex profile field.

use crate::backend::resources::PlateCapacity;

use crate::backend::opentrons::flex::profile::schema::{
    DeckLabware, Pipette, TemperatureModule, Thermocycler, TipRacks, Trash,
};

pub(super) fn default_small_pipette() -> Pipette {
    Pipette {
        model: "p50_single_flex".to_owned(),
        mount: "left".to_owned(),
    }
}

pub(super) fn default_large_pipette() -> Pipette {
    Pipette {
        model: "p1000_single_flex".to_owned(),
        mount: "right".to_owned(),
    }
}

pub(super) fn default_sources() -> TemperatureModule {
    TemperatureModule {
        model: "temperatureModuleV2".to_owned(),
        slot: "C1".to_owned(),
        labware: "opentrons_24_aluminumblock_nest_1.5ml_snapcap".to_owned(),
        max_volume_each_ul: 1500,
        capacity: plate_capacity(24),
    }
}

pub(super) fn default_work() -> Thermocycler {
    Thermocycler {
        model: "thermocyclerModuleV2".to_owned(),
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        max_volume_each_ul: 100,
        capacity: plate_capacity(96),
    }
}

pub(super) fn default_trash() -> Trash {
    Trash {
        area: "movableTrashA3".to_owned(),
    }
}

pub(super) fn default_plate_capacity() -> PlateCapacity {
    plate_capacity(96)
}

pub(super) fn default_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_flex_96_tiprack_50ul".to_owned(),
        slots: vec!["C2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_large_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_flex_96_tiprack_1000ul".to_owned(),
        slots: vec!["D2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

/// A literal geometry this compiler ships as a default.
fn plate_capacity(capacity: usize) -> PlateCapacity {
    PlateCapacity::new(capacity).expect("built-in defaults declare addressable geometries")
}

/// A 15 mL conical carrying 10 mL of medium, the reference Flex bench.
pub(super) fn default_flex_tracked_source_volume_ul() -> u32 {
    10_000
}

pub(super) fn default_flex_tracked_usable_depth_mm() -> f64 {
    75.0
}

pub(super) fn default_flex_tracked_meniscus_offset_mm() -> f64 {
    3.0
}

pub(super) fn default_flex_tracked_minimum_height_mm() -> f64 {
    2.0
}

pub(super) fn default_bulk() -> DeckLabware {
    DeckLabware {
        slot: "D1".to_owned(),
        labware: "opentrons_15_tuberack_falcon_15ml_conical".to_owned(),
        max_volume_each_ul: 15000,
        capacity: plate_capacity(15),
    }
}

pub(super) fn default_source_volume_limit() -> u32 {
    1500
}
pub(super) fn default_work_volume_limit() -> u32 {
    100
}
