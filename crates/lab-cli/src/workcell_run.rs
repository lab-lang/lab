//! The `lab run` command for a workcell wave: terminal presentation over
//! the runtime's workcell walk.

use std::path::PathBuf;

use anyhow::{Result, bail};
use lab_runtime::clock::WallClock;
use lab_runtime::events::{EventSink, ProgramExtent, RunEvent};
use lab_runtime::operator::StdinOperator;
use lab_runtime::stations::HardwareConnector;
use lab_runtime::workcell::{
    Bench, RunConfig, WorkcellOutcome, load_workcell_directory, parse_station_addresses,
    render_dry_run, run_workcell,
};

pub(crate) use lab_runtime::workcell::is_workcell_directory;

/// The terminal sink: narrates the walk the way an operator at the bench
/// reads it.
struct HumanSink;

impl EventSink for HumanSink {
    fn emit(&mut self, event: RunEvent) {
        match event {
            RunEvent::Planned { pending, completed } => println!(
                "about to execute {pending} coordination node(s){}",
                if completed == 0 {
                    String::new()
                } else {
                    format!(", resuming past {completed} completed")
                }
            ),
            RunEvent::Connecting { station, detail } => {
                println!("connecting to {station} ({detail})")
            }
            RunEvent::Connected { station } => println!("connected; {station} is ready"),
            RunEvent::NodeSkipped { id } => println!("skipping {id} (completed in the ledger)"),
            RunEvent::NodeStarted { .. } | RunEvent::NodeCompleted { .. } => {}
            RunEvent::DocumentStarted { .. } => {}
            RunEvent::ProgramStarted {
                station,
                title,
                extent,
            } => match extent {
                ProgramExtent::Frames { frames } => {
                    println!("\n{station}: {title} ({frames} frames)")
                }
                ProgramExtent::Plateaus { plateaus, .. } => {
                    println!("\n{station}: {title} ({plateaus} plateaus)")
                }
            },
            RunEvent::Frame {
                index, description, ..
            } => println!("  [{index:>3}] {description}"),
            RunEvent::ThermalRunning { .. } => println!(
                "running; completion may take hours — the wave resumes with --resume if interrupted"
            ),
            RunEvent::ThermalWarning { station, warning } => {
                println!("{station} warning: {warning}")
            }
            RunEvent::ThermalHold { celsius, .. } => {
                println!("holding the block at {celsius} °C until retrieval")
            }
            RunEvent::DoorOpened { station } => println!("{station} door is open"),
            RunEvent::DoorClosed { station } => println!("{station} door is closed"),
            RunEvent::AttentionRequired { prompt, .. } => println!("\nby hand — {prompt}"),
            RunEvent::AttentionReleased { .. } | RunEvent::LabwareMoved { .. } => {}
        }
    }
}

pub(crate) fn run_workcell_command(
    directory: PathBuf,
    dry_run: bool,
    yes: bool,
    resume: bool,
    station_addresses: Vec<String>,
    output: &crate::Output,
) -> Result<()> {
    let loaded = load_workcell_directory(&directory)?;
    let addresses = parse_station_addresses(&station_addresses)?;

    if dry_run {
        return output.success(
            "dry-run",
            serde_json::json!({ "nodes": loaded.nodes.len() }),
            render_dry_run(&loaded),
        );
    }

    let bench = Bench {
        thermocycler_station: loaded.thermocycler_station.clone(),
        addresses,
    };
    let config = RunConfig {
        assume_yes: yes,
        resume,
    };
    let mut connector = HardwareConnector;
    let mut operator = StdinOperator;
    let mut sink = HumanSink;
    let outcome = run_workcell(
        &directory,
        &loaded,
        &bench,
        &config,
        &mut connector,
        &mut operator,
        &mut sink,
        &WallClock,
    )?;
    match outcome {
        WorkcellOutcome::Completed { executed, skipped } => output.success(
            "run",
            serde_json::json!({ "nodes": executed, "skipped": skipped }),
            format!(
                "completed {executed} coordination node(s){}",
                if skipped == 0 {
                    String::new()
                } else {
                    format!(" ({skipped} skipped as already complete)")
                }
            ),
        ),
        WorkcellOutcome::Cancelled => bail!("run cancelled before any motion"),
        WorkcellOutcome::Declined { node } => bail!(
            "node '{node}' failed: the operator declined; the ledger holds every completed node — resolve the bench and continue with --resume"
        ),
        WorkcellOutcome::Failed { node, error } => bail!(
            "node '{node}' failed: {error}; the ledger holds every completed node — resolve the bench and continue with --resume"
        ),
    }
}
