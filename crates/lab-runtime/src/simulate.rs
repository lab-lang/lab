//! The simulation interpreter: the same walk the live runner performs,
//! over simulated stations, a modeled operator, and a virtual clock.
//!
//! Simulation writes no ledger — the ledger is evidence of physical work,
//! and none happened. Its record is the trace.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;

use anyhow::{Result, bail};

use crate::clock::VirtualClock;
use crate::durations::DurationModel;
use crate::events::{EventSink, ProgramExtent, RunEvent};
use crate::operator::{ConfirmKind, Operator};
use crate::stations::Sessions;
use crate::stations::sim::{SharedClock, SimConnector};
use crate::trace::{SIM_TRACE_FORMAT, SimTraceDocument, TraceSink, summarize};
use crate::workcell::{Bench, LoadedWorkcell, NodeRun, execute_node};

/// How a simulation is configured.
pub struct SimulationConfig {
    /// The wall time the virtual clock starts at, for ledger-comparable
    /// timestamps in reports. Timing inside the trace is relative seconds.
    pub origin_unix: u64,
    pub durations: DurationModel,
}

/// The operator a simulation models: confirms everything, and charges the
/// modeled human time for each kind of step.
struct SimOperator {
    clock: SharedClock,
    durations: Rc<DurationModel>,
}

impl Operator for SimOperator {
    fn confirm(&mut self, kind: ConfirmKind, _prompt: &str) -> Result<bool> {
        let seconds = match kind {
            ConfirmKind::PreRun => 0.0,
            ConfirmKind::Handoff => self.durations.handoff_seconds,
            ConfirmKind::Manual => self.durations.manual_seconds,
        };
        self.clock.borrow_mut().advance(seconds);
        Ok(true)
    }
}

/// Simulates one workcell wave and returns its trace.
///
/// The plan's `after` edges are asserted as the walk proceeds: today's
/// emitters produce a linear chain, and a future DAG plan must fail loudly
/// here rather than be silently walked in document order.
pub fn simulate_workcell(
    loaded: &LoadedWorkcell,
    config: SimulationConfig,
) -> Result<SimTraceDocument> {
    let clock: SharedClock = Rc::new(RefCell::new(VirtualClock::new(config.origin_unix)));
    let durations = Rc::new(config.durations);
    let bench = Bench {
        thermocycler_station: loaded.thermocycler_station.clone(),
        addresses: BTreeMap::new(),
    };
    let mut connector = SimConnector::new(clock.clone(), durations.clone());
    let mut operator = SimOperator {
        clock: clock.clone(),
        durations: durations.clone(),
    };
    let mut sink = TraceSink::new(clock.clone());
    let mut sessions = Sessions::new(&mut connector);

    let mut done: BTreeSet<&str> = BTreeSet::new();
    for node in &loaded.nodes {
        for dependency in &node.after {
            if !done.contains(dependency.as_str()) {
                bail!(
                    "node '{}' depends on '{dependency}', which has not run; this plan is not the linear chain this simulator walks — simulating dependency graphs is not supported yet",
                    node.id
                );
            }
        }
        sink.emit(RunEvent::NodeStarted {
            id: node.id.clone(),
        });
        match execute_node(node, &mut sessions, &bench, &mut operator, &mut sink)? {
            NodeRun::Done => {}
            NodeRun::Declined => bail!("the simulated operator declined; this cannot happen"),
        }
        sink.emit(RunEvent::NodeCompleted {
            id: node.id.clone(),
        });
        done.insert(node.id.as_str());
    }

    let total_seconds = clock.borrow().elapsed_seconds();
    let summary = summarize(&sink.events, total_seconds);
    Ok(SimTraceDocument {
        format: SIM_TRACE_FORMAT.to_string(),
        plan: lab_runfmt::WORKCELL_PLAN_FILE.to_string(),
        durations: durations.name.clone(),
        events: sink.events,
        summary,
    })
}

