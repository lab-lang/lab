//! Deserializable shape of an OT-2 target profile: instruments, deck
//! modules, and the labware each build stage claims.

use serde::{Deserialize, Serialize};

use super::defaults::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default = "default_api_level")]
    pub api_level: String,
}

impl Default for TargetMetadata {
    fn default() -> Self {
        Self {
            name: default_bench_name(),
            backend: default_backend(),
            api_level: default_api_level(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pipette {
    pub model: String,
    pub mount: String,
}

/// Hardware present for every stage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemperatureModule {
    pub model: String,
    pub slot: String,
    /// Rack of chilled source tubes carried on the module.
    pub labware: String,
    pub capacity: usize,
}

/// The thermocycler occupies fixed slots, so it declares no slot of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thermocycler {
    pub model: String,
    pub labware: String,
    pub capacity: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stages {
    #[serde(default = "default_assembly_stage")]
    pub assembly: AssemblyStage,
    #[serde(default = "default_transformation_stage")]
    pub transformation: TransformationStage,
    #[serde(default = "default_plating_stage")]
    pub plating: PlatingStage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// One or more identical plates. Allocation fills each in turn, so adding a
/// slot raises a build's capacity without changing any program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plates {
    pub labware: String,
    pub slots: Vec<String>,
    #[serde(default = "default_plate_capacity")]
    pub capacity: usize,
}

impl Plates {
    pub fn total_capacity(&self) -> usize {
        self.slots.len() * self.capacity
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TipRacks {
    pub labware: String,
    pub slots: Vec<String>,
    #[serde(default = "default_plate_capacity")]
    pub capacity: usize,
}

impl TipRacks {
    pub fn total_capacity(&self) -> usize {
        self.slots.len() * self.capacity
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRack {
    pub labware: String,
    pub slot: String,
    pub medium_well: String,
}
