//! `lab.sim-trace.v0`: the record a simulation leaves behind.
//!
//! The trace is the contract for all visualization. Every event carries the
//! virtual time it happened at; a viewer plays events and computes nothing.
//! Like the run formats, a change to what an event means is a new format
//! version, not an edit.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::clock::VirtualClock;
use crate::events::{EventSink, RunEvent};

/// The format string every `lab.sim-trace.v0` document declares.
pub const SIM_TRACE_FORMAT: &str = "lab.sim-trace.v0";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimTraceDocument {
    /// Always [`SIM_TRACE_FORMAT`]; readers reject any other value.
    pub format: String,
    /// What was simulated: the plan or manifest path, relative to the
    /// package directory.
    pub plan: String,
    /// The duration model's name; timings are estimates under that model.
    pub durations: String,
    pub events: Vec<TimedEvent>,
    pub summary: SimSummary,
}

/// One event at a virtual time, in seconds from the simulation's start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedEvent {
    pub t: f64,
    #[serde(flatten)]
    pub event: RunEvent,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SimSummary {
    pub total_seconds: f64,
    /// Seconds an operator must be present.
    pub attended_seconds: f64,
    /// Seconds the run proceeds without anyone watching.
    pub walkaway_seconds: f64,
    pub nodes: usize,
    pub stations: BTreeMap<String, StationSummary>,
    pub attention_windows: Vec<AttentionWindow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StationSummary {
    pub busy_seconds: f64,
}

/// One interval an operator is needed for, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttentionWindow {
    pub node: String,
    pub from_seconds: f64,
    pub to_seconds: f64,
}

/// The sink a simulation emits through: stamps every event with the shared
/// clock's current time.
pub struct TraceSink {
    clock: Rc<RefCell<VirtualClock>>,
    pub events: Vec<TimedEvent>,
}

impl TraceSink {
    pub fn new(clock: Rc<RefCell<VirtualClock>>) -> Self {
        Self {
            clock,
            events: Vec::new(),
        }
    }
}

impl EventSink for TraceSink {
    fn emit(&mut self, event: RunEvent) {
        self.events.push(TimedEvent {
            t: self.clock.borrow().elapsed_seconds(),
            event,
        });
    }
}

/// Derives the summary a trace carries: totals, attended intervals, and
/// per-station busy time, all from the recorded events.
pub fn summarize(events: &[TimedEvent], total_seconds: f64) -> SimSummary {
    let mut attention_windows = Vec::new();
    let mut open_attention: Option<(String, f64)> = None;
    let mut stations: BTreeMap<String, StationSummary> = BTreeMap::new();
    let mut open_program: Option<(String, f64)> = None;
    let mut nodes = 0usize;

    for timed in events {
        match &timed.event {
            RunEvent::AttentionRequired { node, .. } => {
                open_attention = Some((node.clone(), timed.t));
            }
            RunEvent::AttentionReleased { .. } => {
                if let Some((node, from_seconds)) = open_attention.take() {
                    attention_windows.push(AttentionWindow {
                        node,
                        from_seconds,
                        to_seconds: timed.t,
                    });
                }
            }
            RunEvent::ProgramStarted { station, .. } => {
                open_program = Some((station.clone(), timed.t));
            }
            RunEvent::NodeCompleted { .. } => {
                nodes += 1;
                if let Some((station, from)) = open_program.take() {
                    stations.entry(station).or_default().busy_seconds += timed.t - from;
                }
            }
            _ => {}
        }
    }

    let attended_seconds: f64 = attention_windows
        .iter()
        .map(|window| window.to_seconds - window.from_seconds)
        .sum();
    SimSummary {
        total_seconds,
        attended_seconds,
        walkaway_seconds: (total_seconds - attended_seconds).max(0.0),
        nodes,
        stations,
        attention_windows,
    }
}
