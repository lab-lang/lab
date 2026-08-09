//! The session layer: a [`Star`] handle owning a transport, a background
//! reader thread correlating replies by command id, the per-module
//! concurrency locks the firmware requires, per-command read timeouts, and
//! the documented setup choreography.
//!
//! The public API is synchronous: every method blocks until the firmware
//! confirms the command (or its typed error decodes). Locking rules —
//! violations of which the firmware answers with trace 40:
//! - at most one in-flight command per module, with all pipetting channels
//!   `P1`–`PG` sharing one mutex;
//! - slave-module commands block master (`C0`) commands, and a master
//!   command is exclusive: it waits for the slaves to drain and runs alone;
//! - read-only `R*`/`Q*` queries are exempt and run fully parallel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::catalog::{TIP_5ML, TipType, fitting_depth};
use crate::commands::autoload::{AutoloadInitialize, AutoloadToSafeZ, MoveAutoloadToTrack};
use crate::commands::channel_direct::{RequestMinimumYSpacing, RequestStopDiskZ};
use crate::commands::core96::{
    Head96Initialize, Head96MoveToZSafety, Head96QueryInfo, Head96QueryType, Head96RequestZ,
    Head96Type,
};
use crate::commands::iswap::{
    IswapInitialize, IswapPark, MasterEepromOffset, ReadMasterEepromOffset,
};
use crate::commands::pipetting::{
    Aspirate, ChannelTarget, DEFAULT_TRAVERSE_HEIGHT, Dispense, InitializeChannels,
    RequestAllChannelY, RequestChannelTipZ, RequestLastLldHeights, RequestTipPresence, TipDiscard,
    TipDiscardMethod, TipDiscardReport, TipPickup,
};
use crate::commands::system::{
    DefineTipType, ExtendedConfiguration, MachineConfiguration, MoveAllChannelsToZSafety,
    PreInitialize, QueryInitializationStatus, RequestExtendedConfiguration, RequestFaultyParameter,
    RequestFirmwareVersion, RequestMachineConfiguration, RequestMaxXTravel, RequestWorkingEnvelope,
    XTravelRanges,
};
use crate::commands::{Command, is_query, read_timeout};
use crate::errors::{CommandError, FirmwareError};
use crate::framing::{CommandId, Module};
use crate::response::{RawResponse, ResponseParseError, split_error_section};
#[cfg(feature = "usb")]
use crate::transport::UsbTransport;
use crate::transport::{Transport, TransportError};
use crate::units::{Axis, Millimeters};

/// The minimum spacing between adjacent pipetting channels, in 0.1 mm. The
/// firmware enforces 9 mm; the session validates the same bound before a
/// command reaches the machine.
pub const MIN_CHANNEL_SPACING: u32 = 90;

/// The error raised by session operations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StarError {
    #[error(transparent)]
    Firmware(#[from] FirmwareError),
    #[error("{error}; the firmware names the faulty parameter as {parameter:?}")]
    FaultyParameter {
        error: FirmwareError,
        parameter: String,
    },
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Parse(#[from] ResponseParseError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("command {code} timed out after {seconds} s waiting for the firmware's reply")]
    Timeout { code: String, seconds: u64 },
    #[error(
        "channels {first} and {second} are {spacing} tenth-mm apart in Y; adjacent channels need at least 90 (9 mm)"
    )]
    ChannelSpacing {
        first: usize,
        second: usize,
        spacing: u32,
    },
    #[error("the session is closed; no further commands can be sent")]
    Closed,
}

/// The error raised when a frame cannot be accepted for raw replay.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RawCommandError {
    #[error("frame {frame:?} is shorter than the four-character module and command envelope")]
    TooShort { frame: String },
    #[error("frame {frame:?} is not plain ASCII; the firmware protocol admits no other bytes")]
    NotAscii { frame: String },
    #[error("frame {frame:?} addresses module {address:?}, which this crate does not know")]
    UnknownModule { frame: String, address: String },
    #[error("frame {frame:?} carries command code {code:?}; codes are two uppercase letters")]
    BadCode { frame: String, code: String },
    #[error(
        "frame {frame:?} already carries an id; raw frames must be id-less so the session can assign one"
    )]
    AlreadyHasId { frame: String },
}

/// One pre-authored id-less firmware frame, validated for replay through
/// [`Star::execute_raw`]. The envelope selects the module locks and the read
/// timeout; the session splices in the command id at send time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCommand {
    module: Module,
    code: String,
    frame: String,
}

