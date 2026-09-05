//! Deserializable Flex configuration for the physical resources consumed by canonical programs.

use schemars::JsonSchema;

use crate::backend::resources::PlateCapacity;
use serde::{Deserialize, Serialize};

pub use crate::backend::profile::TipRacks;

use crate::backend::opentrons::flex::profile::defaults::*;

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
pub struct FlexResources {
    #[serde(default = "default_sources")]
    pub sources: TemperatureModule,
    #[serde(default = "default_work")]
    pub work: Thermocycler,
    #[serde(default = "default_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_large_tips")]
    pub large_tips: TipRacks,
    #[serde(default = "default_trash")]
    pub trash: Trash,
}

impl Default for FlexResources {
    fn default() -> Self {
        Self {
            sources: default_sources(),
            work: default_work(),
            small_tips: default_small_tips(),
            large_tips: default_large_tips(),
            trash: default_trash(),
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

impl Default for Trash {
    fn default() -> Self {
        default_trash()
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
}

/// The thermocycler-backed work area. It occupies slots A1 and B1 and declares no slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Thermocycler {
    pub model: String,
    pub labware: String,
    pub capacity: PlateCapacity,
}

/// The movable trash bin, named by its addressable area. The bin occupies its
/// deck slot, so no stage may place labware there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Trash {
    pub area: String,
}

/// Calibrated Flex realization policy for canonical liquid-access techniques.
///
/// These are measured properties of one bench, not scientific quantities. The canonical program
/// states that an aspiration must follow the liquid surface; this states the geometry that turns
/// a planned volume into a millimetre offset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlexTechniqueCalibration {
    /// Volume the operator loads into the tracked medium source before the run.
    #[serde(default = "default_flex_tracked_source_volume_ul")]
    pub tracked_source_volume_ul: u32,
    /// Usable liquid depth of the tracked source at its declared load volume.
    #[serde(default = "default_flex_tracked_usable_depth_mm")]
    pub tracked_usable_depth_mm: f64,
    /// How far below the computed meniscus the tip is placed.
    #[serde(default = "default_flex_tracked_meniscus_offset_mm")]
    pub tracked_meniscus_offset_mm: f64,
    /// Floor for the computed offset, so the tip never reaches the vessel bottom.
    #[serde(default = "default_flex_tracked_minimum_height_mm")]
    pub tracked_minimum_height_mm: f64,
}

impl Default for FlexTechniqueCalibration {
    fn default() -> Self {
        Self {
            tracked_source_volume_ul: default_flex_tracked_source_volume_ul(),
            tracked_usable_depth_mm: default_flex_tracked_usable_depth_mm(),
            tracked_meniscus_offset_mm: default_flex_tracked_meniscus_offset_mm(),
            tracked_minimum_height_mm: default_flex_tracked_minimum_height_mm(),
        }
    }
}

impl FlexTechniqueCalibration {
    /// Aspiration offset above the vessel bottom once `withdrawn_ul` has been removed.
    pub fn tracked_offset_mm(&self, withdrawn_ul: f64) -> f64 {
        let loaded = f64::from(self.tracked_source_volume_ul);
        let remaining = (loaded - withdrawn_ul).max(0.0);
        let meniscus = (remaining / loaded) * self.tracked_usable_depth_mm;
        (meniscus - self.tracked_meniscus_offset_mm).max(self.tracked_minimum_height_mm)
    }

    pub(super) fn validate(&self) -> Result<(), &'static str> {
        if self.tracked_source_volume_ul == 0 {
            return Err("tracked_source_volume_ul must be greater than zero");
        }
        for value in [
            self.tracked_usable_depth_mm,
            self.tracked_meniscus_offset_mm,
            self.tracked_minimum_height_mm,
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err("tracked technique offsets must be finite and greater than zero");
            }
        }
        if self.tracked_minimum_height_mm > self.tracked_usable_depth_mm {
            return Err("tracked_minimum_height_mm cannot exceed tracked_usable_depth_mm");
        }
        Ok(())
    }
}
