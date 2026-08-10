//! Thermal profiles, the thermocycler interface, and its stations.

mod inheco_odtc;

pub use inheco_odtc::{OdtcStation, OdtcStationError, odtc_thermal_limits};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A complete thermal program: ordered stages, each a cycled group of steps.
///
/// The profile is device-neutral data. Whether a step's optional ramp rate
/// or per-step lid temperature is honored depends on the instrument; a
/// device that cannot honor a stated value rejects the profile rather than
/// silently approximating it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermalProfile {
    pub stages: Vec<ThermalStage>,
}

/// A group of steps executed in order and repeated as a block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermalStage {
    pub steps: Vec<ThermalStep>,
    /// Total executions of the block; 1 means run once.
    pub repeats: u32,
}

/// One plateau: ramp to a temperature, hold it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermalStep {
    pub celsius: f64,
    pub hold_seconds: f64,
    /// Ramp rate toward this plateau; `None` means the device maximum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_c_per_s: Option<f64>,
    /// Lid temperature during this step; `None` means the device default.
    /// Only devices with per-step lid control honor a value here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lid_celsius: Option<f64>,
}

/// The envelope a concrete thermocycler accepts, used to validate profiles
/// before anything is uploaded or run.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermalLimits {
    pub block_min_celsius: f64,
    pub block_max_celsius: f64,
    pub lid_min_celsius: f64,
    pub lid_max_celsius: f64,
    pub ramp_max_c_per_s: f64,
    /// Whether per-step lid temperatures are honored at all.
    pub per_step_lid: bool,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ThermalProfileError {
    #[error("a thermal profile must contain at least one stage with at least one step")]
    Empty,
    #[error("stage {stage} repeats {repeats} times; a stage runs at least once")]
    ZeroRepeats { stage: usize, repeats: u32 },
    #[error(
        "step {step} of stage {stage} targets {celsius} °C, outside the block range {min}–{max} °C"
    )]
    BlockTemperatureOutOfRange {
        stage: usize,
        step: usize,
        celsius: f64,
        min: f64,
        max: f64,
    },
    #[error(
        "step {step} of stage {stage} sets the lid to {celsius} °C, outside the lid range {min}–{max} °C"
    )]
    LidTemperatureOutOfRange {
        stage: usize,
        step: usize,
        celsius: f64,
        min: f64,
        max: f64,
    },
    #[error(
        "step {step} of stage {stage} sets a per-step lid temperature, which this device cannot vary by step"
    )]
    PerStepLidUnsupported { stage: usize, step: usize },
    #[error(
        "step {step} of stage {stage} asks for {ramp} °C/s, above the device maximum {max} °C/s"
    )]
    RampOutOfRange {
        stage: usize,
        step: usize,
        ramp: f64,
        max: f64,
    },
    #[error("step {step} of stage {stage} holds for {seconds} s; a hold cannot be negative")]
    NegativeHold {
        stage: usize,
        step: usize,
        seconds: f64,
    },
    #[error("step {step} of stage {stage} asks for a ramp of {ramp} °C/s; a ramp must be positive")]
    NonPositiveRamp {
        stage: usize,
        step: usize,
        ramp: f64,
    },
}