impl RawCommand {
    /// Validates an id-less frame: a known module address, a two-uppercase-
    /// letter code, plain ASCII, and no id of its own.
    pub fn parse(frame: &str) -> Result<RawCommand, RawCommandError> {
        if !frame.is_ascii() {
            return Err(RawCommandError::NotAscii {
                frame: frame.to_string(),
            });
        }
        if frame.len() < 4 {
            return Err(RawCommandError::TooShort {
                frame: frame.to_string(),
            });
        }
        let address = &frame[..2];
        let Some(module) = Module::from_address(address) else {
            return Err(RawCommandError::UnknownModule {
                frame: frame.to_string(),
                address: address.to_string(),
            });
        };
        let code = &frame[2..4];
        if !code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(RawCommandError::BadCode {
                frame: frame.to_string(),
                code: code.to_string(),
            });
        }
        if frame[4..].starts_with("id") {
            return Err(RawCommandError::AlreadyHasId {
                frame: frame.to_string(),
            });
        }
        Ok(RawCommand {
            module,
            code: code.to_string(),
            frame: frame.to_string(),
        })
    }

    pub fn module(&self) -> Module {
        self.module
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    /// The id-less frame exactly as parsed.
    pub fn frame(&self) -> &str {
        &self.frame
    }

    /// The read timeout the session will wait under, from the per-code
    /// table.
    pub fn read_timeout(&self) -> Duration {
        read_timeout(&self.code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PendingKey {
    Id(u16),
    /// Module plus command code, for replies to id-less commands.
    Envelope(String),
}

struct Inner {
    transport: Arc<dyn Transport>,
    pending: Mutex<HashMap<PendingKey, mpsc::Sender<RawResponse>>>,
    next_id: Mutex<CommandId>,
    /// Slave commands hold this for read; master commands for write.
    gate: RwLock<()>,
    /// One mutex per module family; all pipetting channels share one entry.
    module_locks: Mutex<HashMap<&'static str, Arc<Mutex<()>>>>,
    shutdown: AtomicBool,
    dead: AtomicBool,
    read_timeout_override: Mutex<Option<Duration>>,
}

impl Inner {
    fn module_lock(&self, module: Module) -> Arc<Mutex<()>> {
        let key: &'static str = match module {
            Module::PipettingChannel(_) => "Px",
            Module::Head96 => "H0",
            Module::XDrives => "X0",
            Module::Iswap => "R0",
            Module::Autoload => "I0",
            Module::Master => "C0",
            _ => "other",
        };
        self.module_locks
            .lock()
            .expect("the module-lock table is never poisoned")
            .entry(key)
            .or_default()
            .clone()
    }

    fn fail_all_pending(&self) {
        // Dropping the senders disconnects every waiting receiver.
        self.pending
            .lock()
            .expect("the pending table is never poisoned")
            .clear();
    }
}

/// What the setup choreography discovered about the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineInfo {
    pub configuration: MachineConfiguration,
    pub extended: ExtendedConfiguration,
    pub travel: XTravelRanges,
    /// The six `UA` working-envelope values, 0.1 mm.
    pub working_envelope: Vec<i64>,
    /// The channel count, discovered as the length of the `RT` reply.
    pub channel_count: usize,
    /// Per-channel tip presence at connect time.
    pub tip_presence: Vec<bool>,
    /// Whether the firmware reported itself already initialized.
    pub was_initialized: bool,
    /// Per-channel `VY` minimum-spacing tables, Y-drive increments.
    pub minimum_y_spacings: Vec<Vec<i64>>,
    /// Channel 1's firmware version.
    pub channel_firmware: Option<String>,
    /// The iSWAP X offset (`kg`), 0.1 mm, when an iSWAP is installed.
    pub iswap_x_offset: Option<i64>,
    /// The 96-head X offset (`kf`), 0.1 mm, when a head is installed.
    pub head96_x_offset: Option<i64>,
    /// The 96 head's firmware version, when installed.
    pub head96_firmware: Option<String>,
    /// The 96 head's device information, when installed.
    pub head96_info: Option<String>,
    /// The 96 head's type, when installed.
    pub head96_type: Option<Head96Type>,
}

/// Options for the setup choreography, for the pieces that need deck
/// knowledge this crate does not carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InitializeOptions {
    /// Overrides the tip-waste X for channel initialization; the default is
    /// the machine's own `xw` from the extended configuration.
    pub tip_waste_x: Option<u32>,
    /// The track to park the autoload at (the machine's maximum track).
    /// Without it the autoload is raised to safe Z but not parked, because
    /// the track count depends on the deck.
    pub autoload_park_track: Option<u32>,
    /// The trash position for 96-head initialization. Without it an
    /// uninitialized head is left uninitialized, because the trash location
    /// depends on the deck.
    pub head96_trash: Option<Head96Initialize>,
}

/// One tip location on the deck, for the tip pickup and discard helpers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipSpot {
    /// 0-based channel index.
    pub channel: usize,
    pub x: Millimeters,
    pub y: Millimeters,
    /// The tip-spot Z: where the tip's mounting collar sits.
    pub z: Millimeters,
}

/// A connected STAR.
pub struct Star {
    inner: Arc<Inner>,
    reader: Option<JoinHandle<()>>,
    state: Mutex<SessionState>,
}

#[derive(Default)]
struct SessionState {
    machine: Option<MachineInfo>,
    /// Tip types already defined this session, keyed by value; the table is
    /// volatile in the firmware, so the cache resets with the session.
    tip_types: HashMap<(u32, u32, u32, bool, u32), u32>,
    next_tip_index: u32,
}

impl Star {
    /// Wraps a transport: drains stale responses, resets the id counter,
    /// and starts the reader thread.
    pub fn new(transport: Arc<dyn Transport>) -> Result<Star, StarError> {
        transport.drain()?;
        let inner = Arc::new(Inner {
            transport,
            pending: Mutex::new(HashMap::new()),
            next_id: Mutex::new(CommandId::FIRST),
            gate: RwLock::new(()),
            module_locks: Mutex::new(HashMap::new()),
            shutdown: AtomicBool::new(false),
            dead: AtomicBool::new(false),
            read_timeout_override: Mutex::new(None),
        });
        let reader_inner = Arc::clone(&inner);
        let reader = std::thread::Builder::new()
            .name("star-reader".to_string())
            .spawn(move || reader_loop(&reader_inner))
            .expect("spawning the reader thread cannot fail under normal conditions");
        Ok(Star {
            inner,
            reader: Some(reader),
            state: Mutex::new(SessionState::default()),
        })
    }

    /// Opens the first STAR on USB and wraps it in a session.
    #[cfg(feature = "usb")]
    pub fn open_usb() -> Result<Star, StarError> {
        let transport = UsbTransport::open()?;
        Star::new(Arc::new(transport))
    }

