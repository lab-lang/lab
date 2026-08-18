//! Simulated stations: the same session traits the live walk drives, over
//! a virtual clock instead of hardware.
//!
//! A simulated station is honest about what it models: time, block
//! temperature, and door state. It never invents readings the real device
//! would measure.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Result, bail};
use hamilton_star::RawCommand;
use lab_instruments::{RunHandle, ThermalLimits, ThermalProfile, odtc_thermal_limits};

use crate::clock::VirtualClock;
use crate::durations::DurationModel;
use crate::events::EventSink;
use crate::stations::{Connector, CyclerSession, StarSession, StationSession};
use crate::workcell::Bench;

/// The one clock a simulation shares: every station and the modeled
/// operator advance it as their work completes.
pub type SharedClock = Rc<RefCell<VirtualClock>>;

/// A simulated STAR: every frame succeeds and costs its modeled time.
pub struct SimStar {
    clock: SharedClock,
    durations: Rc<DurationModel>,
}

impl SimStar {
    pub fn new(clock: SharedClock, durations: Rc<DurationModel>) -> Self {
        Self { clock, durations }
    }
}

impl StarSession for SimStar {
    fn execute(&mut self, command: &RawCommand) -> Result<(), String> {
        let frame = command.frame();
        let module = frame.get(..2).unwrap_or("");
        let seconds = self.durations.star_frame_seconds(module, command.code());
        self.clock.borrow_mut().advance(seconds);
        Ok(())
    }

    fn retract(&mut self) {
        let seconds = self.durations.star_frame_seconds("C0", "ZA");
        self.clock.borrow_mut().advance(seconds);
    }
}

/// A simulated thermocycler: profiles complete in exactly their computed
/// time, and the block temperature carries between programs.
pub struct SimThermocycler {
    clock: SharedClock,
    durations: Rc<DurationModel>,
    limits: ThermalLimits,
    block_celsius: f64,
    pending: Option<(RunHandle, f64, f64)>,
    next_handle: u64,
}

impl SimThermocycler {
    pub fn new(clock: SharedClock, durations: Rc<DurationModel>) -> Self {
        let ambient = durations.ambient_celsius;
        Self {
            clock,
            durations,
            limits: odtc_thermal_limits(),
            block_celsius: ambient,
            pending: None,
            next_handle: 1,
        }
    }
}

impl CyclerSession for SimThermocycler {
    fn open_lid(&mut self) -> Result<()> {
        self.clock.borrow_mut().advance(self.durations.door_seconds);
        Ok(())
    }

    fn close_lid(&mut self) -> Result<()> {
        self.clock.borrow_mut().advance(self.durations.door_seconds);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.pending = None;
        Ok(())
    }

    fn run_profile(&mut self, profile: &ThermalProfile) -> Result<RunHandle> {
        let (seconds, final_celsius) =
            self.durations
                .thermal_profile_seconds(profile, &self.limits, self.block_celsius);
        let handle = RunHandle::new(self.next_handle);
        self.next_handle += 1;
        self.pending = Some((handle, seconds, final_celsius));
        Ok(handle)
    }

    fn await_completion(&mut self, handle: RunHandle) -> Result<()> {
        let Some((pending, seconds, final_celsius)) = self.pending.take() else {
            bail!("no thermal program is running");
        };
        if pending != handle {
            bail!("the awaited handle is not the running program's");
        }
        self.clock.borrow_mut().advance(seconds);
        self.block_celsius = final_celsius;
        Ok(())
    }

    fn hold_block(&mut self, celsius: f64) -> Result<()> {
        let seconds =
            self.durations
                .thermal_ramp_seconds(&self.limits, self.block_celsius, celsius);
        self.clock.borrow_mut().advance(seconds);
        self.block_celsius = celsius;
        Ok(())
    }

    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// The connector a simulation uses: every station kind the live runner
/// knows gets a simulated session over the shared clock.
pub struct SimConnector {
    clock: SharedClock,
    durations: Rc<DurationModel>,
}

impl SimConnector {
    pub fn new(clock: SharedClock, durations: Rc<DurationModel>) -> Self {
        Self { clock, durations }
    }
}

impl Connector for SimConnector {
    fn connect(
        &mut self,
        station: &str,
        kind: &str,
        _bench: &Bench,
        _events: &mut dyn EventSink,
    ) -> Result<StationSession> {
        match kind {
            "hamilton.star" => Ok(StationSession::Star(Box::new(SimStar::new(
                self.clock.clone(),
                self.durations.clone(),
            )))),
            "inheco.odtc" => Ok(StationSession::Cycler(Box::new(SimThermocycler::new(
                self.clock.clone(),
                self.durations.clone(),
            )))),
            other => bail!(
                "station '{station}' has kind '{other}', which this simulator has no model for"
            ),
        }
    }
}
