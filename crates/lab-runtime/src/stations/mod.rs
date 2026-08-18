//! Station sessions: the executor's view of connected instruments.
//!
//! The walk talks to stations through two narrow session traits, one per
//! program shape: frame replay on a liquid handler, and thermal programs on
//! a cycler. Live sessions wrap the vendor drivers; simulated sessions
//! (`sim`) advance a virtual clock instead of hardware. Which one a walk
//! gets is the [`Connector`]'s decision, made once per station name.

pub mod sim;

use std::collections::BTreeMap;

#[cfg(feature = "hardware")]
use anyhow::Context;
use anyhow::{Result, bail};
use hamilton_star::RawCommand;
use lab_instruments::{RunHandle, ThermalProfile};

use crate::events::EventSink;
#[cfg(feature = "hardware")]
use crate::events::RunEvent;
use crate::workcell::Bench;

/// A session that replays reviewed STAR frames.
pub trait StarSession {
    /// Executes one frame; the error is the firmware's meaning, as text.
    fn execute(&mut self, command: &RawCommand) -> Result<(), String>;
    /// Best-effort Z-safety retract after a failure; never fails louder
    /// than the failure it follows.
    fn retract(&mut self);
}

/// A session that runs device-neutral thermal programs.
pub trait CyclerSession {
    fn open_lid(&mut self) -> Result<()>;
    fn close_lid(&mut self) -> Result<()>;
    /// Drops any hold; the plate is out and nothing needs temperature.
    fn stop(&mut self) -> Result<()>;
    fn run_profile(&mut self, profile: &ThermalProfile) -> Result<RunHandle>;
    fn await_completion(&mut self, handle: RunHandle) -> Result<()>;
    fn hold_block(&mut self, celsius: f64) -> Result<()>;
    /// Warnings the device raised during the run, drained.
    fn take_warnings(&mut self) -> Vec<String>;
}

/// One open station, whichever shape it has.
pub enum StationSession {
    Star(Box<dyn StarSession>),
    Cycler(Box<dyn CyclerSession>),
}

/// Opens a session for a station the walk touches for the first time. The
/// live connector reaches hardware; the simulated connector builds models.
pub trait Connector {
    fn connect(
        &mut self,
        station: &str,
        kind: &str,
        bench: &Bench,
        events: &mut dyn EventSink,
    ) -> Result<StationSession>;
}

/// The open sessions a walk accumulates: each station connects on first
/// use, keyed by name, and stays open for the wave.
pub struct Sessions<'connector> {
    open: BTreeMap<String, StationSession>,
    connector: &'connector mut dyn Connector,
}

impl<'connector> Sessions<'connector> {
    pub fn new(connector: &'connector mut dyn Connector) -> Self {
        Self {
            open: BTreeMap::new(),
            connector,
        }
    }

    pub fn ensure(
        &mut self,
        station: &str,
        kind: &str,
        bench: &Bench,
        events: &mut dyn EventSink,
    ) -> Result<&mut StationSession> {
        if !self.open.contains_key(station) {
            let session = self.connector.connect(station, kind, bench, events)?;
            self.open.insert(station.to_string(), session);
        }
        Ok(self
            .open
            .get_mut(station)
            .expect("the session was just ensured"))
    }

    pub fn ensure_star(
        &mut self,
        station: &str,
        kind: &str,
        bench: &Bench,
        events: &mut dyn EventSink,
    ) -> Result<&mut dyn StarSession> {
        match self.ensure(station, kind, bench, events)? {
            StationSession::Star(session) => Ok(session.as_mut()),
            StationSession::Cycler(_) => {
                bail!("station '{station}' is a cycler, not a liquid handler")
            }
        }
    }

    pub fn ensure_cycler(
        &mut self,
        station: &str,
        kind: &str,
        bench: &Bench,
        events: &mut dyn EventSink,
    ) -> Result<&mut dyn CyclerSession> {
        match self.ensure(station, kind, bench, events)? {
            StationSession::Cycler(session) => Ok(session.as_mut()),
            StationSession::Star(_) => {
                bail!("station '{station}' is a liquid handler, not a cycler")
            }
        }
    }
}