    /// Overrides every command's read timeout. Intended for tests over a
    /// mock transport; on hardware the per-command defaults are the
    /// documented safe values.
    pub fn set_read_timeout_override(&self, timeout: Option<Duration>) {
        *self
            .inner
            .read_timeout_override
            .lock()
            .expect("the override lock is never poisoned") = timeout;
    }

    /// What the setup choreography discovered, once [`Star::initialize`]
    /// has run.
    pub fn machine_info(&self) -> Option<MachineInfo> {
        self.state
            .lock()
            .expect("the session state is never poisoned")
            .machine
            .clone()
    }

    fn allocate_id(&self) -> CommandId {
        let mut next = self
            .inner
            .next_id
            .lock()
            .expect("the id counter is never poisoned");
        let pending = self
            .inner
            .pending
            .lock()
            .expect("the pending table is never poisoned");
        let mut candidate = *next;
        // Skip ids still awaiting replies; the space wraps 9999 → 1.
        while pending.contains_key(&PendingKey::Id(candidate.value())) {
            candidate = candidate.next();
        }
        *next = candidate.next();
        candidate
    }

    #[cfg(test)]
    fn set_next_id(&self, id: CommandId) {
        *self
            .inner
            .next_id
            .lock()
            .expect("the id counter is never poisoned") = id;
    }

    /// Sends a command while holding the locks its module requires, and
    /// waits for the correlated reply.
    fn transact<C: Command>(&self, command: &C) -> Result<RawResponse, StarError> {
        self.transact_frame(command.module(), C::CODE, &command.to_wire(None))
    }

