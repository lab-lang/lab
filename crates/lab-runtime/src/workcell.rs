//! The workcell walk: one node-by-node interpretation of a coordination
//! plan for live execution and dry-run review.
//!
//! `plan.workcell.json` names every node; station programs run on their
//! instruments, and every handoff or manual step stops for the operator's
//! confirmation. During a live run a durable ledger records each node as it
//! completes, so an interrupted wave — a crash, a power cut, an overnight
//! incubation — resumes from the first incomplete node with `--resume`
//! instead of repeating motion that already happened.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use hamilton_star::RawCommand;
use lab_runfmt::{StarRunDocument, ThermocycleRunDocument, WORKCELL_PLAN_FILE, WorkcellAction};

use crate::clock::Clock;
use crate::events::{EventSink, ProgramExtent, RunEvent};
use crate::ledger::{LEDGER_FILE, LedgerEvent, append_ledger, completed_nodes};
use crate::operator::{ConfirmKind, Operator};
use crate::stations::{Connector, Sessions};

/// A station program, loaded and validated up front so nothing is
/// discovered mid-walk.
pub enum LoadedProgram {
    Star {
        station: String,
        document: StarRunDocument,
        steps: Vec<(RawCommand, String)>,
    },
    Thermocycle {
        station: String,
        document: ThermocycleRunDocument,
    },
}

/// One executable unit of the walk, in plan order.
pub struct LoadedNode {
    pub id: String,
    /// Node ids that must complete first. The plan emits a linear chain
    /// today; the loader still validates this ordering rather than trusting it.
    pub after: Vec<String>,
    pub action: LoadedAction,
}

pub enum LoadedAction {
    Program(LoadedProgram),
    Handoff {
        from: String,
        to: String,
        labware: String,
        instructions: String,
    },
    Manual {
        title: String,
        instructions: String,
    },
}

impl LoadedAction {
    /// The operator-facing text for a handoff, naming the labware and both
    /// endpoints.
    pub fn handoff_prompt(from: &str, to: &str, labware: &str, instructions: &str) -> String {
        format!("{instructions} ({labware}: {from} -> {to})")
    }
}

pub struct LoadedWorkcell {
    pub nodes: Vec<LoadedNode>,
    /// The thermocycler station's name, when the plan declares one.
    pub thermocycler_station: Option<String>,
}

/// True when the directory holds a workcell coordination plan.
pub fn is_workcell_directory(directory: &Path) -> bool {
    directory.join(WORKCELL_PLAN_FILE).is_file()
}

