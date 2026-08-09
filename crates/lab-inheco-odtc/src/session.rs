//! The session layer: an [`Odtc`] handle owning a transport, the connect
//! handshake, the run choreography, and completion resolved by callback
//! with a polling fallback.
//!
//! One command is in flight at a time — the device serializes commands
//! anyway — and request ids increment and wrap within 31 bits.
//! Completion of an asynchronous command is resolved by the matching
//! `ResponseEvent` when the callback channel works, and by polling
//! `GetStatus` at the configured interval for a settled state (idle or
//! standby) when it does not, so a firewall never wedges a run.
//!
//! Dropping the session does **not** stop a running method: a thermal
//! program outlives a process crash by design, and the runner decides
//! when to stop it.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::methodset::{self, MethodSetError, MethodSettings, ThermalProgram};
use crate::soap::{
    Command, DataEvent, DeviceState, IncomingEvent, RETURN_CODE_ACCEPTED,
    RETURN_CODE_ASYNC_SUCCESS, RETURN_CODE_ASYNC_WARNING, RETURN_CODE_SUCCESS, ResponseEvent,
    SoapError, SyncResponse,
};
use crate::transport::{SoapTransport, TransportError};

/// The highest request id; the next one wraps to 1.
const MAX_REQUEST_ID: u32 = 0x7FFF_FFFF;

/// The lid temperature a hold uses when the caller states none.
const DEFAULT_HOLD_LID_CELSIUS: f64 = 105.0;

/// The error raised by session operations.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum OdtcError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Protocol(#[from] SoapError),
    #[error(transparent)]
    MethodSet(#[from] MethodSetError),
    #[error("the device rejected {command} with return code {code}: {message}")]
    Rejected {
        command: &'static str,
        code: i32,
        message: String,
    },
    #[error("{command} failed on the device with return code {code}: {message}")]
    Failed {
        command: String,
        code: i32,
        message: String,
    },
    #[error("timed out after {seconds} s waiting for {command} to complete")]
    CompletionTimeout { command: String, seconds: u64 },
    #[error("timed out after {seconds} s waiting for the device to reach idle or standby")]
    IdleTimeout { seconds: u64 },
    #[error("the device is '{state}'; {problem}")]
    State { state: String, problem: String },
    #[error("method run {request_id} names no run this session started")]
    UnknownRun { request_id: u32 },
    #[error(
        "{command} completed without a ResponseEvent, so its data never arrived; check that the device can reach the callback listener at {uri}"
    )]
    MissingResponseData { command: String, uri: String },
    #[error("the temperature report names no Mount sensor; the mount reading is mandatory")]
    MissingMountSensor,
}

/// Options for a session.
#[derive(Clone, Debug, PartialEq)]
pub struct OdtcOptions {
    /// The `deviceId` registered by `Reset`.
    pub device_id: String,
    /// Whether the device runs in its firmware simulation mode.
    pub simulation: bool,
    /// The completion budget for short asynchronous commands: reset,
    /// initialization, door moves, uploads, temperature reads.
    pub command_timeout: Duration,
    /// The completion budget for `ExecuteMethod` and holds; profiles run
    /// for hours.
    pub method_timeout: Duration,
    /// How often the polling fallback asks `GetStatus` while waiting.
    pub poll_interval: Duration,
    /// Method-level settings profiles do not carry.
    pub method_settings: MethodSettings,
}

impl Default for OdtcOptions {
    fn default() -> OdtcOptions {
        OdtcOptions {
            device_id: "lab".to_string(),
            simulation: false,
            command_timeout: Duration::from_secs(300),
            method_timeout: Duration::from_secs(24 * 60 * 60),
            poll_interval: Duration::from_secs(1),
            method_settings: MethodSettings::default(),
        }
    }
}

/// What `GetDeviceIdentification` reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceIdentification {
    pub device_name: Option<String>,
    pub serial_number: Option<String>,
    pub firmware_version: Option<String>,
}

/// A method run this session started, resolved by [`Odtc::await_method`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodRun {
    request_id: u32,
}

impl MethodRun {
    /// The request id `ExecuteMethod` ran under.
    pub fn request_id(&self) -> u32 {
        self.request_id
    }
}

