//! The durable run ledger a workcell wave accumulates beside its plan.
//!
//! The ledger is the run's memory and its evidence: which nodes completed,
//! when, and on whose confirmation.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;

/// The ledger file a wave accumulates beside its plan.
pub const LEDGER_FILE: &str = "run-ledger.jsonl";

/// The durable ledger format for generic facility-wide execution plans.
pub const EXECUTION_LEDGER_FORMAT: &str = "lab.execution-ledger.v1";

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
    file.sync_data()
        .with_context(|| format!("failed to sync {}", path.display()))?;
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

/// A generic execution ledger already bound to one exact reviewed plan.
#[derive(Debug)]
pub struct ExecutionLedger {
    path: PathBuf,
    plan_sha256: String,
    valid_nodes: BTreeSet<String>,
    completed: BTreeSet<String>,
    started_at_unix_seconds: u64,
    last_completed_at_unix_seconds: Option<u64>,
}

impl ExecutionLedger {
    /// Creates a fresh ledger. Existing physical state is never overwritten.
    pub fn create(
        directory: &Path,
        plan_sha256: &str,
        inventory_sha256: &str,
        valid_nodes: BTreeSet<String>,
        clock: &dyn Clock,
    ) -> Result<Self> {
        let path = directory.join(LEDGER_FILE);
        let started_at_unix_seconds = clock.now_unix();
        let header = ExecutionLedgerRecord::Header {
            format: EXECUTION_LEDGER_FORMAT.to_owned(),
            plan_sha256: plan_sha256.to_owned(),
            inventory_sha256: inventory_sha256.to_owned(),
            started_at_unix_seconds,
        };
        let mut line = serde_json::to_string(&header)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "{} already exists; resume the reviewed plan instead of replacing durable physical state",
                    path.display()
                )
            })?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("failed to initialize {}", path.display()))?;
        file.sync_data()
            .with_context(|| format!("failed to sync {}", path.display()))?;
        Ok(Self {
            path,
            plan_sha256: plan_sha256.to_owned(),
            valid_nodes,
            completed: BTreeSet::new(),
            started_at_unix_seconds,
            last_completed_at_unix_seconds: None,
        })
    }

    /// Opens a prior ledger only when it belongs to the exact preflighted plan and inventory.
    pub fn resume(
        directory: &Path,
        plan_sha256: &str,
        inventory_sha256: &str,
        valid_nodes: BTreeSet<String>,
    ) -> Result<Self> {
        let path = directory.join(LEDGER_FILE);
        let text = fs::read_to_string(&path).with_context(|| {
            format!(
                "cannot resume because {} does not exist; start without --resume",
                path.display()
            )
        })?;
        let mut records = Vec::new();
        for (number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record =
                serde_json::from_str::<ExecutionLedgerRecord>(line).with_context(|| {
                    format!(
                        "{} line {} is not a {} record",
                        path.display(),
                        number + 1,
                        EXECUTION_LEDGER_FORMAT
                    )
                })?;
            records.push(record);
        }
        let Some(ExecutionLedgerRecord::Header {
            format,
            plan_sha256: recorded_plan,
            inventory_sha256: recorded_inventory,
            started_at_unix_seconds,
        }) = records.first()
        else {
            bail!(
                "{} has no {} header and cannot be resumed as a facility execution plan",
                path.display(),
                EXECUTION_LEDGER_FORMAT
            );
        };
        if format != EXECUTION_LEDGER_FORMAT {
            bail!(
                "{} declares ledger format '{}', expected '{}'",
                path.display(),
                format,
                EXECUTION_LEDGER_FORMAT
            );
        }
        if recorded_plan != plan_sha256 {
            bail!(
                "{} belongs to reviewed plan {}, but preflight loaded {}; substitutions require a new reviewed plan and a fresh run",
                path.display(),
                recorded_plan,
                plan_sha256
            );
        }
        if recorded_inventory != inventory_sha256 {
            bail!(
                "{} belongs to inventory {}, but the reviewed plan names {}",
                path.display(),
                recorded_inventory,
                inventory_sha256
            );
        }
        let started_at_unix_seconds = *started_at_unix_seconds;

        let mut completed = BTreeSet::new();
        let mut last_completed_at_unix_seconds: Option<u64> = None;
        for record in records.into_iter().skip(1) {
            match record {
                ExecutionLedgerRecord::Header { .. } => {
                    bail!("{} contains more than one ledger header", path.display())
                }
                ExecutionLedgerRecord::Node {
                    plan_sha256: entry_plan,
                    node,
                    event,
                    at_unix_seconds,
                } => {
                    if entry_plan != plan_sha256 {
                        bail!(
                            "{} contains a node record for reviewed plan {}, expected {}",
                            path.display(),
                            entry_plan,
                            plan_sha256
                        );
                    }
                    if !valid_nodes.contains(&node) {
                        bail!(
                            "{} records unknown node '{}'; it cannot resume this reviewed plan",
                            path.display(),
                            node
                        );
                    }
                    if event == LedgerEvent::Completed {
                        completed.insert(node);
                        last_completed_at_unix_seconds = Some(
                            last_completed_at_unix_seconds
                                .map_or(at_unix_seconds, |prior| prior.max(at_unix_seconds)),
                        );
                    }
                }
            }
        }
        Ok(Self {
            path,
            plan_sha256: plan_sha256.to_owned(),
            valid_nodes,
            completed,
            started_at_unix_seconds,
            last_completed_at_unix_seconds,
        })
    }

    pub fn completed_nodes(&self) -> &BTreeSet<String> {
        &self.completed
    }

    pub fn started_at_unix_seconds(&self) -> u64 {
        self.started_at_unix_seconds
    }

    pub fn last_completed_at_unix_seconds(&self) -> Option<u64> {
        self.last_completed_at_unix_seconds
    }

    /// Appends and syncs one node transition before the runner proceeds.
    pub fn append(&mut self, node: &str, event: LedgerEvent, clock: &dyn Clock) -> Result<()> {
        if !self.valid_nodes.contains(node) {
            bail!("cannot record unknown execution-plan node '{node}'");
        }
        let at_unix_seconds = clock.now_unix();
        let record = ExecutionLedgerRecord::Node {
            plan_sha256: self.plan_sha256.clone(),
            node: node.to_owned(),
            event,
            at_unix_seconds,
        };
        let mut line = serde_json::to_string(&record)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("failed to append to {}", self.path.display()))?;
        file.sync_data()
            .with_context(|| format!("failed to sync {}", self.path.display()))?;
        if event == LedgerEvent::Completed {
            self.completed.insert(node.to_owned());
            self.last_completed_at_unix_seconds = Some(at_unix_seconds);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum ExecutionLedgerRecord {
    Header {
        format: String,
        plan_sha256: String,
        inventory_sha256: String,
        started_at_unix_seconds: u64,
    },
    Node {
        plan_sha256: String,
        node: String,
        event: LedgerEvent,
        at_unix_seconds: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::WallClock;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_unix(&self) -> u64 {
            1_725_000_000
        }
    }

    fn nodes() -> BTreeSet<String> {
        ["prepare", "execute"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

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

    #[test]
    fn an_execution_ledger_is_bound_to_the_exact_reviewed_plan() {
        let directory = tempfile::tempdir().unwrap();
        let mut ledger = ExecutionLedger::create(
            directory.path(),
            &"a".repeat(64),
            &"b".repeat(64),
            nodes(),
            &FixedClock,
        )
        .unwrap();
        ledger
            .append("prepare", LedgerEvent::Started, &FixedClock)
            .unwrap();
        ledger
            .append("prepare", LedgerEvent::Completed, &FixedClock)
            .unwrap();
        ledger
            .append("execute", LedgerEvent::Started, &FixedClock)
            .unwrap();

        let resumed =
            ExecutionLedger::resume(directory.path(), &"a".repeat(64), &"b".repeat(64), nodes())
                .unwrap();

        assert_eq!(
            resumed.completed_nodes(),
            &["prepare".to_owned()].into_iter().collect()
        );
        let header = fs::read_to_string(directory.path().join(LEDGER_FILE)).unwrap();
        assert!(
            header
                .lines()
                .next()
                .unwrap()
                .contains(EXECUTION_LEDGER_FORMAT)
        );
        assert!(header.lines().next().unwrap().contains(&"a".repeat(64)));
    }

    #[test]
    fn resume_refuses_a_changed_plan_or_inventory() {
        let directory = tempfile::tempdir().unwrap();
        ExecutionLedger::create(
            directory.path(),
            &"a".repeat(64),
            &"b".repeat(64),
            nodes(),
            &FixedClock,
        )
        .unwrap();

        let changed_plan =
            ExecutionLedger::resume(directory.path(), &"c".repeat(64), &"b".repeat(64), nodes())
                .unwrap_err()
                .to_string();
        assert!(changed_plan.contains("substitutions require a new reviewed plan"));

        let changed_inventory =
            ExecutionLedger::resume(directory.path(), &"a".repeat(64), &"c".repeat(64), nodes())
                .unwrap_err()
                .to_string();
        assert!(changed_inventory.contains("belongs to inventory"));
    }

    #[test]
    fn a_fresh_execution_never_overwrites_an_existing_ledger() {
        let directory = tempfile::tempdir().unwrap();
        ExecutionLedger::create(
            directory.path(),
            &"a".repeat(64),
            &"b".repeat(64),
            nodes(),
            &FixedClock,
        )
        .unwrap();

        let error = ExecutionLedger::create(
            directory.path(),
            &"a".repeat(64),
            &"b".repeat(64),
            nodes(),
            &FixedClock,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("resume the reviewed plan"), "{error}");
    }
}