/// Loads a wave directory: the coordination plan names every node, and
/// every referenced station document must parse and validate before
/// anything is reported ready. Thermal programs are checked against the
/// cycler's envelope here, so an unrunnable profile fails before any
/// motion — the same eagerness STAR frames get.
pub fn load_workcell_directory(directory: &Path) -> Result<LoadedWorkcell> {
    let plan = lab_runfmt::load_workcell_plan(directory)?;

    let station_kind = |name: &str| -> Result<&str> {
        plan.stations
            .iter()
            .find(|station| station.name == name)
            .map(|station| station.kind.as_str())
            .with_context(|| format!("the plan references station '{name}' it never declares"))
    };

    let mut nodes = Vec::new();
    for node in &plan.nodes {
        let action = match &node.action {
            WorkcellAction::StationProgram { station, document } => {
                let path = directory.join(document);
                match station_kind(station)? {
                    "hamilton.star" => {
                        let document = lab_runfmt::load_star_run(&path)?;
                        let steps = document
                            .steps
                            .iter()
                            .map(|step| {
                                RawCommand::parse(&step.frame)
                                    .map(|command| (command, step.description.clone()))
                                    .with_context(|| {
                                        format!("{} carries an unreplayable frame", path.display())
                                    })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        LoadedAction::Program(LoadedProgram::Star {
                            station: station.clone(),
                            document,
                            steps,
                        })
                    }
                    "inheco.odtc" => {
                        let document = lab_runfmt::load_thermocycle(&path)?;
                        document
                            .profile
                            .validate(&lab_instruments::odtc_thermal_limits())
                            .with_context(|| {
                                format!("'{}' is outside the {station} envelope", document.id)
                            })?;
                        LoadedAction::Program(LoadedProgram::Thermocycle {
                            station: station.clone(),
                            document,
                        })
                    }
                    other => bail!(
                        "station '{station}' has kind '{other}', which this runner has no executor for"
                    ),
                }
            }
            WorkcellAction::Handoff {
                from,
                to,
                labware,
                instructions,
            } => LoadedAction::Handoff {
                from: from.clone(),
                to: to.clone(),
                labware: labware.clone(),
                instructions: instructions.clone(),
            },
            WorkcellAction::Manual {
                title,
                instructions,
            } => LoadedAction::Manual {
                title: title.clone(),
                instructions: instructions.clone(),
            },
        };
        nodes.push(LoadedNode {
            id: node.id.clone(),
            after: node.after.clone(),
            action,
        });
    }
    let thermocycler_station = plan
        .stations
        .iter()
        .find(|station| station.kind == "inheco.odtc")
        .map(|station| station.name.clone());
    Ok(LoadedWorkcell {
        nodes,
        thermocycler_station,
    })
}

/// Renders the dry-run walk: every node in order, with program contents
/// summarized the way the live run narrates them.
pub fn render_dry_run(loaded: &LoadedWorkcell) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    let _ = writeln!(
        text,
        "dry run: {} coordination node(s), all documents validated",
        loaded.nodes.len()
    );
    for (index, node) in loaded.nodes.iter().enumerate() {
        match &node.action {
            LoadedAction::Program(LoadedProgram::Star {
                station,
                document,
                steps,
            }) => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} on {station} — {} ({} frames)",
                    index + 1,
                    node.id,
                    document.title,
                    steps.len()
                );
            }
            LoadedAction::Program(LoadedProgram::Thermocycle { station, document }) => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} on {station} — {} ({} plateaus{})",
                    index + 1,
                    node.id,
                    document.title,
                    document.profile.total_steps(),
                    match document.final_hold_celsius {
                        Some(celsius) => format!(", then hold {celsius} °C"),
                        None => String::new(),
                    }
                );
            }
            LoadedAction::Handoff {
                from,
                to,
                labware,
                instructions,
            } => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} — by hand: {}",
                    index + 1,
                    node.id,
                    LoadedAction::handoff_prompt(from, to, labware, instructions)
                );
            }
            LoadedAction::Manual {
                title,
                instructions,
            } => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} — by hand: {title}: {instructions}",
                    index + 1,
                    node.id
                );
            }
        }
    }
    text
}

/// Bench context the walk carries: which station is the cycler, and where
/// stations answer on this bench. Addresses are runtime input — compiled
/// artifacts never carry them.
pub struct Bench {
    pub thermocycler_station: Option<String>,
    pub addresses: BTreeMap<String, String>,
}

/// Parses repeated `--station NAME=ADDRESS` flags.
pub fn parse_station_addresses(entries: &[String]) -> Result<BTreeMap<String, String>> {
    let mut addresses = BTreeMap::new();
    for entry in entries {
        let Some((name, address)) = entry.split_once('=') else {
            bail!("--station takes NAME=ADDRESS, e.g. --station odtc-1=169.254.10.40:8080");
        };
        addresses.insert(name.to_string(), address.to_string());
    }
    Ok(addresses)
}

/// How one live walk was configured.
pub struct RunConfig {
    /// Skip the pre-run gate. Handoff and manual confirmations always ask:
    /// they attest that a physical step happened, and no flag can attest
    /// that for the operator.
    pub assume_yes: bool,
    pub resume: bool,
}

/// How a walk ended.
#[derive(Debug, PartialEq, Eq)]
pub enum WorkcellOutcome {
    Completed {
        executed: usize,
        skipped: usize,
    },
    /// The operator declined the pre-run gate; nothing moved.
    Cancelled,
    /// The operator declined a handoff or manual step mid-walk.
    Declined {
        node: String,
    },
    /// A station failed; the ledger holds every completed node.
    Failed {
        node: String,
        error: String,
    },
}

/// How one node's execution ended.
pub(crate) enum NodeRun {
    Done,
    Declined,
}