/// One named sensor's reading, verbatim from the device.
#[derive(Clone, Debug, PartialEq)]
pub struct SensorValue {
    pub name: String,
    pub celsius: f64,
}

/// What `ReadActualTemperature` reported: the mount (block) sensor, the
/// lid when present, and every other named sensor as the device sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct ActualTemperatures {
    pub mount_celsius: f64,
    pub lid_celsius: Option<f64>,
    pub sensors: Vec<SensorValue>,
}

#[derive(Clone, Debug)]
struct ActiveRun {
    run: MethodRun,
    method_name: String,
}

/// A connected ODTC.
pub struct Odtc {
    transport: Arc<dyn SoapTransport>,
    options: OdtcOptions,
    next_request_id: u32,
    run_counter: u32,
    active_run: Option<ActiveRun>,
    latest_data: Option<DataEvent>,
    warnings: Vec<String>,
}

impl Odtc {
    /// Connects: registers the transport's event-receiver URI with
    /// `Reset`, runs `Initialize`, and polls `GetStatus` until the
    /// device reports idle.
    pub fn connect(
        transport: Arc<dyn SoapTransport>,
        options: OdtcOptions,
    ) -> Result<Odtc, OdtcError> {
        let mut session = Odtc {
            transport,
            options,
            next_request_id: 1,
            run_counter: 0,
            active_run: None,
            latest_data: None,
            warnings: Vec::new(),
        };
        let reset = Command::Reset {
            device_id: session.options.device_id.clone(),
            event_receiver_uri: session.transport.event_receiver_uri(),
            simulation_mode: session.options.simulation,
        };
        session.execute(&reset)?;
        session.execute(&Command::Initialize)?;
        session.wait_for_settled(session.options.command_timeout)?;
        Ok(session)
    }

    /// The method name of the run in flight, when one is.
    pub fn active_method_name(&self) -> Option<&str> {
        self.active_run.as_ref().map(|run| run.method_name.as_str())
    }

    /// Warnings the device attached to otherwise-successful completions
    /// (return code 12), drained on read.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    /// The newest telemetry event the device POSTed during a wait, when
    /// any arrived. Temperature series carry centi-degrees.
    pub fn latest_data_event(&self) -> Option<&DataEvent> {
        self.latest_data.as_ref()
    }

    /// Asks the device for its current state.
    pub fn status(&mut self) -> Result<DeviceState, OdtcError> {
        let (_, response) = self.transact(&Command::GetStatus)?;
        let state = response.state().ok_or_else(|| SoapError::MissingElement {
            element: "state".to_string(),
        })?;
        Ok(DeviceState::from_wire(state))
    }

    /// Asks the device who it is.
    pub fn identification(&mut self) -> Result<DeviceIdentification, OdtcError> {
        let (_, response) = self.transact(&Command::GetDeviceIdentification)?;
        Ok(DeviceIdentification {
            device_name: response.field("DeviceName").map(str::to_string),
            serial_number: response.field("SerialNumber").map(str::to_string),
            firmware_version: response.field("FirmwareVersion").map(str::to_string),
        })
    }

    fn allocate_request_id(&mut self) -> u32 {
        let id = self.next_request_id;
        self.next_request_id = advance_request_id(id);
        id
    }

    /// Sends one command and returns its synchronous response, verifying
    /// the return code admits it: 1 (already complete) or 2 (accepted
    /// for asynchronous completion).
    fn transact(&mut self, command: &Command) -> Result<(u32, SyncResponse), OdtcError> {
        let request_id = self.allocate_request_id();
        let body = self
            .transport
            .send(&command.soap_action(), &command.envelope(request_id))?;
        let response = SyncResponse::parse(&body)?;
        if response.return_code != RETURN_CODE_SUCCESS
            && response.return_code != RETURN_CODE_ACCEPTED
        {
            return Err(OdtcError::Rejected {
                command: command.name(),
                code: response.return_code,
                message: response.message,
            });
        }
        Ok((request_id, response))
    }

