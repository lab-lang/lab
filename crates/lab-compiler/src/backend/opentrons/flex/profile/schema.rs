//! Deserializable shape of a Flex target profile: instruments, deck modules,
//! the trash bin, and the labware each build stage claims.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use crate::backend::profile::{MediaRack, Plates, TipRacks};

use crate::backend::opentrons::flex::profile::defaults::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetMetadata {
    /// The bench this profile describes, named by whoever loaded it: a profile
    /// is selected as `targets/<name>.toml`, so the file does not repeat its
    /// own name and cannot disagree with it. Emitted plans carry the name so
    /// an operator can see which bench a protocol was compiled for.
    #[serde(skip_deserializing, default = "default_bench_name")]
    pub name: String,
    #[serde(default = "default_backend")]
    pub backend: String,
}

impl Default for TargetMetadata {
    fn default() -> Self {
        Self {
            name: default_bench_name(),
            backend: default_backend(),
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

/// Hardware present for every stage: the two installed modules and the
/// movable trash bin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlexDeck {
    #[serde(default = "default_temperature_module")]
    pub temperature_module: TemperatureModule,
    #[serde(default = "default_thermocycler")]
    pub thermocycler: Thermocycler,
    #[serde(default = "default_trash")]
    pub trash: Trash,
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
    /// Rack of chilled source tubes carried on the module.
    pub labware: String,
    pub capacity: usize,
}

/// The thermocycler installs across slots A1 and B1, so it declares no slot
/// of its own and nothing else may claim those.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Thermocycler {
    pub model: String,
    pub labware: String,
    pub capacity: usize,
}

/// The movable trash bin, named by its addressable area. The bin occupies its
/// deck slot, so no stage may place labware there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Trash {
    pub area: String,
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
