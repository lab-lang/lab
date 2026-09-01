use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ProcedureLocalId, TemperatureRange, Volume};

/// The semantic role of one logical vessel before any deck or well allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum VesselRole {
    /// A liquid value arriving through the enclosing Procedure task's zero-based input list.
    ProcedureInput {
        input: u32,
    },
    MaterialSource {
        material: ProcedureLocalId,
    },
    Product {
        output: ProcedureLocalId,
    },
    /// A physical vessel arriving through a task input and leaving as a new material state.
    InputOutput {
        input: u32,
        output: ProcedureLocalId,
    },
    /// A material substrate, such as selective agar, that becomes the task's product.
    MaterialProduct {
        material: ProcedureLocalId,
        output: ProcedureLocalId,
    },
    Intermediate,
}

/// A logical vessel with zero-based addressable positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vessel {
    pub id: ProcedureLocalId,
    pub role: VesselRole,
    pub positions: u32,
    /// Exact liquid volume initially present in every position when it is known to the Method.
    /// Material sources may omit this value so the adapter can calculate a sufficient source load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_volume_each: Option<Volume>,
    /// Largest liquid volume one position of this vessel can hold.
    ///
    /// Stated when the Method knows the vessel it requires. A dispense that would exceed it is a
    /// spill, which is cheaper to catch here than on the deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_capacity_each: Option<Volume>,
    /// Volume in one position the program must not draw below.
    ///
    /// This is the Method's own floor, such as leaving residual above a pellet, not the labware's
    /// unaspirable residual. An adapter knows the tube it will use and enforces its own dead volume
    /// on top of this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_volume_each: Option<Volume>,
    /// Temperature this vessel's contents must be held at while the program runs.
    ///
    /// This is per vessel rather than per program because one program routinely stages materials
    /// with different requirements: chemically competent cells must stay near 0 C while the
    /// recovery medium they are later given must not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<TemperatureRange>,
}

/// One logical position in a Procedure vessel.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub vessel: ProcedureLocalId,
    pub position: u32,
}

#[cfg(test)]
mod tests {
    use super::Vessel;

    #[test]
    fn a_canonical_vessel_rejects_keys_it_does_not_know() {
        let stale_vessel = r#"{
            "id": "cells",
            "role": {"kind": "intermediate"},
            "positions": 1,
            "initial_volume": {"value": {"type": "real", "value": "50"}, "unit": "http://qudt.org/vocab/unit/MicroL"}
        }"#;
        let error = serde_json::from_str::<Vessel>(stale_vessel).unwrap_err();
        assert!(
            error.to_string().contains("initial_volume"),
            "the unknown key is named: {error}"
        );
    }
}
