//! Profile fragments every liquid-handler bench declares, whatever the robot.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::resources::PlateCapacity;

fn default_tip_capacity() -> PlateCapacity {
    PlateCapacity::new(96).expect("96 is an addressable plate geometry")
}

/// One or more identical tip racks used by the canonical pipetting interpreter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TipRacks {
    pub labware: String,
    pub slots: Vec<String>,
    #[serde(default = "default_tip_capacity")]
    pub capacity: PlateCapacity,
}

impl TipRacks {
    pub fn total_capacity(&self) -> usize {
        self.slots.len() * self.capacity.get()
    }
}