    /// Runs a command to completion under the short-command budget:
    /// immediately when the device answers return code 1, otherwise by
    /// waiting for the matching `ResponseEvent` with the polling
    /// fallback. Returns the event when one arrived; `None` means the
    /// polling fallback observed completion instead.
    fn execute(&mut self, command: &Command) -> Result<Option<ResponseEvent>, OdtcError> {
        let (request_id, response) = self.transact(command)?;
        if response.return_code == RETURN_CODE_SUCCESS {
            return Ok(None);
        }
        self.wait_for_response(request_id, command.name(), self.options.command_timeout)
    }

    /// Blocks until the command behind `request_id` completes: the
    /// matching `ResponseEvent` resolves it directly; between events,
    /// `GetStatus` is polled and a settled state (idle or standby)
    /// resolves it as the fallback. A transition into an error state,
    /// seen either way, fails the wait naming the state.
    fn wait_for_response(
        &mut self,
        request_id: u32,
        command: &str,
        timeout: Duration,
    ) -> Result<Option<ResponseEvent>, OdtcError> {
        let deadline = Instant::now() + timeout;
        let mut next_poll = Instant::now() + self.options.poll_interval;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(OdtcError::CompletionTimeout {
                    command: command.to_string(),
                    seconds: timeout.as_secs(),
                });
            }
            let wait = next_poll.min(deadline).saturating_duration_since(now);
            match self.transport.receive_event(wait)? {
                Some(IncomingEvent::Response(event)) if event.request_id == request_id => {
                    return self.resolve_response(event, command).map(Some);
                }
                // A stale command's completion; nothing waits on it.
                Some(IncomingEvent::Response(_)) => {}
                Some(IncomingEvent::Status(status)) => {
                    if let Some(state) = status.device_state()
                        && state.is_error()
                    {
                        return Err(OdtcError::State {
                            state: state.as_wire().to_string(),
                            problem: format!("{command} cannot complete from an error state"),
                        });
                    }
                }
                Some(IncomingEvent::Data(data)) => {
                    self.latest_data = Some(data);
                }
                None => {}
            }
            if Instant::now() >= next_poll {
                next_poll = Instant::now() + self.options.poll_interval;
                let state = self.status()?;
                if state.is_error() {
                    return Err(OdtcError::State {
                        state: state.as_wire().to_string(),
                        problem: format!("{command} cannot complete from an error state"),
                    });
                }
                if state.is_settled() {
                    return Ok(None);
                }
            }
        }
    }

    fn resolve_response(
        &mut self,
        event: ResponseEvent,
        command: &str,
    ) -> Result<ResponseEvent, OdtcError> {
        match event.return_code {
            RETURN_CODE_ASYNC_SUCCESS => Ok(event),
            RETURN_CODE_ASYNC_WARNING => {
                self.warnings.push(format!("{command}: {}", event.message));
                Ok(event)
            }
            code => Err(OdtcError::Failed {
                command: command.to_string(),
                code,
                message: event.message,
            }),
        }
    }

    /// Polls `GetStatus` until the device settles at idle or standby,
    /// draining events between polls so telemetry is kept and stale
    /// completions do not pile up.
    fn wait_for_settled(&mut self, timeout: Duration) -> Result<(), OdtcError> {
        let deadline = Instant::now() + timeout;
        loop {
            let state = self.status()?;
            if state.is_settled() {
                return Ok(());
            }
            if state.is_error() {
                return Err(OdtcError::State {
                    state: state.as_wire().to_string(),
                    problem: "resolve the device error before continuing".to_string(),
                });
            }
            if Instant::now() >= deadline {
                return Err(OdtcError::IdleTimeout {
                    seconds: timeout.as_secs(),
                });
            }
            if let Some(IncomingEvent::Data(data)) =
                self.transport.receive_event(self.options.poll_interval)?
            {
                self.latest_data = Some(data);
            }
        }
    }

    /// Stops whatever runs and settles the device. `StopMethod` is sent
    /// unconditionally; an idle device may reject it, and the settle
    /// poll afterward is the real arbiter, so only transport failures
    /// propagate from the stop itself.
    fn stop_and_settle(&mut self) -> Result<(), OdtcError> {
        let request_id = self.allocate_request_id();
        let command = Command::StopMethod;
        self.transport
            .send(&command.soap_action(), &command.envelope(request_id))?;
        self.active_run = None;
        self.wait_for_settled(self.options.command_timeout)
    }

    fn fresh_method_name(&mut self, kind: &str) -> String {
        self.run_counter += 1;
        format!("lab_{kind}_{:03}", self.run_counter)
    }

    fn upload_method_set(&mut self, method_set_xml: &str) -> Result<(), OdtcError> {
        self.execute(&Command::SetParameters {
            params_xml: methodset::parameter_set(method_set_xml),
        })?;
        Ok(())
    }

    fn refuse_door_while_running(&mut self) -> Result<(), OdtcError> {
        let state = self.status()?;
        if matches!(state, DeviceState::Busy | DeviceState::Paused) {
            return Err(OdtcError::State {
                state: state.as_wire().to_string(),
                problem: "a method is running — stop it before moving the door".to_string(),
            });
        }
        Ok(())
    }
}

