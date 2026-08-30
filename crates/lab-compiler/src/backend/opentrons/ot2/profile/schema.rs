//! Deserializable shape of OT-2 adapter configuration: protocol options, instruments, deck modules, and the labware each build stage claims.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use crate::backend::profile::{MediaRack, Plates, SourceRack, TipRacks};

use crate::backend::opentrons::ot2::profile::defaults::*;

/// Calibrated OT-2 realization policy for portable pipetting techniques.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TechniqueCalibration {
    #[serde(default = "default_aspiration_rate")]
    pub aspiration_rate: f64,
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
    #[serde(default = "default_tracked_low_volume_fraction")]
    pub tracked_low_volume_fraction: f64,
    #[serde(default = "default_tracked_chunk_size")]
    pub tracked_chunk_size: usize,
    #[serde(default = "default_distribution_disposal_volume_ul")]
    pub distribution_disposal_volume_ul: u32,
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

/// Hardware present for every stage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SharedDeck {
    #[serde(default = "default_temperature_module")]
    pub temperature_module: TemperatureModule,
    #[serde(default = "default_thermocycler")]
    pub thermocycler: Thermocycler,
}

impl Default for TemperatureModule {
    fn default() -> Self {
        default_temperature_module()
    }
}

impl Default for Thermocycler {
    fn default() -> Self {
        default_thermocycler()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemperatureModule {
    pub model: String,
    pub slot: String,
    /// Rack of chilled source tubes carried on the module.
    pub labware: String,
    pub capacity: usize,
}

/// The thermocycler occupies fixed slots, so it declares no slot of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Thermocycler {
    pub model: String,
    pub labware: String,
    pub capacity: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stages {
    #[serde(default = "default_assembly_stage")]
    pub assembly: AssemblyStage,
    #[serde(default = "default_transformation_stage")]
    pub transformation: TransformationStage,
    #[serde(default = "default_plating_stage")]
    pub plating: PlatingStage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssemblyStage {
    #[serde(default = "default_assembly_small_tips")]
    pub small_tips: TipRacks,
}

impl Default for AssemblyStage {
    fn default() -> Self {
        default_assembly_stage()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransformationStage {
    /// Plate holding the assembled plasmids a transformation draws from.
    #[serde(default = "default_transformation_dna_plate")]
    pub dna_plate: Plates,
    /// Rack holding competent-cell sources and recovery medium.
    #[serde(default = "default_transformation_source_rack")]
    pub source_rack: SourceRack,
    #[serde(default = "default_transformation_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_transformation_large_tips")]
    pub large_tips: TipRacks,
}

impl Default for TransformationStage {
    fn default() -> Self {
        default_transformation_stage()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatingStage {
    #[serde(default = "default_dilution_plate")]
    pub dilution_plate: Plates,
    #[serde(default = "default_agar_plate")]
    pub agar_plate: Plates,
    #[serde(default = "default_media_rack")]
    pub media_rack: MediaRack,
    #[serde(default = "default_plating_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_plating_large_tips")]
    pub large_tips: TipRacks,
}

impl Default for PlatingStage {
    fn default() -> Self {
        default_plating_stage()
    }
}
