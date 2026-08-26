//! Profile fragments every liquid-handler bench declares, whatever the robot.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_plate_capacity() -> usize {
    96
}

/// One or more identical plates. Allocation fills each in turn, so adding a
/// slot raises a build's capacity without changing any program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaRack {
    pub labware: String,
    pub slot: String,
    pub medium_well: String,
}