    /// Sends one id-less frame: takes the locks the module requires, splices
    /// a freshly allocated id into the envelope, writes, and waits for the
    /// correlated reply under the code's read timeout.
    fn transact_frame(
        &self,
        module: Module,
        code: &str,
        id_less_frame: &str,
    ) -> Result<RawResponse, StarError> {
        if self.inner.dead.load(Ordering::Acquire) {
            return Err(StarError::Closed);
        }
        let query = is_query(code);
        // Queries run fully parallel; slave commands take the gate shared;
        // a master command takes it exclusively and drains the slaves.
        let (_read_gate, _write_gate) = if query {
            (None, None)
        } else if module == Module::Master {
            (
                None,
                Some(self.inner.gate.write().expect("the gate is never poisoned")),
            )
        } else {
            (
                Some(self.inner.gate.read().expect("the gate is never poisoned")),
                None,
            )
        };
        let module_mutex = (!query).then(|| self.inner.module_lock(module));
        let _module_guard = module_mutex
            .as_ref()
            .map(|m| m.lock().expect("module locks are never poisoned"));

        let id = self.allocate_id();
        let (sender, receiver) = mpsc::channel();
        self.inner
            .pending
            .lock()
            .expect("the pending table is never poisoned")
            .insert(PendingKey::Id(id.value()), sender);

        // The id parameter comes first, immediately after the four-character
        // envelope, so splicing it there reproduces exactly the frame the
        // typed builder would have produced with the id present.
        let wire = format!(
            "{}id{:04}{}",
            &id_less_frame[..4],
            id.value(),
            &id_less_frame[4..]
        );
        if let Err(error) = self.inner.transport.write_message(wire.as_bytes()) {
            self.inner
                .pending
                .lock()
                .expect("the pending table is never poisoned")
                .remove(&PendingKey::Id(id.value()));
            return Err(error.into());
        }

        let timeout = self
            .inner
            .read_timeout_override
            .lock()
            .expect("the override lock is never poisoned")
            .unwrap_or_else(|| read_timeout(code));
        match receiver.recv_timeout(timeout) {
            Ok(response) => Ok(response),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.inner
                    .pending
                    .lock()
                    .expect("the pending table is never poisoned")
                    .remove(&PendingKey::Id(id.value()));
                Err(StarError::Timeout {
                    code: code.to_string(),
                    seconds: timeout.as_secs(),
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(StarError::Transport(TransportError::Disconnected))
            }
        }
    }

    /// Executes a typed command: sends it, waits for the reply, decodes the
    /// error section, and parses the typed response. A trace-31
    /// unknown-parameter error triggers an automatic `VP` follow-up so the
    /// returned error names the offending parameter.
    pub fn execute<C: Command>(&self, command: &C) -> Result<C::Response, StarError> {
        if !C::EXPECTS_REPLY {
            // NS and AB (and R0 WP) produce no reply at all; waiting would
            // hang until the timeout.
            let wire = command.to_wire(None);
            self.inner.transport.write_message(wire.as_bytes())?;
            return C::parse_response("").map_err(Into::into);
        }
        let response = self.transact(command)?;
        let payload = self.decode_reply(&response)?;
        C::parse_response(&payload).map_err(Into::into)
    }

    /// Decodes a reply's error section into `Ok(payload)` or the typed
    /// firmware error, following up trace-31 with the `VP` faulty-parameter
    /// query.
    fn decode_reply(&self, response: &RawResponse) -> Result<String, StarError> {
        let (section, payload) = split_error_section(&response.payload)?;
        if let Some(section) = section
            && let Err(error) = FirmwareError::from_section(&section, &response.module)
        {
            if error.has_unknown_parameter_trace()
                && let Ok(vp_response) = self.transact(&RequestFaultyParameter)
                && let Ok((_, vp_payload)) = split_error_section(&vp_response.payload)
                && let Ok(parameter) = RequestFaultyParameter::parse_response(&vp_payload)
            {
                return Err(StarError::FaultyParameter { error, parameter });
            }
            return Err(error.into());
        }
        Ok(payload)
    }

    /// Replays one pre-authored id-less frame: the session assigns the id,
    /// applies the module locking and the code's read timeout, and decodes
    /// the error section. The reply payload (with the error section removed)
    /// comes back raw for the caller to interpret.
    pub fn execute_raw(&self, command: &RawCommand) -> Result<String, StarError> {
        let response = self.transact_frame(command.module(), command.code(), command.frame())?;
        self.decode_reply(&response)
    }

    // ------------------------------------------------------------------
    // Setup choreography
    // ------------------------------------------------------------------

    /// Runs the documented setup choreography: discover configuration and
    /// geometry, initialize the instrument when needed, discard stray tips
    /// over the tip waste, and prepare whichever optional modules are
    /// installed. Returns (and caches) what was discovered.
    pub fn initialize(&self, options: InitializeOptions) -> Result<MachineInfo, StarError> {
        let configuration = self.execute(&RequestMachineConfiguration)?;
        let extended = self.execute(&RequestExtendedConfiguration)?;
        let travel = self.execute(&RequestMaxXTravel)?;
        let working_envelope = self.execute(&RequestWorkingEnvelope)?;

        let was_initialized = self.execute(&QueryInitializationStatus::master())?;
        if !was_initialized {
            self.execute(&PreInitialize)?;
        } else {
            self.execute(&MoveAllChannelsToZSafety)?;
            if extended.has_core96_head() {
                self.execute(&Head96MoveToZSafety)?;
                self.execute(&Head96RequestZ)?;
            }
        }

        let tip_presence = self.execute(&RequestTipPresence)?;
        let channel_count = if tip_presence.is_empty() {
            configuration.channel_count
        } else {
            tip_presence.len()
        };

        if !was_initialized || tip_presence.iter().any(|&present| present) {
            let tip_type = self.define_tip_type(&TIP_5ML)?;
            let tip_waste_x = options.tip_waste_x.unwrap_or(extended.tip_waste_x.0);
            let targets = tip_waste_targets(tip_waste_x, channel_count);
            // The working defaults over the tip waste: deposit from 245.0
            // down to 122.0 mm, finish at 360.0 mm, place-and-shift.
            let discard = InitializeChannels::new(
                &targets,
                channel_count,
                2450,
                1220,
                3600,
                tip_type,
                TipDiscardMethod::PlaceAndShift,
            )?;
            self.execute(&discard)?;
        }

        let mut minimum_y_spacings = Vec::with_capacity(channel_count);
        for channel in 0..channel_count {
            minimum_y_spacings.push(self.execute(&RequestMinimumYSpacing::new(channel)?)?);
        }
        let channel_firmware = (channel_count > 0)
            .then(|| {
                self.execute(&RequestFirmwareVersion {
                    module: Module::PipettingChannel(0),
                })
            })
            .transpose()?;

        if configuration.has_autoload() {
            let autoload_ready = self.execute(&QueryInitializationStatus {
                module: Module::Autoload,
            })?;
            if !autoload_ready {
                self.execute(&AutoloadInitialize)?;
            }
            self.execute(&AutoloadToSafeZ)?;
            if let Some(track) = options.autoload_park_track {
                self.execute(&MoveAutoloadToTrack::new(track)?)?;
            }
        }

        let mut iswap_x_offset = None;
        if configuration.has_iswap() {
            let iswap_ready = self.execute(&QueryInitializationStatus {
                module: Module::Iswap,
            })?;
            if !iswap_ready {
                self.execute(&IswapInitialize)?;
            }
            self.execute(&IswapPark::default())?;
            let read = ReadMasterEepromOffset {
                offset: MasterEepromOffset::IswapXOffset,
            };
            let payload = self.execute(&read)?;
            iswap_x_offset = read.parse_offset(&payload).ok();
        }

        let mut head96_x_offset = None;
        let mut head96_firmware = None;
        let mut head96_info = None;
        let mut head96_type = None;
        if extended.has_core96_head() {
            let head_ready = self.execute(&QueryInitializationStatus {
                module: Module::Head96,
            })?;
            if !head_ready && let Some(trash) = options.head96_trash {
                self.execute(&trash)?;
            }
            head96_firmware = Some(self.execute(&RequestFirmwareVersion {
                module: Module::Head96,
            })?);
            head96_info = Some(self.execute(&Head96QueryInfo)?);
            head96_type = Some(self.execute(&Head96QueryType)?);
            let read = ReadMasterEepromOffset {
                offset: MasterEepromOffset::Head96XOffset,
            };
            let payload = self.execute(&read)?;
            head96_x_offset = read.parse_offset(&payload).ok();
        }

        let info = MachineInfo {
            configuration,
            extended,
            travel,
            working_envelope,
            channel_count,
            tip_presence,
            was_initialized,
            minimum_y_spacings,
            channel_firmware,
            iswap_x_offset,
            head96_x_offset,
            head96_firmware,
            head96_info,
            head96_type,
        };
        self.state
            .lock()
            .expect("the session state is never poisoned")
            .machine = Some(info.clone());
        Ok(info)
    }

    // ------------------------------------------------------------------
    // Tip types
    // ------------------------------------------------------------------

    /// The firmware index for a tip type, defining it on first use. The
    /// firmware table is volatile, so the cache lives with the session and
    /// re-defines types after a reconnect.
    pub fn define_tip_type(&self, tip: &TipType) -> Result<u32, StarError> {
        let key = tip.cache_key();
        {
            let state = self
                .state
                .lock()
                .expect("the session state is never poisoned");
            if let Some(&index) = state.tip_types.get(&key) {
                return Ok(index);
            }
        }
        let index = {
            let state = self
                .state
                .lock()
                .expect("the session state is never poisoned");
            state.next_tip_index
        };
        let definition = DefineTipType::new(
            index,
            tip.has_filter,
            tip.wire_length(),
            tip.wire_volume(),
            tip.size,
            tip.pickup_method,
        )?;
        self.execute(&definition)?;
        let mut state = self
            .state
            .lock()
            .expect("the session state is never poisoned");
        state.tip_types.insert(key, index);
        state.next_tip_index += 1;
        Ok(index)
    }

    // ------------------------------------------------------------------
    // Pipetting
    // ------------------------------------------------------------------

    fn machine_channels(&self) -> usize {
        self.state
            .lock()
            .expect("the session state is never poisoned")
            .machine
            .as_ref()
            .map(|machine| machine.channel_count)
            .unwrap_or(8)
    }

    /// Picks up tips: derives the firmware Z window from the tip-spot Z and
    /// the tip geometry, including the empirical size-class correction
    /// (low-volume +0.2 mm, non-standard −0.2 mm).
    pub fn pick_up_tips(&self, spots: &[TipSpot], tip: &TipType) -> Result<(), StarError> {
        let tip_type = self.define_tip_type(tip)?;
        let targets = spot_targets(spots);
        check_channel_spacing(&targets)?;
        let machine_channels = self.machine_channels();
        let z = spots.first().map(|spot| spot.z.0).unwrap_or(0.0);
        let begin = ((z + tip.total_length.0) * 10.0).round() as i32 + tip.pickup_z_correction();
        let travel = ((tip.total_length.0 - fitting_depth(tip.size).0) * 10.0).round() as i32;
        let end = begin - travel;
        let pickup = TipPickup::new(
            &targets,
            machine_channels,
            tip_type,
            begin.max(0) as u32,
            end.max(0) as u32,
            DEFAULT_TRAVERSE_HEIGHT,
            tip.pickup_method,
        )?;
        self.execute(&pickup)
    }

    /// Discards tips. With `PlaceAndShift` the deposit window references
    /// the tip cone end: deposit Z + 59.9 mm down to + 49.9 mm (empirical
    /// constants). With `Drop` it references the stop disk via the tip
    /// geometry.
    pub fn discard_tips(
        &self,
        spots: &[TipSpot],
        tip: &TipType,
        method: TipDiscardMethod,
    ) -> Result<TipDiscardReport, StarError> {
        let targets = spot_targets(spots);
        check_channel_spacing(&targets)?;
        let machine_channels = self.machine_channels();
        let z = spots.first().map(|spot| spot.z.0).unwrap_or(0.0);
        let (begin, end) = match method {
            TipDiscardMethod::PlaceAndShift => (
                ((z + 59.9) * 10.0).round() as u32,
                ((z + 49.9) * 10.0).round() as u32,
            ),
            TipDiscardMethod::Drop => (
                ((z + tip.total_length.0) * 10.0).round() as u32,
                ((z + tip.total_length.0 - fitting_depth(tip.size).0) * 10.0).round() as u32,
            ),
        };
        let discard = TipDiscard::new(
            &targets,
            machine_channels,
            begin,
            end,
            DEFAULT_TRAVERSE_HEIGHT,
            DEFAULT_TRAVERSE_HEIGHT,
            method,
        )?;
        self.execute(&discard)
    }

    /// Aspirates. The command carries every parameter explicitly; the
    /// session adds only the adjacent-channel spacing check.
    pub fn aspirate(&self, command: &Aspirate) -> Result<(), StarError> {
        self.execute(command)
    }

    /// Dispenses.
    pub fn dispense(&self, command: &Dispense) -> Result<(), StarError> {
        self.execute(command)
    }

    /// Moves every channel to the Z-safety height. Call before any X or
    /// arm motion and in error recovery.
    pub fn move_all_channels_to_z_safety(&self) -> Result<(), StarError> {
        self.execute(&MoveAllChannelsToZSafety)
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Per-channel tip presence; the reply length is the channel count.
    pub fn tip_presence(&self) -> Result<Vec<bool>, StarError> {
        self.execute(&RequestTipPresence)
    }

    /// Every channel's Y position, 0.1 mm.
    pub fn channel_y_positions(&self) -> Result<Vec<i64>, StarError> {
        self.execute(&RequestAllChannelY)
    }

    /// The last liquid-level-detection heights, 0.1 mm per channel.
    pub fn last_lld_heights(&self) -> Result<Vec<i64>, StarError> {
        self.execute(&RequestLastLldHeights)
    }

    /// A module's firmware version string.
    pub fn firmware_version(&self, module: Module) -> Result<String, StarError> {
        self.execute(&RequestFirmwareVersion { module })
    }

    /// Measures the mounted tip's length on one channel:
    /// `Px RZ − (C0 RD − 8 mm)`, returned in millimeters.
    pub fn measure_tip_length(&self, channel: usize) -> Result<Millimeters, StarError> {
        let stop_disk_increments = self.execute(&RequestStopDiskZ::new(channel)?)?;
        let tip_bottom_tenth_mm = self.execute(&RequestChannelTipZ::new(channel)?)?;
        let stop_disk_mm = Axis::CHANNEL_Z.units_from(stop_disk_increments);
        let tip_bottom_mm = tip_bottom_tenth_mm as f64 / 10.0;
        Ok(Millimeters(stop_disk_mm - (tip_bottom_mm - 8.0)))
    }

    /// Shuts the session down: the reader thread exits and pending waiters
    /// disconnect.
    pub fn close(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.dead.store(true, Ordering::Release);
        self.inner.fail_all_pending();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for Star {
    fn drop(&mut self) {
        self.close();
    }
}

fn reader_loop(inner: &Arc<Inner>) {
    while !inner.shutdown.load(Ordering::Acquire) {
        match inner.transport.read_message(Duration::from_millis(200)) {
            Ok(Some(bytes)) => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let Ok(response) = RawResponse::parse(&text) else {
                    // An unparseable frame cannot be correlated; drop it.
                    continue;
                };
                let key = match response.id {
                    Some(id) => PendingKey::Id(id.value()),
                    None => PendingKey::Envelope(format!("{}{}", response.module, response.code)),
                };
                let sender = inner
                    .pending
                    .lock()
                    .expect("the pending table is never poisoned")
                    .remove(&key);
                if let Some(sender) = sender {
                    let _ = sender.send(response);
                }
                // Replies with no waiting command are stale; they are
                // dropped so they can never satisfy a later command.
            }
            Ok(None) => {}
            Err(_) => {
                inner.dead.store(true, Ordering::Release);
                inner.fail_all_pending();
                break;
            }
        }
    }
}

fn spot_targets(spots: &[TipSpot]) -> Vec<ChannelTarget> {
    spots
        .iter()
        .map(|spot| ChannelTarget {
            channel: spot.channel,
            x: spot.x.to_wire().0,
            y: spot.y.to_wire().0,
        })
        .collect()
}

/// Validates the 9 mm minimum Y spacing between channels on adjacent
/// channel indices.
pub fn check_channel_spacing(targets: &[ChannelTarget]) -> Result<(), StarError> {
    let mut sorted: Vec<&ChannelTarget> = targets.iter().collect();
    sorted.sort_by_key(|target| target.channel);
    for pair in sorted.windows(2) {
        if pair[1].channel == pair[0].channel + 1 {
            let spacing = pair[0].y.abs_diff(pair[1].y);
            if spacing < MIN_CHANNEL_SPACING {
                return Err(StarError::ChannelSpacing {
                    first: pair[0].channel,
                    second: pair[1].channel,
                    spacing,
                });
            }
        }
    }
    Ok(())
}

/// The working default tip-discard targets over the tip waste: every
/// channel at the waste X, spread in Y from 405.0 mm down to 217.5 mm.
fn tip_waste_targets(tip_waste_x: u32, channel_count: usize) -> Vec<ChannelTarget> {
    (0..channel_count)
        .map(|i| {
            let y = if channel_count > 1 {
                4050 - (i as u32 * (4050 - 2175) / (channel_count as u32 - 1))
            } else {
                4050
            };
            ChannelTarget {
                channel: i,
                x: tip_waste_x,
                y,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::channel_direct::ChannelZMove;
    use crate::commands::pipetting::PositionChannelY;
    use crate::commands::system::TriggerNextStep;
    use crate::transport::MockTransport;

    /// Echoes `er00/00` success (or a slave `er00`) for any command, so the
    /// session can run without scripting each reply.
    fn success_responder(command: &str) -> Vec<String> {
        let envelope = &command[..4];
        let id = command.get(6..10).unwrap_or("0000");
        if envelope.starts_with("C0") {
            vec![format!(
                "{envelope}id{id}er00/00{}",
                canned_payload(envelope)
            )]
        } else {
            vec![format!("{envelope}id{id}er00{}", canned_payload(envelope))]
        }
    }

    fn canned_payload(envelope: &str) -> &'static str {
        match envelope {
            "C0RM" => "kb00kp02",
            "C0QM" => {
                "ka000000ke00000000xt04xa08xw04350xl00xn00xr00xo00xm00001xx00001xu0000xv0000kc0kr0ys000kl000km000ym0000yu0000yx0000"
            }
            "C0RU" => "00100 15450 00000 00000",
            "C0UA" => "00100 15450 00000 00000 00000 00000",
            "C0QW" => "qw1",
            "C0RT" => "rt0 0",
            "P1VY" => "yc194 194",
            "P2VY" => "yc194 194",
            "P1RF" => "rf1.0S 2009-06-24 A",
            _ => "",
        }
    }

    fn star_with_responder() -> (Arc<MockTransport>, Star) {
        let transport = Arc::new(MockTransport::new());
        transport.set_responder(success_responder);
        let star = Star::new(Arc::clone(&transport) as Arc<dyn Transport>)
            .expect("a mock session always opens");
        (transport, star)
    }

    #[test]
    fn ids_increment_and_wrap_from_9999_to_1() {
        let (transport, star) = star_with_responder();
        star.set_next_id(CommandId::new(9999).expect("9999 is a valid id"));
        star.execute(&MoveAllChannelsToZSafety)
            .expect("the responder answers ZA");
        star.execute(&MoveAllChannelsToZSafety)
            .expect("the responder answers ZA");
        let written = transport.written();
        assert_eq!(written[0], "C0ZAid9999", "the first command takes id 9999");
        assert_eq!(
            written[1], "C0ZAid0001",
            "the id space wraps to 1, skipping 0"
        );
    }

    #[test]
    fn out_of_order_responses_correlate_by_id() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|_| Vec::new());
        let star = Arc::new(star);

        let star_a = Arc::clone(&star);
        let a = std::thread::spawn(move || star_a.execute(&RequestTipPresence));
        let star_b = Arc::clone(&star);
        let b = std::thread::spawn(move || star_b.execute(&RequestAllChannelY));

        // Wait until both queries are on the wire, then answer them in
        // reverse order.
        while transport.written().len() < 2 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let written = transport.written();
        let id_of = |code: &str| {
            written
                .iter()
                .find(|w| w[2..4] == *code)
                .map(|w| w[6..10].to_string())
                .expect("both queries were written")
        };
        transport.push_response(&format!("C0RYid{}er00/00ry2418 2328", id_of("RY")));
        transport.push_response(&format!("C0RTid{}er00/00rt1 0", id_of("RT")));

        let presence = a.join().expect("thread a finishes").expect("RT succeeds");
        let positions = b.join().expect("thread b finishes").expect("RY succeeds");
        assert_eq!(
            presence,
            vec![true, false],
            "RT got the RT reply despite arriving second"
        );
        assert_eq!(
            positions,
            vec![2418, 2328],
            "RY got the RY reply despite arriving first"
        );
    }

    #[test]
    fn concurrent_channel_commands_serialize_on_the_shared_mutex() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|_| Vec::new());
        let star = Arc::new(star);

        let star_a = Arc::clone(&star);
        let first = std::thread::spawn(move || {
            star_a.execute(&ChannelZMove::new(0, 20000, 1000, 50, 3).expect("in range"))
        });
        while transport.written().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        let star_b = Arc::clone(&star);
        let second = std::thread::spawn(move || {
            star_b.execute(&ChannelZMove::new(1, 20000, 1000, 50, 3).expect("in range"))
        });

        // The second channel command must not reach the wire while the
        // first still holds the shared channel mutex.
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            transport.written().len(),
            1,
            "P2's command waits on the mutex all pipetting channels share"
        );

        let first_id = transport.written()[0][6..10].to_string();
        transport.push_response(&format!("P1ZAid{first_id}er00"));
        first.join().expect("thread finishes").expect("P1 succeeds");

        while transport.written().len() < 2 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let second_id = transport.written()[1][6..10].to_string();
        transport.push_response(&format!("P2ZAid{second_id}er00"));
        second
            .join()
            .expect("thread finishes")
            .expect("P2 succeeds");
    }