impl Odtc {
    /// Opens the motorized door. Refused while a method runs.
    pub fn open_door(&mut self) -> Result<(), OdtcError> {
        self.refuse_door_while_running()?;
        self.execute(&Command::OpenDoor)?;
        Ok(())
    }

    /// Closes the motorized door. Refused while a method runs.
    pub fn close_door(&mut self) -> Result<(), OdtcError> {
        self.refuse_door_while_running()?;
        self.execute(&Command::CloseDoor)?;
        Ok(())
    }

    /// Uploads and runs a `PreMethod`: the device equilibrates block and
    /// lid together — several minutes — then holds until a method runs
    /// or [`Odtc::stop`] intervenes. The call blocks until the hold is
    /// established.
    pub fn hold(&mut self, celsius: f64, lid_celsius: Option<f64>) -> Result<(), OdtcError> {
        let lid = lid_celsius.unwrap_or(DEFAULT_HOLD_LID_CELSIUS);
        let method_name = self.fresh_method_name("hold");
        let method_xml = methodset::render_pre_method(
            &method_name,
            "lab",
            &utc_timestamp(),
            celsius,
            lid,
            true,
        )?;
        self.stop_and_settle()?;
        self.upload_method_set(&method_xml)?;
        let (request_id, response) = self.transact(&Command::ExecuteMethod { method_name })?;
        if response.return_code == RETURN_CODE_ACCEPTED {
            self.wait_for_response(request_id, "ExecuteMethod", self.options.method_timeout)?;
        }
        Ok(())
    }

    /// Validates and renders the program, stops whatever runs, settles
    /// the device, uploads the method set under a fresh name, and starts
    /// it. Returns without waiting; completion belongs to
    /// [`Odtc::await_method`].
    pub fn start_method(&mut self, program: &ThermalProgram) -> Result<MethodRun, OdtcError> {
        let method_name = self.fresh_method_name("profile");
        let settings = self.options.method_settings.clone();
        let method_xml =
            methodset::render_method(&method_name, "lab", &utc_timestamp(), program, &settings)?;
        self.stop_and_settle()?;
        self.upload_method_set(&method_xml)?;
        let (request_id, _) = self.transact(&Command::ExecuteMethod {
            method_name: method_name.clone(),
        })?;
        let run = MethodRun { request_id };
        self.active_run = Some(ActiveRun { run, method_name });
        Ok(run)
    }

    /// Blocks until the referenced run finishes, however long that takes.
    pub fn await_method(&mut self, run: MethodRun) -> Result<(), OdtcError> {
        let Some(active) = self.active_run.clone() else {
            return Err(OdtcError::UnknownRun {
                request_id: run.request_id,
            });
        };
        if active.run != run {
            return Err(OdtcError::UnknownRun {
                request_id: run.request_id,
            });
        }
        let outcome = self.wait_for_response(
            active.run.request_id,
            "ExecuteMethod",
            self.options.method_timeout,
        );
        self.active_run = None;
        outcome.map(|_| ())
    }

