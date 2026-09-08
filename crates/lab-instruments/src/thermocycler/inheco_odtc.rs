//! The Inheco ODTC adapter for Lab's thermocycler capability.

use crate::{
    ProfileProgress, RunHandle, SensorReading, ThermalLimits, ThermalProfile, ThermalReadings,
    ThermalRun, ThermalRunError, Thermocycler,
};
use inheco_sila::{
    ActualTemperatures, MethodRun, MethodSettings, Odtc, OdtcError, OdtcOptions, ThermalProgram,
};
use thiserror::Error;

/// The ODTC's envelope in Lab's vocabulary, for validating profiles
/// before a device is even connected.
pub fn odtc_thermal_limits() -> ThermalLimits {
    ThermalLimits {
        block_min_celsius: inheco_sila::BLOCK_MIN_CELSIUS,
        block_max_celsius: inheco_sila::BLOCK_MAX_CELSIUS,
        lid_min_celsius: inheco_sila::LID_MIN_CELSIUS,
        lid_max_celsius: inheco_sila::LID_MAX_CELSIUS,
        ramp_max_c_per_s: inheco_sila::MAX_SLOPE_C_PER_S,
        per_step_lid: true,
    }
}

#[derive(Debug, Error)]
pub enum OdtcStationError {
    #[error(transparent)]
    Device(#[from] OdtcError),
    #[error(transparent)]
    InvalidRun(#[from] ThermalRunError),
    #[error("run handle {handle} names no run this adapter started")]
    UnknownRun { handle: u64 },
    #[error(
        "this ODTC session was configured for {configured_fill_volume_ul} µL with post-heating {configured_post_heating}, but the requested run needs {requested_fill_volume_ul} µL with post-heating {requested_post_heating}"
    )]
    RunSettingsMismatch {
        configured_fill_volume_ul: f64,
        configured_post_heating: bool,
        requested_fill_volume_ul: f64,
        requested_post_heating: bool,
    },
}

/// An ODTC session speaking Lab's [`Thermocycler`] capability.
pub struct OdtcStation {
    device: Odtc,
    /// Settings frozen when the vendor session connected. `inheco-sila`
    /// applies these when it renders every method uploaded by this session.
    method_settings: MethodSettings,
    /// The run in flight, keyed both ways: Lab's handle and the vendor's.
    active: Option<(RunHandle, MethodRun)>,
}

impl OdtcStation {
    fn new(device: Odtc, method_settings: MethodSettings) -> OdtcStation {
        OdtcStation {
            device,
            method_settings,
            active: None,
        }
    }

    /// Connects a fresh vendor session configured for one complete reviewed run.
    ///
    /// ODTC method settings are session-scoped in `inheco-sila`. Callers therefore
    /// open one station per run instead of reusing a session whose fill-volume
    /// control class may have been selected for a different load.
    pub fn connect_for_run(
        device: std::net::SocketAddr,
        run: &ThermalRun,
    ) -> Result<OdtcStation, OdtcStationError> {
        run.validate(&odtc_thermal_limits())?;
        let method_settings = method_settings_from(run);
        let transport = inheco_sila::HttpSoapTransport::connect(device)
            .map_err(|error| OdtcStationError::Device(OdtcError::Transport(error)))?;
        let options = OdtcOptions {
            method_settings: method_settings.clone(),
            ..OdtcOptions::default()
        };
        let session = Odtc::connect(std::sync::Arc::new(transport), options)?;
        Ok(OdtcStation::new(session, method_settings))
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

/// Maps every run-level value understood by the ODTC MethodSet into the
/// connection-scoped vendor settings used to render the uploaded method.
fn method_settings_from(run: &ThermalRun) -> MethodSettings {
    MethodSettings {
        fill_volume_ul: run.fill_volume_ul,
        // A requested final hold is installed explicitly after the finite
        // method. Keep the last plateau controlled until that command takes
        // over; otherwise let the finite method end without implicit heating.
        post_heating: run.final_hold_celsius.is_some(),
        ..MethodSettings::default()
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
            .map(|stage| inheco_sila::ProgramStage {
                steps: stage
                    .steps
                    .iter()
                    .map(|step| inheco_sila::ProgramStep {
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

    fn start_run(&mut self, run: &ThermalRun) -> Result<RunHandle, OdtcStationError> {
        run.validate(&self.limits())?;
        let requested = method_settings_from(run);
        if requested != self.method_settings {
            return Err(OdtcStationError::RunSettingsMismatch {
                configured_fill_volume_ul: self.method_settings.fill_volume_ul,
                configured_post_heating: self.method_settings.post_heating,
                requested_fill_volume_ul: requested.fill_volume_ul,
                requested_post_heating: requested.post_heating,
            });
        }
        let method_run = self.device.start_method(&program_from(&run.profile))?;
        let handle = RunHandle::new(u64::from(method_run.request_id()));
        self.active = Some((handle, method_run));
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
    use crate::{ThermalStage, ThermalStep};

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
    fn a_run_configures_the_vendor_session_from_reviewed_physical_values() {
        let run = ThermalRun {
            profile: ThermalProfile {
                stages: vec![ThermalStage {
                    steps: vec![ThermalStep {
                        celsius: 37.0,
                        hold_seconds: 90.0,
                        ramp_c_per_s: None,
                        lid_celsius: None,
                    }],
                    repeats: 1,
                }],
            },
            sample_count: 8,
            fill_volume_ul: 82.5,
            final_hold_celsius: Some(4.0),
        };

        let settings = method_settings_from(&run);
        assert_eq!(settings.fill_volume_ul, 82.5);
        assert!(
            settings.post_heating,
            "the last plateau remains controlled until the explicit final hold takes over"
        );
        assert_eq!(
            (settings.start_block_celsius, settings.start_lid_celsius),
            (25.0, 105.0),
            "unstated start conditions retain the vendor defaults"
        );

        let settings_without_hold = method_settings_from(&ThermalRun {
            final_hold_celsius: None,
            ..run
        });
        assert!(!settings_without_hold.post_heating);
    }

    #[test]
    fn the_mount_sensor_becomes_the_block_reading() {
        let readings = readings_from(ActualTemperatures {
            mount_celsius: 37.0,
            lid_celsius: Some(105.0),
            sensors: vec![inheco_sila::SensorValue {
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
