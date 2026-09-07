//! Deserializable OT-2 configuration for the physical resources consumed by canonical programs.

use schemars::JsonSchema;

use crate::backend::resources::PlateCapacity;
use serde::{Deserialize, Serialize};

pub use crate::backend::profile::TipRacks;

use crate::backend::opentrons::ot2::profile::defaults::*;

/// Calibrated OT-2 realization policy for portable pipetting techniques.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TechniqueCalibration {
    #[serde(default = "default_aspiration_rate")]
    pub aspiration_rate: f64,
    /// Source mixing and multi-dispense loads have independent calibrated rates.
    #[serde(default = "default_dispense_rate")]
    pub mix_aspiration_rate: f64,
    #[serde(default = "default_dispense_rate")]
    pub distribution_aspiration_rate: f64,
    #[serde(default = "default_dispense_rate")]
    pub dispense_rate: f64,
    #[serde(default = "default_tracked_source_volume_ul")]
    pub tracked_source_volume_ul: u32,
    #[serde(default = "default_tracked_meniscus_offset_mm")]
    pub tracked_meniscus_offset_mm: f64,
    #[serde(default = "default_tracked_usable_depth_offset_mm")]
    pub tracked_usable_depth_offset_mm: f64,
    #[serde(default = "default_tracked_minimum_height_mm")]
    pub tracked_minimum_height_mm: f64,
    #[serde(default = "default_above_liquid_offset_mm")]
    pub above_liquid_offset_mm: f64,
    #[serde(default = "default_material_surface_offset_mm")]
    pub material_surface_offset_mm: f64,
    #[serde(default = "default_touch_tip_radius")]
    pub touch_tip_radius: f64,
    #[serde(default = "default_touch_tip_vertical_offset_mm")]
    pub touch_tip_vertical_offset_mm: f64,
    #[serde(default = "default_touch_tip_speed_mm_s")]
    pub touch_tip_speed_mm_s: f64,
}

impl Default for TechniqueCalibration {
    fn default() -> Self {
        default_technique_calibration()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProtocolOptions {
    /// Opentrons Python Protocol API version emitted by this adapter.
    #[serde(default = "default_api_level")]
    pub api_level: String,
}

impl Default for ProtocolOptions {
    fn default() -> Self {
        Self {
            api_level: default_api_level(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Instruments {
    #[serde(default = "default_small_pipette")]
    pub small: Pipette,
    #[serde(default = "default_large_pipette")]
    pub large: Pipette,
}

impl Default for Instruments {
    fn default() -> Self {
        Self {
            small: default_small_pipette(),
            large: default_large_pipette(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pipette {
    pub model: String,
    pub mount: String,
}

/// Physical resources loaded for one canonical program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ot2Resources {
    #[serde(default = "default_sources")]
    pub sources: TemperatureModule,
    #[serde(default = "default_work")]
    pub work: Thermocycler,
    #[serde(default = "default_bulk")]
    pub bulk: DeckLabware,
    #[serde(default = "default_surface")]
    pub surface: DeckLabware,
    #[serde(default = "default_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_large_tips")]
    pub large_tips: TipRacks,
}

impl Default for Ot2Resources {
    fn default() -> Self {
        Self {
            sources: default_sources(),
            work: default_work(),
            bulk: default_bulk(),
            surface: default_surface(),
            small_tips: default_small_tips(),
            large_tips: default_large_tips(),
        }
    }
}

impl Default for TemperatureModule {
    fn default() -> Self {
        default_sources()
    }
}

impl Default for Thermocycler {
    fn default() -> Self {
        default_work()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemperatureModule {
    pub model: String,
    pub slot: String,
    /// Addressable source labware carried on the module.
    pub labware: String,
    pub capacity: PlateCapacity,
    /// Reviewed working volume for one physical position, in microlitres.
    #[serde(default = "default_source_volume_limit")]
    pub max_volume_each_ul: u32,
}

/// The thermocycler-backed work area. It occupies fixed slots, so it declares no slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Thermocycler {
    pub model: String,
    pub labware: String,
    pub capacity: PlateCapacity,
    /// Reviewed working volume for one physical position, in microlitres.
    #[serde(default = "default_work_volume_limit")]
    pub max_volume_each_ul: u32,
}

/// A passive deck resource for bulk liquids or material-surface dispensing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckLabware {
    pub slot: String,
    pub labware: String,
    pub capacity: PlateCapacity,
    /// Reviewed working volume for one physical position, in microlitres.
    pub max_volume_each_ul: u32,
}