/// Simulates a single-station Hamilton STAR package: every run document's
/// frames at their modeled cost, with the manual steps that follow each
/// run charged as attended time.
pub fn simulate_star_package(
    directory: &Path,
    config: SimulationConfig,
) -> Result<SimTraceDocument> {
    let (runs, _autoload_park_track) = crate::star::load_run_directory(directory)?;
    let clock: SharedClock = Rc::new(RefCell::new(VirtualClock::new(config.origin_unix)));
    let durations = Rc::new(config.durations);
    let mut sink = TraceSink::new(clock.clone());

    for run in &runs {
        sink.emit(RunEvent::NodeStarted { id: run.id.clone() });
        sink.emit(RunEvent::ProgramStarted {
            station: "hamilton.star".to_string(),
            title: run.title.clone(),
            extent: ProgramExtent::Frames {
                frames: run.steps.len(),
            },
        });
        for (index, step) in run.steps.iter().enumerate() {
            let position = crate::events::frame_position(step.command.frame());
            sink.emit(RunEvent::Frame {
                station: "hamilton.star".to_string(),
                index: index + 1,
                description: step.description.clone(),
                x_mm: position.map(|(x, _)| x),
                y_mm: position.map(|(_, y)| y),
            });
            let frame = step.command.frame();
            let module = frame.get(..2).unwrap_or("");
            let seconds = durations.star_frame_seconds(module, step.command.code());
            clock.borrow_mut().advance(seconds);
        }
        sink.emit(RunEvent::NodeCompleted { id: run.id.clone() });
        for manual in &run.manual_after {
            let id = format!("{}.manual", run.id);
            sink.emit(RunEvent::NodeStarted { id: id.clone() });
            sink.emit(RunEvent::AttentionRequired {
                node: id.clone(),
                prompt: format!("{}: {}", manual.title, manual.instructions),
            });
            clock.borrow_mut().advance(durations.manual_seconds);
            sink.emit(RunEvent::AttentionReleased { node: id.clone() });
            sink.emit(RunEvent::NodeCompleted { id });
        }
    }

    let total_seconds = clock.borrow().elapsed_seconds();
    let summary = summarize(&sink.events, total_seconds);
    Ok(SimTraceDocument {
        format: SIM_TRACE_FORMAT.to_string(),
        plan: "automation_manifest.json".to_string(),
        durations: durations.name.clone(),
        events: sink.events,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::write_synthetic_wave;
    use crate::workcell::load_workcell_directory;

    #[test]
    fn a_synthetic_wave_simulates_to_its_exact_modeled_duration() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let loaded = load_workcell_directory(directory.path()).unwrap();
        let trace = simulate_workcell(
            &loaded,
            SimulationConfig {
                origin_unix: 0,
                durations: DurationModel::default(),
            },
        )
        .unwrap();

        let model = DurationModel::default();
        // STAR run: one C0TT and one C0ZA frame.
        let star = model.star_frame_seconds("C0", "TT") + model.star_frame_seconds("C0", "ZA");
        // Each cycler handoff: door open, human, door close.
        let handoff = model.door_seconds + model.handoff_seconds + model.door_seconds;
        // Thermal: 25 °C ambient to 37 °C at the 4.4 °C/s maximum, then 90 s.
        let thermal = (37.0 - model.ambient_celsius) / 4.4 + 90.0;
        let manual = model.manual_seconds;
        let expected = star + handoff + thermal + handoff + manual;
        assert!(
            (trace.summary.total_seconds - expected).abs() < 1e-9,
            "expected {expected}, got {}",
            trace.summary.total_seconds
        );

        // Attended: two handoffs and the manual step.
        let attended = 2.0 * model.handoff_seconds + model.manual_seconds;
        assert!(
            (trace.summary.attended_seconds - attended).abs() < 1e-9,
            "expected {attended} attended, got {}",
            trace.summary.attended_seconds
        );
        assert_eq!(trace.summary.attention_windows.len(), 3);
        assert_eq!(trace.summary.nodes, 5);
        assert!(
            trace.summary.stations.contains_key("star-1")
                && trace.summary.stations.contains_key("odtc-1"),
            "both stations report busy time"
        );

        // The trace round-trips as a document. Timestamps may drift by an
        // ulp through JSON floats, so equality is structural plus a
        // tolerance on the clock.
        let text = serde_json::to_string(&trace).unwrap();
        let back: SimTraceDocument = serde_json::from_str(&text).unwrap();
        assert_eq!(back.format, trace.format);
        assert_eq!(back.events.len(), trace.events.len());
        assert_eq!(back.summary.nodes, trace.summary.nodes);
        assert!((back.summary.total_seconds - trace.summary.total_seconds).abs() < 1e-6);
    }

    #[test]
    fn simulation_leaves_no_ledger_behind() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let loaded = load_workcell_directory(directory.path()).unwrap();
        simulate_workcell(
            &loaded,
            SimulationConfig {
                origin_unix: 0,
                durations: DurationModel::default(),
            },
        )
        .unwrap();
        assert!(
            !directory.path().join(crate::ledger::LEDGER_FILE).exists(),
            "the ledger is evidence of physical work, and none happened"
        );
    }

    #[test]
    fn an_out_of_order_plan_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let mut loaded = load_workcell_directory(directory.path()).unwrap();
        loaded.nodes.swap(0, 4);
        let error = simulate_workcell(
            &loaded,
            SimulationConfig {
                origin_unix: 0,
                durations: DurationModel::default(),
            },
        )
        .expect_err("a broken chain is refused, not silently walked");
        assert!(
            error.to_string().contains("depends on"),
            "the error names the unmet dependency: {error}"
        );
    }
}