    #[test]
    fn a_master_command_waits_until_slave_commands_drain() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|_| Vec::new());
        let star = Arc::new(star);

        let star_slave = Arc::clone(&star);
        let slave = std::thread::spawn(move || {
            star_slave.execute(&ChannelZMove::new(0, 20000, 1000, 50, 3).expect("in range"))
        });
        while transport.written().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }

        let star_master = Arc::clone(&star);
        let master = std::thread::spawn(move || star_master.execute(&MoveAllChannelsToZSafety));

        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            transport.written().len(),
            1,
            "the exclusive C0 command waits for the in-flight slave command"
        );

        let slave_id = transport.written()[0][6..10].to_string();
        transport.push_response(&format!("P1ZAid{slave_id}er00"));
        slave
            .join()
            .expect("thread finishes")
            .expect("the slave command succeeds");

        while transport.written().len() < 2 {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            transport.written()[1].starts_with("C0ZA"),
            "the master command reaches the wire once the slaves drained"
        );
        let master_id = transport.written()[1][6..10].to_string();
        transport.push_response(&format!("C0ZAid{master_id}er00/00"));
        master
            .join()
            .expect("thread finishes")
            .expect("the master command succeeds");
    }

    #[test]
    fn a_missing_reply_times_out_naming_the_command() {
        let transport = Arc::new(MockTransport::new());
        let star = Star::new(Arc::clone(&transport) as Arc<dyn Transport>)
            .expect("a mock session always opens");
        star.set_read_timeout_override(Some(Duration::from_millis(50)));
        let error = star
            .execute(&MoveAllChannelsToZSafety)
            .expect_err("no responder answers, so the command times out");
        assert_eq!(
            error,
            StarError::Timeout {
                code: "ZA".to_string(),
                seconds: 0
            },
            "the timeout names the command code"
        );
    }

    #[test]
    fn no_reply_commands_return_without_waiting() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|_| Vec::new());
        star.execute(&TriggerNextStep)
            .expect("NS returns immediately");
        assert_eq!(
            transport.written(),
            vec!["C0NS".to_string()],
            "NS goes out id-less"
        );
        assert!(
            star.inner
                .pending
                .lock()
                .expect("the pending table is never poisoned")
                .is_empty(),
            "nothing waits for a reply that will never come"
        );
    }

    #[test]
    fn stale_responses_are_drained_before_the_session_starts() {
        let transport = Arc::new(MockTransport::new());
        transport.push_response("C0ZAid0001er00/00");
        let star = Star::new(Arc::clone(&transport) as Arc<dyn Transport>)
            .expect("a mock session always opens");
        star.set_read_timeout_override(Some(Duration::from_millis(50)));
        let error = star
            .execute(&MoveAllChannelsToZSafety)
            .expect_err("the stale id-0001 reply must not satisfy the new id-0001 command");
        assert_eq!(
            error,
            StarError::Timeout {
                code: "ZA".to_string(),
                seconds: 0
            },
            "the new command times out instead of consuming the stale reply"
        );
    }

    #[test]
    fn firmware_errors_decode_to_typed_errors() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            vec![format!("C0KYid{id}er08/00")]
        });
        let error = star
            .execute(&PositionChannelY::new(0, 2418).expect("in range"))
            .expect_err("the firmware reports no tips");
        assert_eq!(
            error,
            StarError::Firmware(FirmwareError::Master {
                code: crate::errors::MasterErrorCode::NoTips,
                trace: crate::errors::MasterTrace::None,
            }),
            "code 08 decodes to the no-tips error"
        );
    }

    #[test]
    fn trace_31_triggers_the_faulty_parameter_follow_up() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            if &command[2..4] == "VP" {
                vec![format!("C0VPid{id}er00/00vpqq")]
            } else {
                vec![format!("C0KYid{id}er01/31")]
            }
        });
        let error = star
            .execute(&PositionChannelY::new(0, 2418).expect("in range"))
            .expect_err("the firmware rejects an unknown parameter");
        let StarError::FaultyParameter { parameter, .. } = error else {
            panic!("trace 31 must produce the enriched faulty-parameter error, got {error:?}");
        };
        assert_eq!(
            parameter, "qq",
            "the VP follow-up names the offending parameter"
        );
    }

    #[test]
    fn the_setup_choreography_discovers_the_machine() {
        let (transport, star) = star_with_responder();
        let info = star
            .initialize(InitializeOptions::default())
            .expect("the scripted machine initializes");
        assert_eq!(
            info.channel_count, 2,
            "the RT reply length is the channel count"
        );
        assert!(
            info.was_initialized,
            "the scripted firmware reported itself initialized"
        );
        assert_eq!(
            info.minimum_y_spacings,
            vec![vec![194, 194], vec![194, 194]],
            "each channel's VY table is cached; 194 increments is the 9 mm floor"
        );
        assert_eq!(
            info.channel_firmware.as_deref(),
            Some("1.0S 2009-06-24 A"),
            "channel 1's firmware version is cached"
        );
        let written = transport.written();
        let codes: Vec<&str> = written.iter().map(|w| &w[..4]).collect();
        assert_eq!(
            &codes[..7],
            &["C0RM", "C0QM", "C0RU", "C0UA", "C0QW", "C0ZA", "C0RT"],
            "the choreography discovers configuration before touching motion"
        );
    }

    #[test]
    fn channel_spacing_below_9_mm_is_rejected_before_the_wire() {
        let targets = [
            ChannelTarget {
                channel: 0,
                x: 1179,
                y: 2418,
            },
            ChannelTarget {
                channel: 1,
                x: 1179,
                y: 2340,
            },
        ];
        let error = check_channel_spacing(&targets)
            .expect_err("78 tenth-mm between adjacent channels is below the firmware's 9 mm floor");
        assert_eq!(
            error,
            StarError::ChannelSpacing {
                first: 0,
                second: 1,
                spacing: 78
            },
            "the error names both channels and the offending spacing"
        );
    }

    #[test]
    fn a_raw_frame_gets_the_id_spliced_exactly_where_the_builder_puts_it() {
        // The id-less golden TP frame with the session's id spliced in must
        // reproduce the id-bearing golden wire string byte for byte.
        let (transport, star) = star_with_responder();
        star.set_next_id(CommandId::new(2).expect("2 is a valid id"));
        let command = RawCommand::parse(
            "C0TPxp01179 01179 00000&yp2418 2328 0000&tm1 1 0&tt01tp2244tz2164th2450td0",
        )
        .expect("the golden frame parses");
        assert_eq!(command.code(), "TP", "the code comes from the envelope");
        assert_eq!(
            command.read_timeout(),
            Duration::from_secs(120),
            "TP carries the tip-operation timeout"
        );
        star.execute_raw(&command)
            .expect("the responder answers TP");
        assert_eq!(
            transport.written(),
            vec![
                "C0TPid0002xp01179 01179 00000&yp2418 2328 0000&tm1 1 0&tt01tp2244tz2164th2450td0"
                    .to_string()
            ],
            "the spliced frame is the golden id-bearing wire string"
        );
    }

    #[test]
    fn raw_frames_with_their_own_id_are_rejected() {
        let error = RawCommand::parse("C0ZAid0007").expect_err("an id-bearing frame is not raw");
        assert_eq!(
            error,
            RawCommandError::AlreadyHasId {
                frame: "C0ZAid0007".to_string()
            },
            "the session owns id assignment"
        );
    }

    #[test]
    fn raw_frames_for_unknown_modules_are_rejected() {
        let error = RawCommand::parse("Q9ZA").expect_err("Q9 is not a module this crate knows");
        assert_eq!(
            error,
            RawCommandError::UnknownModule {
                frame: "Q9ZA".to_string(),
                address: "Q9".to_string()
            },
            "an unknown address cannot select locks or a trace table"
        );
    }

    #[test]
    fn raw_replay_decodes_firmware_errors_like_typed_execution() {
        let (transport, star) = star_with_responder();
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            vec![format!("C0ZAid{id}er08/00")]
        });
        let command = RawCommand::parse("C0ZA").expect("the retract frame parses");
        let error = star
            .execute_raw(&command)
            .expect_err("the firmware reports no tips");
        assert_eq!(
            error,
            StarError::Firmware(FirmwareError::Master {
                code: crate::errors::MasterErrorCode::NoTips,
                trace: crate::errors::MasterTrace::None,
            }),
            "raw replay goes through the same error decoding"
        );
    }

    #[test]
    fn tip_types_are_cached_by_value_and_defined_once() {
        let (transport, star) = star_with_responder();
        let first = star
            .define_tip_type(&crate::catalog::TIP_300UL_FILTER)
            .expect("the definition succeeds");
        let second = star
            .define_tip_type(&crate::catalog::TIP_300UL_FILTER)
            .expect("the cache answers");
        assert_eq!(first, second, "the same tip type keeps its index");
        let definitions = transport
            .written()
            .iter()
            .filter(|w| w[2..4] == *"TT")
            .count();
        assert_eq!(definitions, 1, "the firmware saw exactly one TT definition");
        assert_eq!(
            transport.written()[0],
            "C0TTid0001tt00tf1tl0519tv03600tg2tu0",
            "the definition carries the 300 µL filter tip's wire values at index 0"
        );
    }
}
