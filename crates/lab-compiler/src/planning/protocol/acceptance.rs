use serde::{Deserialize, Serialize};

/// Evidence-backed condition that a protocol plan must satisfy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceCriterion {
    ExactSequence,
    MinimumConcentration { nanograms_per_microliter: u32 },
    MinimumVolume { microliters: u32 },
}