/// The live workcell walk: validate everything, then walk the plan in
/// order, recording each node in the ledger as it completes.
#[allow(clippy::too_many_arguments)]
pub fn run_workcell(
    directory: &Path,
    loaded: &LoadedWorkcell,
    bench: &Bench,
    config: &RunConfig,
    connector: &mut dyn Connector,
    operator: &mut dyn Operator,
    events: &mut dyn EventSink,
    clock: &dyn Clock,
) -> Result<WorkcellOutcome> {
    let completed = if config.resume {
        completed_nodes(directory)?
    } else {
        let ledger = directory.join(LEDGER_FILE);
        if ledger.is_file() {
            bail!(
                "{} already exists; a wave that stopped mid-run continues with --resume, and a fresh run of the same wave means physical state this runner cannot verify — remove the ledger only if the bench was truly reset",
                ledger.display()
            );
        }
        BTreeSet::new()
    };

    let pending = loaded
        .nodes
        .iter()
        .filter(|node| !completed.contains(&node.id))
        .count();
    events.emit(RunEvent::Planned {
        pending,
        completed: completed.len(),
    });
    if !config.assume_yes
        && !operator.confirm(ConfirmKind::PreRun, "proceed? Stations will move. [y/N] ")?
    {
        return Ok(WorkcellOutcome::Cancelled);
    }

    let mut sessions = Sessions::new(connector);
    let mut executed = 0usize;
    for node in &loaded.nodes {
        if completed.contains(&node.id) {
            events.emit(RunEvent::NodeSkipped {
                id: node.id.clone(),
            });
            continue;
        }
        append_ledger(directory, &node.id, LedgerEvent::Started, clock)?;
        events.emit(RunEvent::NodeStarted {
            id: node.id.clone(),
        });
        match execute_node(node, &mut sessions, bench, operator, events) {
            Ok(NodeRun::Done) => {
                append_ledger(directory, &node.id, LedgerEvent::Completed, clock)?;
                events.emit(RunEvent::NodeCompleted {
                    id: node.id.clone(),
                });
                executed += 1;
            }
            Ok(NodeRun::Declined) => {
                append_ledger(directory, &node.id, LedgerEvent::Failed, clock)?;
                return Ok(WorkcellOutcome::Declined {
                    node: node.id.clone(),
                });
            }
            Err(error) => {
                append_ledger(directory, &node.id, LedgerEvent::Failed, clock)?;
                return Ok(WorkcellOutcome::Failed {
                    node: node.id.clone(),
                    error: format!("{error:#}"),
                });
            }
        }
    }
    Ok(WorkcellOutcome::Completed {
        executed,
        skipped: completed.len(),
    })
}

/// True when a handoff endpoint is the cycler, whose motorized door the
/// runner must open before the operator can reach the block.
fn involves_cycler(bench: &Bench, station: &str) -> bool {
    bench
        .thermocycler_station
        .as_deref()
        .is_some_and(|cycler| cycler == station)
}