/// A live STAR session over any transport.
pub struct LiveStar {
    star: hamilton_star::Star,
}

impl LiveStar {
    pub fn new(star: hamilton_star::Star) -> Self {
        Self { star }
    }
}

impl StarSession for LiveStar {
    fn execute(&mut self, command: &RawCommand) -> Result<(), String> {
        self.star
            .execute_raw(command)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn retract(&mut self) {
        let retract =
            RawCommand::parse("C0ZA").expect("the retract frame is a constant well-formed frame");
        let _ = self.star.execute_raw(&retract);
    }
}

impl CyclerSession for lab_instruments::OdtcStation {
    fn open_lid(&mut self) -> Result<()> {
        lab_instruments::Thermocycler::open_lid(self).map_err(anyhow::Error::from)
    }

    fn close_lid(&mut self) -> Result<()> {
        lab_instruments::Thermocycler::close_lid(self).map_err(anyhow::Error::from)
    }

    fn stop(&mut self) -> Result<()> {
        lab_instruments::Thermocycler::stop(self).map_err(anyhow::Error::from)
    }

    fn run_profile(&mut self, profile: &ThermalProfile) -> Result<RunHandle> {
        lab_instruments::Thermocycler::run_profile(self, profile).map_err(anyhow::Error::from)
    }

    fn await_completion(&mut self, handle: RunHandle) -> Result<()> {
        lab_instruments::Thermocycler::await_completion(self, handle).map_err(anyhow::Error::from)
    }

    fn hold_block(&mut self, celsius: f64) -> Result<()> {
        lab_instruments::Thermocycler::hold_block(self, celsius, None).map_err(anyhow::Error::from)
    }

    fn take_warnings(&mut self) -> Vec<String> {
        lab_instruments::OdtcStation::take_warnings(self)
    }
}

/// The connector live runs use: USB for the STAR, the bench's address for
/// the ODTC. Available only with the `hardware` feature so simulation
/// builds never link libusb.
#[cfg(feature = "hardware")]
pub struct HardwareConnector;

#[cfg(feature = "hardware")]
impl Connector for HardwareConnector {
    fn connect(
        &mut self,
        station: &str,
        kind: &str,
        bench: &Bench,
        events: &mut dyn EventSink,
    ) -> Result<StationSession> {
        match kind {
            "hamilton.star" => {
                events.emit(RunEvent::Connecting {
                    station: station.to_string(),
                    detail: "the first Hamilton STAR on USB".to_string(),
                });
                let star = hamilton_star::Star::open_usb().context(
                    "no Hamilton STAR answered on USB; use --dry-run to review without hardware",
                )?;
                star.initialize(hamilton_star::InitializeOptions::default())
                    .context(
                        "the setup choreography failed; the machine is not in a known state",
                    )?;
                events.emit(RunEvent::Connected {
                    station: station.to_string(),
                });
                Ok(StationSession::Star(Box::new(LiveStar::new(star))))
            }
            "inheco.odtc" => {
                let address = bench.addresses.get(station).with_context(|| {
                    format!(
                        "station '{station}' has no address on this bench; pass --station {station}=<ip:port> (the ODTC answers on port 8080)"
                    )
                })?;
                let socket: std::net::SocketAddr = address.parse().with_context(|| {
                    format!("'{address}' is not an <ip:port> address for station '{station}'")
                })?;
                events.emit(RunEvent::Connecting {
                    station: station.to_string(),
                    detail: socket.to_string(),
                });
                let session = lab_instruments::OdtcStation::connect(socket).with_context(|| {
                    format!("the {station} connection handshake failed at {socket}")
                })?;
                events.emit(RunEvent::Connected {
                    station: station.to_string(),
                });
                Ok(StationSession::Cycler(Box::new(session)))
            }
            other => bail!(
                "station '{station}' has kind '{other}', which this runner has no executor for"
            ),
        }
    }
}
