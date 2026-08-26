//! Test fixtures: a synthetic workcell wave with the same shape the
//! workcell backend emits — one STAR run, a thermal program bracketed by
//! two handoffs, and a trailing manual step.

use std::path::Path;

use anyhow::{Result, bail};
use hamilton_star::RawCommand;
use lab_instruments::{RunHandle, ThermalProfile};

use crate::events::EventSink;
use crate::stations::{Connector, CyclerSession, StarSession, StationSession};
use crate::workcell::Bench;

struct TestStar;

impl StarSession for TestStar {
    fn execute(&mut self, _command: &RawCommand) -> Result<(), String> {
        Ok(())
    }

    fn retract(&mut self) {}
}

#[derive(Default)]
struct TestCycler {
    next_handle: u64,
}

impl CyclerSession for TestCycler {
    fn open_lid(&mut self) -> Result<()> {
        Ok(())
    }

    fn close_lid(&mut self) -> Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn run_profile(&mut self, _profile: &ThermalProfile) -> Result<RunHandle> {
        self.next_handle += 1;
        Ok(RunHandle::new(self.next_handle))
    }

    fn await_completion(&mut self, _handle: RunHandle) -> Result<()> {
        Ok(())
    }

    fn hold_block(&mut self, _celsius: f64) -> Result<()> {
        Ok(())
    }

    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// A no-I/O connector used by workcell unit tests.
#[derive(Default)]
pub(crate) struct TestConnector;

impl Connector for TestConnector {
    fn connect(
        &mut self,
        station: &str,
        kind: &str,
        _bench: &Bench,
        _events: &mut dyn EventSink,
    ) -> Result<StationSession> {
        match kind {
            "hamilton.star" => Ok(StationSession::Star(Box::new(TestStar))),
            "inheco.odtc" => Ok(StationSession::Cycler(Box::<TestCycler>::default())),
            other => bail!("test station '{station}' has unsupported kind '{other}'"),
        }
    }
}

pub(crate) fn write_synthetic_wave(directory: &Path) {
    let plan = serde_json::json!({
        "format": "lab.workcell-run.v0",
        "stations": [
            { "name": "star-1", "kind": "hamilton.star", "program_dir": "stations/star-1" },
            { "name": "odtc-1", "kind": "inheco.odtc", "program_dir": "stations/odtc-1" }
        ],
        "nodes": [
            { "id": "assembly_run", "after": [], "action": "station-program",
              "station": "star-1", "document": "stations/star-1/assembly_run.star.json" },
            { "id": "assembly_thermocycle.to-odtc-1", "after": ["assembly_run"],
              "action": "handoff", "from": "star-1", "to": "odtc-1",
              "labware": "reaction_plate",
              "instructions": "Seal the reaction_plate and move it from star-1 to odtc-1; close the door." },
            { "id": "assembly_thermocycle", "after": ["assembly_thermocycle.to-odtc-1"],
              "action": "station-program", "station": "odtc-1",
              "document": "stations/odtc-1/assembly_thermocycle.odtc.json" },
            { "id": "assembly_thermocycle.return", "after": ["assembly_thermocycle"],
              "action": "handoff", "from": "odtc-1", "to": "star-1",
              "labware": "reaction_plate",
              "instructions": "Retrieve the reaction_plate from odtc-1 and return it to the star-1 deck position it came from." },
            { "id": "assembly_run.manual-1", "after": ["assembly_thermocycle.return"],
              "action": "manual", "title": "spread plates",
              "instructions": "spread the transformation on selective agar" }
        ]
    });
    let star_run = serde_json::json!({
        "format": "lab.star-run.v0",
        "run": "assembly_run",
        "title": "Golden Gate assembly",
        "machine": "STARlet",
        "channels": 8,
        "steps": [
            { "frame": "C0TTtt00tf1tl0519tv03600tg2tu0", "module": "C0", "code": "TT",
              "description": "define the small tip" },
            { "frame": "C0ZA", "module": "C0", "code": "ZA",
              "description": "retract all channels to Z-safety" }
        ]
    });
    let thermocycle = serde_json::json!({
        "format": "lab.thermocycle-run.v0",
        "id": "assembly_thermocycle",
        "title": "Golden Gate cycling",
        "plate": "reaction_plate",
        "profile": { "stages": [
            { "steps": [ { "celsius": 37.0, "hold_seconds": 90.0 } ], "repeats": 1 }
        ] },
        "fill_volume_ul": 20.0
    });
    std::fs::create_dir_all(directory.join("stations/star-1")).unwrap();
    std::fs::create_dir_all(directory.join("stations/odtc-1")).unwrap();
    std::fs::write(
        directory.join("plan.workcell.json"),
        serde_json::to_string_pretty(&plan).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("stations/star-1/assembly_run.star.json"),
        serde_json::to_string_pretty(&star_run).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("stations/odtc-1/assembly_thermocycle.odtc.json"),
        serde_json::to_string_pretty(&thermocycle).unwrap(),
    )
    .unwrap();
}
