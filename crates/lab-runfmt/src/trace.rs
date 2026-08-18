//! `lab.sim-trace.v0`: the record a simulation leaves behind.
//!
//! The trace is the contract for all visualization. Every event carries the
//! virtual time it happened at; a viewer plays events and computes nothing.
//! Like the run formats, a change to what an event means is a new format
//! version, not an edit.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The format string every `lab.sim-trace.v0` document declares.
pub const SIM_TRACE_FORMAT: &str = "lab.sim-trace.v0";

/// One observable moment in a run walk. Events carry facts, not phrasing;
/// each consumer decides how to present them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum RunEvent {
    /// The walk is about to start `pending` nodes, skipping `completed`.
    Planned {
        pending: usize,
        completed: usize,
    },
    Connecting {
        station: String,
        detail: String,
    },
    Connected {
        station: String,
    },
    NodeStarted {
        id: String,
    },
    NodeSkipped {
        id: String,
    },
    NodeCompleted {
        id: String,
    },
    /// A station program began: a STAR frame sequence or a thermal profile.
    ProgramStarted {
        station: String,
        title: String,
        #[serde(flatten)]
        extent: ProgramExtent,
    },
    /// One STAR frame is about to execute. When the frame carries deck
    /// coordinates, the first channel's target rides along so a viewer
    /// can move a pipetting head to where the work happens.
    Frame {
        station: String,
        index: usize,
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x_mm: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y_mm: Option<f64>,
    },
    /// The thermal profile is running to completion on its station.
    ThermalRunning {
        station: String,
    },
    ThermalWarning {
        station: String,
        warning: String,
    },
    /// The block holds a temperature until retrieval.
    ThermalHold {
        station: String,
        celsius: f64,
    },
    DoorOpened {
        station: String,
    },
    DoorClosed {
        station: String,
    },
    /// The operator is needed, starting now.
    AttentionRequired {
        node: String,
        prompt: String,
    },
    /// The operator's step is done; the walk is unattended again.
    AttentionReleased {
        node: String,
    },
    /// Labware physically moved between stations.
    LabwareMoved {
        labware: String,
        from: String,
        to: String,
    },
}

/// How large a station program is, in the unit the station thinks in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramExtent {
    Frames {
        frames: usize,
    },
    Plateaus {
        plateaus: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_hold_celsius: Option<f64>,
    },
}

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
