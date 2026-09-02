use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::operation::PipettingStep;
use super::vessel::Vessel;
use crate::procedure::ProcedureLocalId;

/// One material made available to a canonical pipetting program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialInput {
    pub id: ProcedureLocalId,
}

/// One material state produced by a canonical pipetting program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialOutput {
    pub id: ProcedureLocalId,
}

/// Cross-cutting conditions that every realization of the program must preserve.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipettingConstraints {}

/// Version 1 of Lab's canonical, device-neutral pipetting contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipettingProgramV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<MaterialInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<MaterialOutput>,
    pub vessels: Vec<Vessel>,
    pub steps: Vec<PipettingStep>,
    #[serde(default)]
    pub constraints: PipettingConstraints,
}

impl PipettingProgramV1 {
    pub fn new(
        materials: Vec<MaterialInput>,
        outputs: Vec<MaterialOutput>,
        vessels: Vec<Vessel>,
        steps: Vec<PipettingStep>,
        constraints: PipettingConstraints,
    ) -> Self {
        Self {
            materials,
            outputs,
            vessels,
            steps,
            constraints,
        }
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::{MaterialInput, MaterialOutput, PipettingConstraints, PipettingProgramV1};
    use crate::procedure::pipetting::{
        FluidPathPolicy, Location, MixTechnique, PipettingStep, TransferTechnique, Vessel,
        VesselRole,
    };
    use crate::procedure::{ProcedureLocalId, Temperature, TemperatureRange, Volume};

    pub(in crate::procedure::pipetting) fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    pub(in crate::procedure::pipetting) fn location(vessel: &str, position: u32) -> Location {
        Location {
            vessel: id(vessel),
            position,
        }
    }

    pub(in crate::procedure::pipetting) fn example() -> PipettingProgramV1 {
        PipettingProgramV1::new(
            vec![MaterialInput { id: id("water") }],
            vec![MaterialOutput { id: id("reaction") }],
            vec![
                Vessel {
                    id: id("water-source"),
                    role: VesselRole::MaterialSource {
                        material: id("water"),
                    },
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: Some(TemperatureRange::exact(
                        Temperature::parse_degrees_celsius("4").unwrap(),
                    )),
                },
                Vessel {
                    id: id("reactions"),
                    role: VesselRole::Product {
                        output: id("reaction"),
                    },
                    positions: 2,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
            ],
            vec![
                PipettingStep::Distribute {
                    id: id("add-water"),
                    source: location("water-source", 0),
                    destinations: vec![location("reactions", 0), location("reactions", 1)],
                    volume_each: Volume::parse_microlitres("0.5").unwrap(),
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                    fluid_path_group: None,
                    technique: TransferTechnique::default(),
                },
                PipettingStep::Transfer {
                    id: id("add-buffer"),
                    source: location("water-source", 0),
                    destination: location("reactions", 0),
                    volume: Volume::parse_microlitres("2").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: TransferTechnique::default(),
                },
                PipettingStep::Mix {
                    id: id("mix-reactions"),
                    targets: vec![location("reactions", 0), location("reactions", 1)],
                    cycles: 3,
                    volume: Volume::parse_microlitres("0.5").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: MixTechnique::default(),
                },
            ],
            PipettingConstraints::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::PipettingConstraints;

    #[test]
    fn pipetting_constraints_reject_keys_the_contract_does_not_know() {
        // A writer still emitting a field this contract has moved or removed must be told, not
        // quietly trimmed. `source_temperature` used to live on the program's constraints.
        let stale_constraints = r#"{"source_temperature": {"minimum": 4, "maximum": 4}}"#;
        assert!(serde_json::from_str::<PipettingConstraints>(stale_constraints).is_err());
    }
}
