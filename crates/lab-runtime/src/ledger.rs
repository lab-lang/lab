//! The durable run ledger a workcell wave accumulates beside its plan.
//!
//! The ledger is the run's memory and its evidence: which nodes completed,
//! when, and on whose confirmation. Only live runs write it; a simulation
//! records a trace instead, so a simulated wave never blocks a real one
//! from starting fresh.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;

/// The ledger file a wave accumulates beside its plan.
pub const LEDGER_FILE: &str = "run-ledger.jsonl";

/// One appended ledger record.
#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub node: String,
    pub event: LedgerEvent,
    /// Wall-clock seconds since the Unix epoch.
    pub at_unix_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LedgerEvent {
    Started,
    Completed,
    Failed,
}

/// Appends one entry; every event is durable before the walk continues.
pub fn append_ledger(
    directory: &Path,
    node: &str,
    event: LedgerEvent,
    clock: &dyn Clock,
) -> Result<()> {
    let entry = LedgerEntry {
        node: node.to_string(),
        event,
        at_unix_seconds: clock.now_unix(),
    };
    let mut line = serde_json::to_string(&entry)?;
    line.push('\n');
    let path = directory.join(LEDGER_FILE);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to append to {}", path.display()))?;
    Ok(())
}

/// The node ids the ledger records as completed.
pub fn completed_nodes(directory: &Path) -> Result<BTreeSet<String>> {
    let path = directory.join(LEDGER_FILE);
    if !path.is_file() {
        return Ok(BTreeSet::new());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut completed = BTreeSet::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: LedgerEntry = serde_json::from_str(line).with_context(|| {
            format!(
                "{} line {} is not a ledger entry",
                path.display(),
                number + 1
            )
        })?;
        if entry.event == LedgerEvent::Completed {
            completed.insert(entry.node);
        }
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::WallClock;

    #[test]
    fn the_ledger_round_trips_and_reports_completed_nodes() {
        let directory = tempfile::tempdir().unwrap();
        let clock = WallClock;
        append_ledger(
            directory.path(),
            "assembly_run",
            LedgerEvent::Started,
            &clock,
        )
        .unwrap();
        append_ledger(
            directory.path(),
            "assembly_run",
            LedgerEvent::Completed,
            &clock,
        )
        .unwrap();
        append_ledger(
            directory.path(),
            "assembly_thermocycle",
            LedgerEvent::Started,
            &clock,
        )
        .unwrap();
        let completed = completed_nodes(directory.path()).unwrap();
        assert!(
            completed.contains("assembly_run"),
            "a completed node is remembered"
        );
        assert!(
            !completed.contains("assembly_thermocycle"),
            "a started-but-unfinished node is not skipped on resume"
        );
    }
}
