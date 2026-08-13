//! The `lab simulate` command: the third interpreter of a run package.
//!
//! Simulation walks the same documents `lab run` executes, on a virtual
//! clock, and reports what the experiment costs in time: total duration,
//! when an operator must be present, and how long each walk-away window
//! lasts. The full record is written as a `lab.sim-trace.v0` trace.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lab_runtime::clock::{Clock, WallClock};
use lab_runtime::durations::DurationModel;
use lab_runtime::events::RunEvent;
use lab_runtime::simulate::{SimulationConfig, simulate_star_package, simulate_workcell};
use lab_runtime::trace::{SimTraceDocument, TimedEvent};
use lab_runtime::workcell::{is_workcell_directory, load_workcell_directory};

use crate::Output;

/// The trace file a simulation writes beside the package it simulated.
const TRACE_FILE: &str = "sim-trace.json";

/// Simulates one run directory and writes its trace beside the plan.
/// The idempotent core `lab render` reuses.
pub(crate) fn simulate_wave(
    directory: &Path,
    facility_path: Option<&Path>,
    trace_path: Option<PathBuf>,
) -> Result<(SimTraceDocument, PathBuf)> {
    let mut durations = DurationModel::default();
    let facility = facility_path
        .map(lab_runtime::facility::load_facility)
        .transpose()?;
    if let Some(facility) = &facility {
        // The facility's transport time is the whole handoff: seal, carry,
        // seat, confirm.
        durations.handoff_seconds = facility.transport.walk_seconds;
        if is_workcell_directory(directory) {
            let plan = lab_runfmt::load_workcell_plan(directory)?;
            facility.check_stations(&plan.stations)?;
        }
    }
    let config = SimulationConfig {
        origin_unix: WallClock.now_unix(),
        durations,
    };
    let trace = if is_workcell_directory(directory) {
        let loaded = load_workcell_directory(directory)?;
        simulate_workcell(&loaded, config)?
    } else {
        simulate_star_package(directory, config)?
    };

    let trace_path = trace_path.unwrap_or_else(|| directory.join(TRACE_FILE));
    let text = serde_json::to_string_pretty(&trace)?;
    std::fs::write(&trace_path, text)
        .with_context(|| format!("failed to write {}", trace_path.display()))?;
    Ok((trace, trace_path))
}

pub(crate) fn simulate(
    directory: PathBuf,
    trace_path: Option<PathBuf>,
    facility_path: Option<PathBuf>,
    output: &Output,
) -> Result<()> {
    let flow = crate::flow::resolve(&directory, facility_path)?;

    // One named run directory keeps its exact single-wave contract.
    if let [wave] = flow.waves.as_slice() {
        let (trace, written) = simulate_wave(wave, flow.facility.as_deref(), trace_path)?;
        let human = render_timeline(&trace, &written);
        return output.success("simulate", &trace, human);
    }

    let mut sections = Vec::new();
    let mut reports = Vec::new();
    for wave in &flow.waves {
        let label = crate::flow::wave_label(wave);
        let (trace, written) = simulate_wave(wave, flow.facility.as_deref(), None)?;
        sections.push(format!(
            "== {label} ==\n{}",
            render_timeline(&trace, &written)
        ));
        reports.push(serde_json::json!({
            "wave": label,
            "trace": written.display().to_string(),
            "summary": trace.summary,
        }));
    }
    output.success("simulate", reports, sections.join("\n\n"))
}

fn hms(seconds: f64) -> String {
    let total = seconds.round() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// One line per node, then the numbers that plan a lab day: attended
/// windows and the walk-away stretches between them.
fn render_timeline(trace: &SimTraceDocument, trace_path: &std::path::Path) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "simulated: {} node(s) in {} (durations: {} — estimates, calibrate against real ledgers)\n\n",
        trace.summary.nodes,
        hms(trace.summary.total_seconds),
        trace.durations,
    );

    // Node table from start/complete pairs, marking attended nodes.
    let mut started: Option<(&str, f64)> = None;
    for TimedEvent { t, event } in &trace.events {
        match event {
            RunEvent::NodeStarted { id } => started = Some((id, *t)),
            RunEvent::NodeCompleted { id } => {
                if let Some((start_id, from)) = started.take()
                    && start_id == id
                {
                    let attended = trace
                        .summary
                        .attention_windows
                        .iter()
                        .any(|window| window.node == *id);
                    let _ = writeln!(
                        text,
                        "  t+{}  {:<40} {:>9}  {}",
                        hms(from),
                        id,
                        hms(t - from),
                        if attended { "attended" } else { "unattended" }
                    );
                }
            }
            _ => {}
        }
    }

    let _ = write!(
        text,
        "\ntotal {}; attended {} in {} window(s); walk-away {}",
        hms(trace.summary.total_seconds),
        hms(trace.summary.attended_seconds),
        trace.summary.attention_windows.len(),
        hms(trace.summary.walkaway_seconds),
    );
    if let Some(longest) = longest_walkaway(trace) {
        let _ = write!(text, "; longest walk-away {}", hms(longest));
    }
    let _ = write!(text, "\ntrace: {}", trace_path.display());
    text
}

/// The longest stretch with no operator needed: the gaps between attention
/// windows, plus the run's unattended head and tail.
fn longest_walkaway(trace: &SimTraceDocument) -> Option<f64> {
    let windows = &trace.summary.attention_windows;
    if windows.is_empty() {
        return (trace.summary.total_seconds > 0.0).then_some(trace.summary.total_seconds);
    }
    let mut gaps = Vec::new();
    gaps.push(windows[0].from_seconds);
    for pair in windows.windows(2) {
        gaps.push(pair[1].from_seconds - pair[0].to_seconds);
    }
    gaps.push(trace.summary.total_seconds - windows[windows.len() - 1].to_seconds);
    gaps.into_iter().reduce(f64::max)
}
