//! The Inheco ODTC as a workcell thermocycler station.

use lab_inheco_odtc::{ActualTemperatures, MethodRun, Odtc, OdtcError, ThermalProgram};
use lab_instruments::{
    ProfileProgress, RunHandle, SensorReading, ThermalLimits, ThermalProfile, ThermalReadings,
    Thermocycler,
};
use thiserror::Error;

/// The ODTC's envelope in Lab's vocabulary, for validating profiles
/// before a device is even connected.
pub fn odtc_thermal_limits() -> ThermalLimits {
    ThermalLimits {
        block_min_celsius: lab_inheco_odtc::BLOCK_MIN_CELSIUS,
        block_max_celsius: lab_inheco_odtc::BLOCK_MAX_CELSIUS,
        lid_min_celsius: lab_inheco_odtc::LID_MIN_CELSIUS,
        lid_max_celsius: lab_inheco_odtc::LID_MAX_CELSIUS,
        ramp_max_c_per_s: lab_inheco_odtc::MAX_SLOPE_C_PER_S,
        per_step_lid: true,
    }
}

#[derive(Debug, Error)]
pub enum OdtcStationError {
    #[error(transparent)]
    Device(#[from] OdtcError),
    #[error("run handle {handle} names no run this station started")]
    UnknownRun { handle: u64 },
}

/// An ODTC session speaking Lab's [`Thermocycler`] capability.
pub struct OdtcStation {
    device: Odtc,
    /// The run in flight, keyed both ways: Lab's handle and the vendor's.
    active: Option<(RunHandle, MethodRun)>,
}

impl OdtcStation {
    /// Wraps a connected vendor session.
    pub fn new(device: Odtc) -> OdtcStation {
        OdtcStation {
            device,
            active: None,
        }
    }

    /// Connects to the device at an address and wraps the session.
    pub fn connect(device: std::net::SocketAddr) -> Result<OdtcStation, OdtcStationError> {
        let transport = lab_inheco_odtc::HttpSoapTransport::connect(device)
            .map_err(|error| OdtcStationError::Device(OdtcError::Transport(error)))?;
        let session = Odtc::connect(
            std::sync::Arc::new(transport),
            lab_inheco_odtc::OdtcOptions::default(),
        )?;
        Ok(OdtcStation::new(session))
    }

    /// The warnings the device raised since the last call, oldest first.
    pub fn take_warnings(&mut self) -> Vec<String> {
        self.device.take_warnings()
    }

    /// The wrapped vendor session, for vendor-specific work the
    /// capability trait does not model.
    pub fn device(&mut self) -> &mut Odtc {
        &mut self.device
    }
}

/// Lab's neutral profile in the vendor's vocabulary. The shapes agree
/// field for field; the translation exists so neither side depends on
/// the other's types.
fn program_from(profile: &ThermalProfile) -> ThermalProgram {
    ThermalProgram {
        stages: profile
            .stages
            .iter()
            .map(|stage| lab_inheco_odtc::ProgramStage {
                steps: stage
                    .steps
                    .iter()
                    .map(|step| lab_inheco_odtc::ProgramStep {
                        plateau_celsius: step.celsius,
                        hold_seconds: step.hold_seconds,
                        slope_c_per_s: step.ramp_c_per_s,
                        lid_celsius: step.lid_celsius,
                    })
                    .collect(),
                repeats: stage.repeats,
            })
            .collect(),
    }
}

/// The vendor's temperature report in Lab's vocabulary: the ODTC's
/// mount sensor is the block.
fn readings_from(temperatures: ActualTemperatures) -> ThermalReadings {
    ThermalReadings {
        block_celsius: temperatures.mount_celsius,
        lid_celsius: temperatures.lid_celsius,
        sensors: temperatures
            .sensors
            .into_iter()
            .map(|sensor| SensorReading {
                name: sensor.name,
                celsius: sensor.celsius,
            })
            .collect(),
    }
}

impl Thermocycler for OdtcStation {
    type Error = OdtcStationError;

    fn limits(&self) -> ThermalLimits {
        odtc_thermal_limits()
    }