    /// Reads every temperature sensor the device reports.
    pub fn read_temperatures(&mut self) -> Result<ActualTemperatures, OdtcError> {
        let event = self
            .execute(&Command::ReadActualTemperature)?
            .ok_or_else(|| OdtcError::MissingResponseData {
                command: "ReadActualTemperature".to_string(),
                uri: self.transport.event_receiver_uri(),
            })?;
        let data = event
            .response_data
            .ok_or_else(|| SoapError::MissingElement {
                element: "responseData".to_string(),
            })?;
        let mut mount = None;
        let mut lid = None;
        let mut sensors = Vec::new();
        for (name, celsius) in crate::soap::parse_temperature_data(&data)? {
            match name.as_str() {
                "Mount" => mount = Some(celsius),
                "Lid" => lid = Some(celsius),
                _ => sensors.push(SensorValue { name, celsius }),
            }
        }
        Ok(ActualTemperatures {
            mount_celsius: mount.ok_or(OdtcError::MissingMountSensor)?,
            lid_celsius: lid,
            sensors,
        })
    }

    /// Stops whatever runs and leaves the device idle.
    pub fn stop(&mut self) -> Result<(), OdtcError> {
        self.stop_and_settle()
    }
}

/// The next request id after `id`: ids increment and wrap within the
/// positive 31-bit range the protocol admits.
fn advance_request_id(id: u32) -> u32 {
    if id >= MAX_REQUEST_ID { 1 } else { id + 1 }
}

/// The current UTC instant as strict ISO-8601 with an explicit `+00:00`
/// offset — the form the MethodSet dialect requires on method
/// timestamps.
fn utc_timestamp() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    format_utc_timestamp(since_epoch.as_secs(), since_epoch.subsec_millis())
}

fn format_utc_timestamp(seconds_since_epoch: u64, millis: u32) -> String {
    let days = seconds_since_epoch / 86_400;
    let seconds_of_day = seconds_since_epoch % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}+00:00",
        hours = seconds_of_day / 3600,
        minutes = seconds_of_day % 3600 / 60,
        seconds = seconds_of_day % 60,
    )
}