/// Executes one node against its station, confirming with the operator
/// wherever the plan needs hands.
pub(crate) fn execute_node(
    node: &LoadedNode,
    sessions: &mut Sessions,
    bench: &Bench,
    operator: &mut dyn Operator,
    events: &mut dyn EventSink,
) -> Result<NodeRun> {
    match &node.action {
        LoadedAction::Handoff {
            from,
            to,
            labware,
            instructions,
        } => {
            let to_cycler = involves_cycler(bench, to);
            let from_cycler = involves_cycler(bench, from);
            let cycler_endpoint = if to_cycler {
                Some(to.as_str())
            } else if from_cycler {
                Some(from.as_str())
            } else {
                None
            };
            if let Some(station) = cycler_endpoint {
                let cycler = sessions.ensure_cycler(station, "inheco.odtc", bench, events)?;
                cycler
                    .open_lid()
                    .with_context(|| format!("could not open the {station} door"))?;
                events.emit(RunEvent::DoorOpened {
                    station: station.to_string(),
                });
            }
            let prompt = LoadedAction::handoff_prompt(from, to, labware, instructions);
            events.emit(RunEvent::AttentionRequired {
                node: node.id.clone(),
                prompt,
            });
            let confirmed = operator.confirm(
                ConfirmKind::Handoff,
                "done, and the bench matches the plan? Continue [y/N] ",
            )?;
            events.emit(RunEvent::AttentionReleased {
                node: node.id.clone(),
            });
            if !confirmed {
                return Ok(NodeRun::Declined);
            }
            events.emit(RunEvent::LabwareMoved {
                labware: labware.clone(),
                from: from.clone(),
                to: to.clone(),
            });
            if let Some(station) = cycler_endpoint {
                let cycler = sessions.ensure_cycler(station, "inheco.odtc", bench, events)?;
                if from_cycler {
                    // The plate is out; nothing holds temperature for it now.
                    cycler
                        .stop()
                        .with_context(|| format!("could not stop {station} after retrieval"))?;
                }
                cycler
                    .close_lid()
                    .with_context(|| format!("could not close the {station} door"))?;
                events.emit(RunEvent::DoorClosed {
                    station: station.to_string(),
                });
            }
            Ok(NodeRun::Done)
        }
        LoadedAction::Manual {
            title,
            instructions,
        } => {
            events.emit(RunEvent::AttentionRequired {
                node: node.id.clone(),
                prompt: format!("{title}: {instructions}"),
            });
            let confirmed = operator.confirm(
                ConfirmKind::Manual,
                "done, and the bench matches the plan? Continue [y/N] ",
            )?;
            events.emit(RunEvent::AttentionReleased {
                node: node.id.clone(),
            });
            if confirmed {
                Ok(NodeRun::Done)
            } else {
                Ok(NodeRun::Declined)
            }
        }
        LoadedAction::Program(LoadedProgram::Star {
            station,
            document,
            steps,
        }) => {
            events.emit(RunEvent::ProgramStarted {
                station: station.clone(),
                title: document.title.clone(),
                extent: ProgramExtent::Frames {
                    frames: steps.len(),
                },
            });
            let star = sessions.ensure_star(station, "hamilton.star", bench, events)?;
            for (index, (command, description)) in steps.iter().enumerate() {
                events.emit(RunEvent::Frame {
                    station: station.clone(),
                    index: index + 1,
                    description: description.clone(),
                });
                if let Err(error) = star.execute(command) {
                    star.retract();
                    bail!(
                        "firmware error at frame {}: {error}; channels were retracted to Z-safety",
                        index + 1
                    );
                }
            }
            Ok(NodeRun::Done)
        }
        LoadedAction::Program(LoadedProgram::Thermocycle { station, document }) => {
            events.emit(RunEvent::ProgramStarted {
                station: station.clone(),
                title: document.title.clone(),
                extent: ProgramExtent::Plateaus {
                    plateaus: document.profile.total_steps(),
                    final_hold_celsius: document.final_hold_celsius,
                },
            });
            let cycler = sessions.ensure_cycler(station, "inheco.odtc", bench, events)?;
            let handle = cycler
                .run_profile(&document.profile)
                .with_context(|| format!("could not start '{}' on {station}", document.id))?;
            events.emit(RunEvent::ThermalRunning {
                station: station.clone(),
            });
            cycler
                .await_completion(handle)
                .with_context(|| format!("'{}' did not complete on {station}", document.id))?;
            for warning in cycler.take_warnings() {
                events.emit(RunEvent::ThermalWarning {
                    station: station.clone(),
                    warning,
                });
            }
            if let Some(celsius) = document.final_hold_celsius {
                cycler
                    .hold_block(celsius)
                    .with_context(|| format!("could not hold {celsius} °C on {station}"))?;
                events.emit(RunEvent::ThermalHold {
                    station: station.clone(),
                    celsius,
                });
            }
            Ok(NodeRun::Done)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::WallClock;
    use crate::events::{RecordingSink, RunEvent};
    use crate::operator::AutoOperator;
    use crate::testing::{TestConnector, write_synthetic_wave};

    fn bench_for(loaded: &LoadedWorkcell) -> Bench {
        Bench {
            thermocycler_station: loaded.thermocycler_station.clone(),
            addresses: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn a_wave_walks_in_order_and_records_every_node_in_the_ledger() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let loaded = load_workcell_directory(directory.path()).unwrap();
        let bench = bench_for(&loaded);
        let mut connector = TestConnector;
        let mut operator = AutoOperator { answer: true };
        let mut sink = RecordingSink::default();
        let outcome = run_workcell(
            directory.path(),
            &loaded,
            &bench,
            &RunConfig {
                assume_yes: true,
                resume: false,
            },
            &mut connector,
            &mut operator,
            &mut sink,
            &WallClock,
        )
        .unwrap();
        assert_eq!(
            outcome,
            WorkcellOutcome::Completed {
                executed: 5,
                skipped: 0
            }
        );
        let started: Vec<&str> = sink
            .events
            .iter()
            .filter_map(|event| match event {
                RunEvent::NodeStarted { id } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            [
                "assembly_run",
                "assembly_thermocycle.to-odtc-1",
                "assembly_thermocycle",
                "assembly_thermocycle.return",
                "assembly_run.manual-1",
            ],
            "the walk follows plan order"
        );
        let completed = crate::ledger::completed_nodes(directory.path()).unwrap();
        assert_eq!(completed.len(), 5, "every node is durable in the ledger");
        assert!(
            sink.events.iter().any(|event| matches!(
                event,
                RunEvent::LabwareMoved { labware, .. } if labware == "reaction_plate"
            )),
            "a confirmed handoff records the labware movement"
        );
        let doors: Vec<&RunEvent> = sink
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    RunEvent::DoorOpened { .. } | RunEvent::DoorClosed { .. }
                )
            })
            .collect();
        assert_eq!(
            doors.len(),
            4,
            "the cycler door opens and closes around each handoff"
        );
    }

    #[test]
    fn a_declining_operator_stops_the_walk_at_the_handoff() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let loaded = load_workcell_directory(directory.path()).unwrap();
        let bench = bench_for(&loaded);
        let mut connector = TestConnector;
        // Confirms the pre-run gate implicitly (assume_yes), then declines
        // the first handoff.
        let mut operator = AutoOperator { answer: false };
        let mut sink = RecordingSink::default();
        let outcome = run_workcell(
            directory.path(),
            &loaded,
            &bench,
            &RunConfig {
                assume_yes: true,
                resume: false,
            },
            &mut connector,
            &mut operator,
            &mut sink,
            &WallClock,
        )
        .unwrap();
        assert_eq!(
            outcome,
            WorkcellOutcome::Declined {
                node: "assembly_thermocycle.to-odtc-1".to_string()
            },
            "the walk stops at the first confirmation"
        );
        let completed = crate::ledger::completed_nodes(directory.path()).unwrap();
        assert_eq!(
            completed.len(),
            1,
            "only the STAR run completed before the decline"
        );
    }

    #[test]
    fn resume_skips_ledgered_nodes_and_a_fresh_run_refuses_a_ledger() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let loaded = load_workcell_directory(directory.path()).unwrap();
        let bench = bench_for(&loaded);
        crate::ledger::append_ledger(
            directory.path(),
            "assembly_run",
            crate::ledger::LedgerEvent::Completed,
            &WallClock,
        )
        .unwrap();

        let mut connector = TestConnector;
        let mut operator = AutoOperator { answer: true };
        let mut sink = RecordingSink::default();
        let fresh = run_workcell(
            directory.path(),
            &loaded,
            &bench,
            &RunConfig {
                assume_yes: true,
                resume: false,
            },
            &mut connector,
            &mut operator,
            &mut sink,
            &WallClock,
        );
        assert!(
            fresh.is_err(),
            "a pre-existing ledger without --resume is physical state the runner cannot verify"
        );

        let resumed = run_workcell(
            directory.path(),
            &loaded,
            &bench,
            &RunConfig {
                assume_yes: true,
                resume: true,
            },
            &mut connector,
            &mut operator,
            &mut sink,
            &WallClock,
        )
        .unwrap();
        assert_eq!(
            resumed,
            WorkcellOutcome::Completed {
                executed: 4,
                skipped: 1
            }
        );
        assert!(
            sink.events
                .iter()
                .any(|event| matches!(event, RunEvent::NodeSkipped { id } if id == "assembly_run")),
            "the ledgered node is skipped, not re-run"
        );
    }

    #[test]
    fn loading_validates_thermal_documents_before_any_motion() {
        let directory = tempfile::tempdir().unwrap();
        write_synthetic_wave(directory.path());
        let path = directory
            .path()
            .join("stations/odtc-1/assembly_thermocycle.odtc.json");
        let text = std::fs::read_to_string(&path).unwrap();
        // 240 °C is far outside any block envelope.
        std::fs::write(&path, text.replace("37.0", "240.0")).unwrap();
        let error = match load_workcell_directory(directory.path()) {
            Ok(_) => panic!("an unrunnable profile fails at load time"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("envelope"),
            "the error names the envelope: {error:#}"
        );
    }
}