impl ThermalProfile {
    /// Validates the profile against a device's envelope. A valid profile
    /// is one the device will accept verbatim; anything else is an error
    /// here rather than a surprise at the bench.
    pub fn validate(&self, limits: &ThermalLimits) -> Result<(), ThermalProfileError> {
        if self.stages.is_empty() || self.stages.iter().any(|stage| stage.steps.is_empty()) {
            return Err(ThermalProfileError::Empty);
        }
        for (stage_index, stage) in self.stages.iter().enumerate() {
            if stage.repeats == 0 {
                return Err(ThermalProfileError::ZeroRepeats {
                    stage: stage_index,
                    repeats: stage.repeats,
                });
            }
            for (step_index, step) in stage.steps.iter().enumerate() {
                if step.celsius < limits.block_min_celsius
                    || step.celsius > limits.block_max_celsius
                {
                    return Err(ThermalProfileError::BlockTemperatureOutOfRange {
                        stage: stage_index,
                        step: step_index,
                        celsius: step.celsius,
                        min: limits.block_min_celsius,
                        max: limits.block_max_celsius,
                    });
                }
                if step.hold_seconds < 0.0 {
                    return Err(ThermalProfileError::NegativeHold {
                        stage: stage_index,
                        step: step_index,
                        seconds: step.hold_seconds,
                    });
                }
                if let Some(ramp) = step.ramp_c_per_s {
                    if ramp <= 0.0 {
                        return Err(ThermalProfileError::NonPositiveRamp {
                            stage: stage_index,
                            step: step_index,
                            ramp,
                        });
                    }
                    if ramp > limits.ramp_max_c_per_s {
                        return Err(ThermalProfileError::RampOutOfRange {
                            stage: stage_index,
                            step: step_index,
                            ramp,
                            max: limits.ramp_max_c_per_s,
                        });
                    }
                }
                if let Some(lid) = step.lid_celsius {
                    if !limits.per_step_lid {
                        return Err(ThermalProfileError::PerStepLidUnsupported {
                            stage: stage_index,
                            step: step_index,
                        });
                    }
                    if lid < limits.lid_min_celsius || lid > limits.lid_max_celsius {
                        return Err(ThermalProfileError::LidTemperatureOutOfRange {
                            stage: stage_index,
                            step: step_index,
                            celsius: lid,
                            min: limits.lid_min_celsius,
                            max: limits.lid_max_celsius,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Total plateau count across all repeats, for progress denominators
    /// and duration estimates.
    pub fn total_steps(&self) -> usize {
        self.stages
            .iter()
            .map(|stage| stage.steps.len() * stage.repeats as usize)
            .sum()
    }
}

/// A running profile, resolved by [`Thermocycler::await_completion`]. The
/// handle is opaque; drivers mint them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunHandle(u64);

impl RunHandle {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn id(&self) -> u64 {
        self.0
    }
}

/// Live temperatures. Every device reports the block; everything else is
/// present when the hardware has the sensor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermalReadings {
    pub block_celsius: f64,
    pub lid_celsius: Option<f64>,
    /// Additional named sensors, verbatim from the device.
    #[serde(default)]
    pub sensors: Vec<SensorReading>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SensorReading {
    pub name: String,
    pub celsius: f64,
}

/// Where a running profile stands, on devices that can say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileProgress {
    pub completed_steps: usize,
    pub total_steps: usize,
}

/// A standalone thermocycler.
///
/// Awaiting completion is the one portable synchronization primitive: a
/// compiled plan may depend on it and on nothing else. [`Self::progress`]
/// is telemetry for operators, never control flow — expressive devices
/// with opaque runs (the ODTC) return `None` for the whole run.
pub trait Thermocycler {
    type Error: std::error::Error + Send + Sync + 'static;

    /// The envelope this device accepts; profiles validate against it
    /// before anything runs.
    fn limits(&self) -> ThermalLimits;

    fn open_lid(&mut self) -> Result<(), Self::Error>;
    fn close_lid(&mut self) -> Result<(), Self::Error>;

    /// Holds the block (and lid, on devices that couple them) at a constant
    /// temperature. On some devices this is slow: the ODTC equilibrates
    /// block and lid together over several minutes.
    fn hold_block(&mut self, celsius: f64, lid_celsius: Option<f64>) -> Result<(), Self::Error>;

    /// Starts a validated profile and returns without waiting.
    fn run_profile(&mut self, profile: &ThermalProfile) -> Result<RunHandle, Self::Error>;

    /// Blocks until the referenced run finishes, however long that takes.
    fn await_completion(&mut self, handle: RunHandle) -> Result<(), Self::Error>;

    fn read_temperatures(&mut self) -> Result<ThermalReadings, Self::Error>;

    /// Stops whatever is running and leaves the device idle.
    fn stop(&mut self) -> Result<(), Self::Error>;

    /// Where the current run stands, on devices that expose it. `None`
    /// means the device cannot say — not that nothing is running.
    fn progress(&mut self) -> Option<ProfileProgress> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn odtc_like_limits() -> ThermalLimits {
        ThermalLimits {
            block_min_celsius: 4.0,
            block_max_celsius: 99.0,
            lid_min_celsius: 30.0,
            lid_max_celsius: 115.0,
            ramp_max_c_per_s: 4.4,
            per_step_lid: true,
        }
    }

    fn golden_gate_profile() -> ThermalProfile {
        ThermalProfile {
            stages: vec![
                ThermalStage {
                    steps: vec![
                        ThermalStep {
                            celsius: 37.0,
                            hold_seconds: 90.0,
                            ramp_c_per_s: None,
                            lid_celsius: None,
                        },
                        ThermalStep {
                            celsius: 16.0,
                            hold_seconds: 180.0,
                            ramp_c_per_s: None,
                            lid_celsius: None,
                        },
                    ],
                    repeats: 30,
                },
                ThermalStage {
                    steps: vec![ThermalStep {
                        celsius: 60.0,
                        hold_seconds: 300.0,
                        ramp_c_per_s: None,
                        lid_celsius: None,
                    }],
                    repeats: 1,
                },
            ],
        }
    }

    #[test]
    fn a_golden_gate_profile_validates_and_counts_its_plateaus() {
        let profile = golden_gate_profile();
        profile
            .validate(&odtc_like_limits())
            .expect("a routine assembly profile is inside every device envelope");
        assert_eq!(
            profile.total_steps(),
            61,
            "30 cycles of two steps plus the final ligation"
        );
    }

    #[test]
    fn a_block_temperature_outside_the_envelope_is_rejected_with_the_range() {
        let mut profile = golden_gate_profile();
        profile.stages[0].steps[0].celsius = 105.0;
        let error = profile
            .validate(&odtc_like_limits())
            .expect_err("105 °C is above the 99 °C block ceiling");
        assert_eq!(
            error,
            ThermalProfileError::BlockTemperatureOutOfRange {
                stage: 0,
                step: 0,
                celsius: 105.0,
                min: 4.0,
                max: 99.0,
            }
        );
    }

    #[test]
    fn a_per_step_lid_on_a_device_without_one_is_rejected() {
        let mut limits = odtc_like_limits();
        limits.per_step_lid = false;
        let mut profile = golden_gate_profile();
        profile.stages[0].steps[0].lid_celsius = Some(105.0);
        let error = profile
            .validate(&limits)
            .expect_err("the device cannot vary the lid by step");
        assert_eq!(
            error,
            ThermalProfileError::PerStepLidUnsupported { stage: 0, step: 0 }
        );
    }

    #[test]
    fn ramps_must_be_positive_and_inside_the_device_maximum() {
        let mut profile = golden_gate_profile();
        profile.stages[0].steps[0].ramp_c_per_s = Some(9.0);
        assert!(matches!(
            profile.validate(&odtc_like_limits()),
            Err(ThermalProfileError::RampOutOfRange { ramp, max, .. }) if ramp == 9.0 && max == 4.4
        ));
        profile.stages[0].steps[0].ramp_c_per_s = Some(0.0);
        assert!(matches!(
            profile.validate(&odtc_like_limits()),
            Err(ThermalProfileError::NonPositiveRamp { .. })
        ));
    }

    #[test]
    fn an_empty_profile_or_stage_is_rejected() {
        let empty = ThermalProfile { stages: vec![] };
        assert_eq!(
            empty.validate(&odtc_like_limits()),
            Err(ThermalProfileError::Empty)
        );
        let hollow = ThermalProfile {
            stages: vec![ThermalStage {
                steps: vec![],
                repeats: 1,
            }],
        };
        assert_eq!(
            hollow.validate(&odtc_like_limits()),
            Err(ThermalProfileError::Empty)
        );
    }

    #[test]
    fn a_profile_round_trips_through_json_without_optional_noise() {
        let profile = golden_gate_profile();
        let text = serde_json::to_string(&profile).expect("profiles serialize");
        assert!(
            !text.contains("ramp_c_per_s"),
            "unset ramps stay out of the document: {text}"
        );
        let back: ThermalProfile = serde_json::from_str(&text).expect("profiles parse");
        assert_eq!(back, profile);
    }
}