/// Days since 1970-01-01 to a civil (year, month, day), via the standard
/// era decomposition over the 400-year Gregorian cycle.
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soap::request_id_of;
    use crate::transport::mock::{MockReply, MockSoapTransport, sync_response};
    use std::sync::Mutex;

    fn fast_options() -> OdtcOptions {
        OdtcOptions {
            command_timeout: Duration::from_millis(500),
            method_timeout: Duration::from_millis(500),
            poll_interval: Duration::from_millis(5),
            ..OdtcOptions::default()
        }
    }

    fn accepted_then_success(command: &str, envelope: &str) -> MockReply {
        let request_id = request_id_of(envelope).expect("every command carries a request id");
        MockReply {
            response: sync_response(command, RETURN_CODE_ACCEPTED, "Accepted", &[]),
            events: vec![IncomingEvent::Response(ResponseEvent {
                request_id,
                return_code: RETURN_CODE_ASYNC_SUCCESS,
                message: "Success".to_string(),
                response_data: None,
            })],
        }
    }

    fn status_reply(state: &Arc<Mutex<&'static str>>) -> MockReply {
        MockReply::sync(sync_response(
            "GetStatus",
            RETURN_CODE_SUCCESS,
            "Success",
            &[(
                "state",
                *state.lock().expect("the state lock is never poisoned"),
            )],
        ))
    }

    /// A responder for the common choreography: asynchronous commands
    /// are accepted and complete immediately by event; `GetStatus`
    /// reports the shared state variable.
    fn scripted_device(state: Arc<Mutex<&'static str>>) -> impl Fn(&str, &str) -> MockReply {
        move |command, envelope| match command {
            "GetStatus" => status_reply(&state),
            "GetDeviceIdentification" => MockReply::sync(sync_response(
                command,
                RETURN_CODE_SUCCESS,
                "Success",
                &[("DeviceName", "ODTC"), ("SerialNumber", "42")],
            )),
            _ => accepted_then_success(command, envelope),
        }
    }

    /// A responder for the firewalled-callback scenarios: asynchronous
    /// commands are accepted but no completion event ever arrives, so
    /// every wait resolves by injected events or by polling alone.
    fn silent_device(state: Arc<Mutex<&'static str>>) -> impl Fn(&str, &str) -> MockReply {
        move |command, _| match command {
            "GetStatus" => status_reply(&state),
            _ => MockReply::sync(sync_response(
                command,
                RETURN_CODE_ACCEPTED,
                "Accepted",
                &[],
            )),
        }
    }

    fn connected_session() -> (Arc<MockSoapTransport>, Arc<Mutex<&'static str>>, Odtc) {
        let transport = Arc::new(MockSoapTransport::new());
        let state = Arc::new(Mutex::new("idle"));
        transport.set_responder(scripted_device(Arc::clone(&state)));
        let session = Odtc::connect(
            Arc::clone(&transport) as Arc<dyn SoapTransport>,
            fast_options(),
        )
        .expect("the handshake succeeds against the scripted device");
        (transport, state, session)
    }

    fn two_step_profile() -> ThermalProgram {
        ThermalProgram {
            stages: vec![crate::methodset::ProgramStage {
                steps: vec![
                    crate::methodset::ProgramStep {
                        plateau_celsius: 37.0,
                        hold_seconds: 90.0,
                        slope_c_per_s: None,
                        lid_celsius: None,
                    },
                    crate::methodset::ProgramStep {
                        plateau_celsius: 16.0,
                        hold_seconds: 180.0,
                        slope_c_per_s: None,
                        lid_celsius: None,
                    },
                ],
                repeats: 30,
            }],
        }
    }

    #[test]
    fn connect_resets_registers_the_listener_initializes_and_polls_to_idle() {
        let (transport, _state, _session) = connected_session();
        assert_eq!(
            transport.sent_names(),
            vec!["Reset", "Initialize", "GetStatus"],
            "the handshake is reset, initialize, then the settle poll"
        );
        let reset = &transport.sent()[0];
        assert!(
            reset.envelope.contains("<deviceId>lab</deviceId>"),
            "Reset registers the configured device id: {}",
            reset.envelope
        );
        assert!(
            reset
                .envelope
                .contains("<eventReceiverURI>http://192.0.2.1:49152/</eventReceiverURI>"),
            "Reset registers the transport's event-receiver URI: {}",
            reset.envelope
        );
        assert!(
            reset
                .envelope
                .contains("<simulationMode>false</simulationMode>"),
            "Reset states the simulation mode explicitly: {}",
            reset.envelope
        );
    }

    #[test]
    fn run_profile_stops_settles_uploads_then_executes() {
        let (transport, _state, mut session) = connected_session();
        session
            .start_method(&two_step_profile())
            .expect("the scripted device accepts the run");
        let names = transport.sent_names();
        assert_eq!(
            &names[3..],
            ["StopMethod", "GetStatus", "SetParameters", "ExecuteMethod"],
            "the run choreography is stop, settle, upload, execute"
        );
        let upload = &transport.sent()[5];
        assert!(
            upload.envelope.contains("MethodsXML"),
            "the upload carries the MethodsXML parameter: {}",
            upload.envelope
        );
        assert!(
            upload.envelope.contains("lab_profile_001"),
            "the freshly generated method name reaches the device: {}",
            upload.envelope
        );
        let execute = &transport.sent()[6];
        assert!(
            execute
                .envelope
                .contains("<methodName>lab_profile_001</methodName>"),
            "the execute names the just-uploaded method: {}",
            execute.envelope
        );
    }

    #[test]
    fn a_run_completes_when_its_response_event_arrives() {
        let (transport, state, mut session) = connected_session();
        transport.set_responder(silent_device(Arc::clone(&state)));
        let handle = session
            .start_method(&two_step_profile())
            .expect("the scripted device accepts the run");
        // The device is busy for the whole wait, so the polling fallback
        // cannot resolve it; only the injected completion event can.
        *state.lock().expect("the state lock is never poisoned") = "busy";
        let request_id = transport
            .sent()
            .last()
            .expect("ExecuteMethod was sent")
            .request_id;
        transport.inject_event(IncomingEvent::Response(ResponseEvent {
            request_id: request_id.expect("ExecuteMethod carries a request id"),
            return_code: RETURN_CODE_ASYNC_SUCCESS,
            message: "Success".to_string(),
            response_data: None,
        }));
        session
            .await_method(handle)
            .expect("the injected completion event resolves the wait");
    }

    #[test]
    fn a_run_completes_by_polling_when_no_event_arrives() {
        let (transport, state, mut session) = connected_session();
        transport.set_responder(silent_device(Arc::clone(&state)));
        let handle = session
            .start_method(&two_step_profile())
            .expect("the scripted device accepts the run");
        let sends_before = transport.sent_names().len();
        session
            .await_method(handle)
            .expect("the idle status poll resolves the wait without any event");
        let status_polls = transport.sent_names()[sends_before..]
            .iter()
            .filter(|name| name.as_str() == "GetStatus")
            .count();
        assert!(
            status_polls >= 1,
            "the fallback polled GetStatus at least once"
        );
    }

    #[test]
    fn an_async_failure_surfaces_the_device_message() {
        let (transport, state, mut session) = connected_session();
        transport.set_responder(silent_device(Arc::clone(&state)));
        let handle = session
            .start_method(&two_step_profile())
            .expect("the scripted device accepts the run");
        let request_id = transport
            .sent()
            .last()
            .expect("ExecuteMethod was sent")
            .request_id
            .expect("ExecuteMethod carries a request id");
        transport.inject_event(IncomingEvent::Response(ResponseEvent {
            request_id,
            return_code: 9,
            message: "method aborted: lid overtemperature".to_string(),
            response_data: None,
        }));
        let error = session
            .await_method(handle)
            .expect_err("a failing completion event fails the wait");
        assert_eq!(
            error,
            OdtcError::Failed {
                command: "ExecuteMethod".to_string(),
                code: 9,
                message: "method aborted: lid overtemperature".to_string(),
            }
        );
    }

    #[test]
    fn a_success_with_warning_completes_and_records_the_warning() {
        let (transport, state, mut session) = connected_session();
        transport.set_responder(silent_device(Arc::clone(&state)));
        let handle = session
            .start_method(&two_step_profile())
            .expect("the scripted device accepts the run");
        let request_id = transport
            .sent()
            .last()
            .expect("ExecuteMethod was sent")
            .request_id
            .expect("ExecuteMethod carries a request id");
        transport.inject_event(IncomingEvent::Response(ResponseEvent {
            request_id,
            return_code: RETURN_CODE_ASYNC_WARNING,
            message: "lid heater aged".to_string(),
            response_data: None,
        }));
        session
            .await_method(handle)
            .expect("return code 12 is success with a warning, not a failure");
        assert_eq!(
            session.take_warnings(),
            vec!["ExecuteMethod: lid heater aged".to_string()],
            "the warning is kept for the operator"
        );
        assert!(session.take_warnings().is_empty(), "warnings drain on read");
    }

    #[test]
    fn the_door_stays_shut_while_a_method_runs() {
        let (transport, state, mut session) = connected_session();
        *state.lock().expect("the state lock is never poisoned") = "busy";
        let error = session
            .open_door()
            .expect_err("a busy device refuses door motion");
        assert_eq!(
            error,
            OdtcError::State {
                state: "busy".to_string(),
                problem: "a method is running — stop it before moving the door".to_string(),
            }
        );
        assert_eq!(
            error.to_string(),
            "the device is 'busy'; a method is running — stop it before moving the door"
        );
        assert!(
            !transport.sent_names().contains(&"OpenDoor".to_string()),
            "no door command reaches a busy device"
        );
    }

    #[test]
    fn a_settled_device_opens_and_closes_its_door() {
        let (transport, _state, mut session) = connected_session();
        session.open_door().expect("an idle device opens its door");
        session
            .close_door()
            .expect("an idle device closes its door");
        let names = transport.sent_names();
        assert!(names.contains(&"OpenDoor".to_string()));
        assert!(names.contains(&"CloseDoor".to_string()));
    }

    #[test]
    fn temperatures_map_mount_and_lid_with_the_rest_as_sensors() {
        let (transport, _state, mut session) = connected_session();
        let sensors = "<Temperature><Mount>3700</Mount><Lid>10496</Lid>\
                       <Ambient>2210</Ambient><Heatsink>2900</Heatsink></Temperature>";
        let response_data = format!(
            "<ResponseData><ParameterSet><Parameter name=\"Temperature\"><String>{}</String>\
             </Parameter></ParameterSet></ResponseData>",
            quick_xml::escape::partial_escape(sensors)
        );
        transport.set_responder(move |command, envelope| match command {
            "ReadActualTemperature" => {
                let request_id = request_id_of(envelope).expect("the read carries a request id");
                MockReply {
                    response: sync_response(command, RETURN_CODE_ACCEPTED, "Accepted", &[]),
                    events: vec![IncomingEvent::Response(ResponseEvent {
                        request_id,
                        return_code: RETURN_CODE_ASYNC_SUCCESS,
                        message: "Success".to_string(),
                        response_data: Some(response_data.clone()),
                    })],
                }
            }
            _ => MockReply::sync(sync_response(command, RETURN_CODE_SUCCESS, "Success", &[])),
        });
        let readings = session
            .read_temperatures()
            .expect("the scripted report parses");
        assert_eq!(readings.mount_celsius, 37.0, "Mount is the block sensor");
        assert_eq!(readings.lid_celsius, Some(104.96), "Lid is the lid");
        assert_eq!(
            readings.sensors,
            vec![
                SensorValue {
                    name: "Ambient".to_string(),
                    celsius: 22.1
                },
                SensorValue {
                    name: "Heatsink".to_string(),
                    celsius: 29.0
                },
            ],
            "every other sensor is carried by name"
        );
    }

    #[test]
    fn a_synchronous_rejection_names_the_command_and_message() {
        let transport = Arc::new(MockSoapTransport::new());
        transport.set_responder(|command, _| {
            MockReply::sync(sync_response(command, 11, "device not ready", &[]))
        });
        let error = Odtc::connect(
            Arc::clone(&transport) as Arc<dyn SoapTransport>,
            fast_options(),
        )
        .err()
        .expect("a rejected Reset fails the handshake");
        assert_eq!(
            error,
            OdtcError::Rejected {
                command: "Reset",
                code: 11,
                message: "device not ready".to_string(),
            }
        );
    }

    #[test]
    fn an_unknown_handle_is_refused_by_name() {
        let (_transport, _state, mut session) = connected_session();
        let error = session
            .await_method(MethodRun { request_id: 999 })
            .expect_err("no run was started");
        assert_eq!(error, OdtcError::UnknownRun { request_id: 999 });
    }

    #[test]
    fn hold_block_uploads_a_pre_method_and_waits_for_equilibration() {
        let (transport, _state, mut session) = connected_session();
        session
            .hold(95.0, None)
            .expect("the scripted device equilibrates");
        let names = transport.sent_names();
        assert_eq!(
            &names[3..],
            ["StopMethod", "GetStatus", "SetParameters", "ExecuteMethod"],
            "a hold follows the same stop, settle, upload, execute choreography"
        );
        let upload = &transport.sent()[5];
        assert!(
            upload.envelope.contains("PreMethod"),
            "the hold uploads a PreMethod: {}",
            upload.envelope
        );
        assert!(
            upload.envelope.contains("TargetBlockTemperature&amp;gt;95"),
            "the doubly escaped hold targets the requested block temperature: {}",
            upload.envelope
        );
    }

    #[test]
    fn request_ids_wrap_within_31_bits() {
        assert_eq!(advance_request_id(1), 2);
        assert_eq!(advance_request_id(MAX_REQUEST_ID - 1), MAX_REQUEST_ID);
        assert_eq!(
            advance_request_id(MAX_REQUEST_ID),
            1,
            "the id after the 31-bit maximum wraps to 1, never 0 or negative"
        );
    }

    #[test]
    fn utc_timestamps_are_strict_iso_8601_with_an_offset() {
        assert_eq!(format_utc_timestamp(0, 0), "1970-01-01T00:00:00.000+00:00");
        assert_eq!(
            format_utc_timestamp(1_754_700_000, 503),
            "2025-08-09T00:40:00.503+00:00"
        );
        let now = utc_timestamp();
        methodset::render_pre_method("lab_hold_1", "lab", &now, 37.0, 105.0, true)
            .expect("a session-minted timestamp satisfies the MethodSet validation");
    }

    #[test]
    fn identification_reports_the_device_fields() {
        let (_transport, _state, mut session) = connected_session();
        let identification = session
            .identification()
            .expect("the scripted device answers");
        assert_eq!(identification.device_name.as_deref(), Some("ODTC"));
        assert_eq!(identification.serial_number.as_deref(), Some("42"));
        assert_eq!(identification.firmware_version, None);
    }
}
