use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Duration, ProcedureLocalId, Temperature, TemperatureRampRate, Volume};

/// The material state carried through one canonical thermal program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalLoad {
    /// Zero-based input of the enclosing Procedure task that is loaded into the instrument.
    pub input: u32,
    /// Exact enclosing Procedure outputs established after the program completes.
    ///
    /// Multiple typed outputs may describe the same processed physical load. For example, heat
    /// shock establishes both a named strain product and the transformed culture that proceeds to
    /// recovery.
    pub outputs: Vec<ProcedureLocalId>,
    /// Number of independently addressable samples run under the same profile.
    pub sample_count: u32,
    /// Fill volume of each sample.
    pub volume_each: Volume,
}

/// One repeated group of ordered thermal plateaus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalStage {
    pub id: ProcedureLocalId,
    /// Total executions of this group; one means execute it once.
    pub repeats: u32,
    pub steps: Vec<ThermalStep>,
}

/// One plateau: reach a block temperature at an optional controlled rate, then hold it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalStep {
    pub id: ProcedureLocalId,
    pub temperature: Temperature,
    pub hold: Duration,
    /// An explicit target ramp rate. `None` permits the implementation's default rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_rate: Option<TemperatureRampRate>,
}

/// Version 1 of Lab's canonical, device-neutral thermal-program contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalProgramV1 {
    pub load: ThermalLoad,
    /// One lid setpoint applied across the program. `None` means no heated-lid requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lid_temperature: Option<Temperature>,
    pub stages: Vec<ThermalStage>,
    /// An indefinite block hold after all finite stages complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_hold: Option<Temperature>,
}

#[cfg(test)]
pub(super) fn test_program() -> ThermalProgramV1 {
    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn temperature(value: &str) -> Temperature {
        Temperature::parse_degrees_celsius(value).unwrap()
    }

    fn duration(value: &str) -> Duration {
        Duration::parse_seconds(value).unwrap()
    }

    ThermalProgramV1 {
        load: ThermalLoad {
            input: 0,
            outputs: vec![id("product")],
            sample_count: 8,
            volume_each: Volume::parse_microlitres("20").unwrap(),
        },
        lid_temperature: Some(temperature("105")),
        stages: vec![ThermalStage {
            id: id("cycle"),
            repeats: 30,
            steps: vec![
                ThermalStep {
                    id: id("denature"),
                    temperature: temperature("95"),
                    hold: duration("15"),
                    ramp_rate: Some(
                        TemperatureRampRate::parse_degrees_celsius_per_second("2.5").unwrap(),
                    ),
                },
                ThermalStep {
                    id: id("anneal"),
                    temperature: temperature("60"),
                    hold: duration("30"),
                    ramp_rate: None,
                },
            ],
        }],
        final_hold: Some(temperature("4")),
    }
}