    fn open_lid(&mut self) -> Result<(), OdtcStationError> {
        Ok(self.device.open_door()?)
    }

    fn close_lid(&mut self) -> Result<(), OdtcStationError> {
        Ok(self.device.close_door()?)
    }

    fn hold_block(
        &mut self,
        celsius: f64,
        lid_celsius: Option<f64>,
    ) -> Result<(), OdtcStationError> {
        Ok(self.device.hold(celsius, lid_celsius)?)
    }

    fn run_profile(&mut self, profile: &ThermalProfile) -> Result<RunHandle, OdtcStationError> {
        let run = self.device.start_method(&program_from(profile))?;
        let handle = RunHandle::new(u64::from(run.request_id()));
        self.active = Some((handle, run));
        Ok(handle)
    }

    fn await_completion(&mut self, handle: RunHandle) -> Result<(), OdtcStationError> {
        let Some((active_handle, run)) = self.active else {
            return Err(OdtcStationError::UnknownRun {
                handle: handle.id(),
            });
        };
        if active_handle != handle {
            return Err(OdtcStationError::UnknownRun {
                handle: handle.id(),
            });
        }
        let outcome = self.device.await_method(run);
        self.active = None;
        Ok(outcome?)
    }

    fn read_temperatures(&mut self) -> Result<ThermalReadings, OdtcStationError> {
        Ok(readings_from(self.device.read_temperatures()?))
    }

    fn stop(&mut self) -> Result<(), OdtcStationError> {
        Ok(self.device.stop()?)
    }

    /// The ODTC exposes no run progress; `None` is the honest answer.
    fn progress(&mut self) -> Option<ProfileProgress> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_instruments::{ThermalStage, ThermalStep};

    #[test]
    fn a_profile_translates_field_for_field_into_the_vendor_program() {
        let profile = ThermalProfile {
            stages: vec![ThermalStage {
                steps: vec![
                    ThermalStep {
                        celsius: 37.0,
                        hold_seconds: 90.0,
                        ramp_c_per_s: Some(2.5),
                        lid_celsius: Some(108.0),
                    },
                    ThermalStep {
                        celsius: 16.0,
                        hold_seconds: 180.0,
                        ramp_c_per_s: None,
                        lid_celsius: None,
                    },
                ],
                repeats: 30,
            }],
        };
        let program = program_from(&profile);
        assert_eq!(program.stages.len(), 1);
        assert_eq!(program.stages[0].repeats, 30);
        let first = &program.stages[0].steps[0];
        assert_eq!(
            (first.plateau_celsius, first.hold_seconds),
            (37.0, 90.0),
            "plateau and hold carry over"
        );
        assert_eq!(
            first.slope_c_per_s,
            Some(2.5),
            "a stated ramp becomes a slope"
        );
        assert_eq!(first.lid_celsius, Some(108.0));
        let second = &program.stages[0].steps[1];
        assert_eq!(
            second.slope_c_per_s, None,
            "an unset ramp stays the device default"
        );
    }

    #[test]
    fn the_mount_sensor_becomes_the_block_reading() {
        let readings = readings_from(ActualTemperatures {
            mount_celsius: 37.0,
            lid_celsius: Some(105.0),
            sensors: vec![lab_inheco_odtc::SensorValue {
                name: "Heatsink".to_string(),
                celsius: 29.0,
            }],
        });
        assert_eq!(readings.block_celsius, 37.0, "Mount is the block");
        assert_eq!(readings.lid_celsius, Some(105.0));
        assert_eq!(readings.sensors.len(), 1);
        assert_eq!(readings.sensors[0].name, "Heatsink");
    }

    #[test]
    fn the_stated_limits_match_the_vendor_envelope() {
        let limits = odtc_thermal_limits();
        assert_eq!(
            (limits.block_min_celsius, limits.block_max_celsius),
            (4.0, 99.0)
        );
        assert_eq!(
            (limits.lid_min_celsius, limits.lid_max_celsius),
            (30.0, 115.0)
        );
        assert_eq!(limits.ramp_max_c_per_s, 4.4);
        assert!(limits.per_step_lid, "the ODTC varies the lid per step");
    }
}
