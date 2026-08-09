//! Live execution of an emitted workcell wave.
//!
//! `lab run <wave dir>` on a directory holding `plan.workcell.json` walks
//! the coordination plan in order: station programs run on their
//! instruments, and every handoff or manual step stops for the operator's
//! confirmation. A durable ledger records each node as it completes, so an
//! interrupted wave — a crash, a power cut, an overnight incubation —
//! resumes from the first incomplete node with `--resume` instead of
//! repeating motion that already happened.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use hamilton_star::RawCommand;
use lab_compiler::runfmt::{
    STAR_RUN_FORMAT, StarRunDocument, THERMOCYCLE_RUN_FORMAT, ThermocycleRunDocument,
    WORKCELL_RUN_FORMAT, WorkcellAction, WorkcellRunDocument,
};
use serde::{Deserialize, Serialize};

/// The ledger file a wave accumulates beside its plan.
pub(crate) const LEDGER_FILE: &str = "run-ledger.jsonl";

/// One appended ledger record. The ledger is the run's memory and its
/// evidence: which nodes completed, when, and on whose confirmation.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct LedgerEntry {
    pub node: String,
    pub event: LedgerEvent,
    /// Wall-clock seconds since the Unix epoch.
    pub at_unix_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LedgerEvent {
    Started,
    Confirmed,
    Completed,
    Failed,
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Appends one entry; every event is durable before the walk continues.
pub(crate) fn append_ledger(directory: &Path, node: &str, event: LedgerEvent) -> Result<()> {
    let entry = LedgerEntry {
        node: node.to_string(),
        event,
        at_unix_seconds: now_unix_seconds(),
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
pub(crate) fn completed_nodes(directory: &Path) -> Result<BTreeSet<String>> {
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

/// A station program, loaded and validated up front so nothing is
/// discovered mid-walk.
pub(crate) enum LoadedProgram {
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
pub(crate) struct LoadedNode {
    pub id: String,
    pub action: LoadedAction,
}

pub(crate) enum LoadedAction {
    Program(LoadedProgram),
    Handoff {
        from: String,
        to: String,
        instructions: String,
    },
    Manual {
        title: String,
        instructions: String,
    },
}

pub(crate) struct LoadedWorkcell {
    pub nodes: Vec<LoadedNode>,
    /// The thermocycler station's name, when the plan declares one.
    pub thermocycler_station: Option<String>,
}

/// True when the directory holds a workcell coordination plan.
pub(crate) fn is_workcell_directory(directory: &Path) -> bool {
    directory.join("plan.workcell.json").is_file()
}

/// Loads a wave directory: the coordination plan names every node, and
/// every referenced station document must parse and validate before
/// anything is reported ready.
pub(crate) fn load_workcell_directory(directory: &Path) -> Result<LoadedWorkcell> {
    let plan_path = directory.join("plan.workcell.json");
    let plan_text = fs::read_to_string(&plan_path)
        .with_context(|| format!("failed to read {}", plan_path.display()))?;
    let plan: WorkcellRunDocument =
        serde_json::from_str(&plan_text).context("failed to parse the coordination plan")?;
    if plan.format != WORKCELL_RUN_FORMAT {
        bail!(
            "{} declares format '{}'; this runner speaks {WORKCELL_RUN_FORMAT}",
            plan_path.display(),
            plan.format
        );
    }

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
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("missing station document {}", path.display()))?;
                match station_kind(station)? {
                    "hamilton.star" => {
                        let document: StarRunDocument = serde_json::from_str(&text)
                            .with_context(|| format!("failed to parse {}", path.display()))?;
                        if document.format != STAR_RUN_FORMAT {
                            bail!(
                                "{} declares format '{}'; station '{station}' runs {STAR_RUN_FORMAT} documents",
                                path.display(),
                                document.format
                            );
                        }
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
                        let document: ThermocycleRunDocument = serde_json::from_str(&text)
                            .with_context(|| format!("failed to parse {}", path.display()))?;
                        if document.format != THERMOCYCLE_RUN_FORMAT {
                            bail!(
                                "{} declares format '{}'; station '{station}' runs {THERMOCYCLE_RUN_FORMAT} documents",
                                path.display(),
                                document.format
                            );
                        }
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
                instructions: format!("{instructions} ({labware}: {from} -> {to})"),
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
pub(crate) fn render_dry_run(loaded: &LoadedWorkcell) -> String {
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
            LoadedAction::Handoff { instructions, .. } => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} — by hand: {instructions}",
                    index + 1,
                    node.id
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

/// The connected stations a walk accumulates: each opens on first use and
/// stays open for the wave.
struct Sessions {
    star: Option<hamilton_star::Star>,
    odtc: Option<lab_inheco_odtc::Odtc>,
}

/// Bench context the walk carries: which station is the cycler, and where
/// stations answer on this bench. Addresses are runtime input — compiled
/// artifacts never carry them.
struct Bench {
    thermocycler_station: Option<String>,
    addresses: std::collections::BTreeMap<String, String>,
}

/// The workcell `lab run` flow: validate everything, then walk.
pub(crate) fn run_workcell(
    directory: PathBuf,
    dry_run: bool,
    yes: bool,
    resume: bool,
    station_addresses: Vec<String>,
    output: &crate::Output,
) -> Result<()> {
    let loaded = load_workcell_directory(&directory)?;
    let mut addresses = std::collections::BTreeMap::new();
    for entry in &station_addresses {
        let Some((name, address)) = entry.split_once('=') else {
            bail!("--station takes NAME=ADDRESS, e.g. --station odtc-1=169.254.10.40:8080");
        };
        addresses.insert(name.to_string(), address.to_string());
    }

    if dry_run {
        let human = render_dry_run(&loaded);
        return output.success(
            "dry-run",
            serde_json::json!({ "nodes": loaded.nodes.len() }),
            human,
        );
    }

    let completed = if resume {
        completed_nodes(&directory)?
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

    let pending: Vec<&LoadedNode> = loaded
        .nodes
        .iter()
        .filter(|node| !completed.contains(&node.id))
        .collect();
    println!(
        "about to execute {} coordination node(s){}",
        pending.len(),
        if completed.is_empty() {
            String::new()
        } else {
            format!(", resuming past {} completed", completed.len())
        }
    );
    if !yes && !crate::run::confirm("proceed? Stations will move. [y/N] ")? {
        bail!("run cancelled before any motion");
    }

    let bench = Bench {
        thermocycler_station: loaded.thermocycler_station.clone(),
        addresses,
    };
    let mut sessions = Sessions {
        star: None,
        odtc: None,
    };
    let mut executed = 0usize;
    for node in &loaded.nodes {
        if completed.contains(&node.id) {
            println!("skipping {} (completed in the ledger)", node.id);
            continue;
        }
        append_ledger(&directory, &node.id, LedgerEvent::Started)?;
        let outcome = execute_node(node, &mut sessions, &bench);
        match outcome {
            Ok(()) => {
                append_ledger(&directory, &node.id, LedgerEvent::Completed)?;
                executed += 1;
            }
            Err(error) => {
                append_ledger(&directory, &node.id, LedgerEvent::Failed)?;
                bail!(
                    "node '{}' failed: {error}; the ledger holds every completed node — resolve the bench and continue with --resume",
                    node.id
                );
            }
        }
    }
    output.success(
        "run",
        serde_json::json!({ "nodes": executed, "skipped": completed.len() }),
        format!(
            "completed {executed} coordination node(s){}",
            if completed.is_empty() {
                String::new()
            } else {
                format!(" ({} skipped as already complete)", completed.len())
            }
        ),
    )
}

fn ensure_star(sessions: &mut Sessions) -> Result<&hamilton_star::Star> {
    if sessions.star.is_none() {
        println!("connecting to the first Hamilton STAR on USB");
        let star = hamilton_star::Star::open_usb().context(
            "no Hamilton STAR answered on USB; use --dry-run to review without hardware",
        )?;
        println!("connected; running the setup choreography");
        star.initialize(hamilton_star::InitializeOptions::default())
            .context("the setup choreography failed; the machine is not in a known state")?;
        sessions.star = Some(star);
    }
    Ok(sessions.star.as_ref().expect("just ensured"))
}

fn ensure_odtc<'sessions>(
    sessions: &'sessions mut Sessions,
    bench: &Bench,
    station: &str,
) -> Result<&'sessions mut lab_inheco_odtc::Odtc> {
    if sessions.odtc.is_none() {
        let address = bench.addresses.get(station).with_context(|| {
            format!(
                "station '{station}' has no address on this bench; pass --station {station}=<ip:port> (the ODTC answers on port 8080)"
            )
        })?;
        let socket: std::net::SocketAddr = address.parse().with_context(|| {
            format!("'{address}' is not an <ip:port> address for station '{station}'")
        })?;
        println!("connecting to {station} at {socket}");
        let transport = lab_inheco_odtc::HttpSoapTransport::connect(socket)
            .with_context(|| format!("station '{station}' did not answer at {socket}"))?;
        let session = lab_inheco_odtc::Odtc::connect(
            std::sync::Arc::new(transport),
            lab_inheco_odtc::OdtcOptions::default(),
        )
        .with_context(|| format!("the {station} connection handshake failed"))?;
        println!("connected; {station} is idle");
        sessions.odtc = Some(session);
    }
    Ok(sessions.odtc.as_mut().expect("just ensured"))
}

/// True when a handoff endpoint is the cycler, whose motorized door the
/// runner must open before the operator can reach the block.
fn involves_cycler(bench: &Bench, station: &str) -> bool {
    bench
        .thermocycler_station
        .as_deref()
        .is_some_and(|cycler| cycler == station)
}

fn execute_node(node: &LoadedNode, sessions: &mut Sessions, bench: &Bench) -> Result<()> {
    use lab_instruments::Thermocycler as _;
    match &node.action {
        LoadedAction::Handoff {
            from,
            to,
            instructions,
        } => {
            let to_cycler = involves_cycler(bench, to);
            let from_cycler = involves_cycler(bench, from);
            if to_cycler || from_cycler {
                let station = if to_cycler { to } else { from };
                let odtc = ensure_odtc(sessions, bench, station)?;
                odtc.open_lid()
                    .with_context(|| format!("could not open the {station} door"))?;
                println!("{station} door is open");
            }
            println!("\nby hand — {instructions}");
            if !crate::run::confirm("done, and the bench matches the plan? Continue [y/N] ")? {
                bail!("the operator declined the handoff");
            }
            if to_cycler || from_cycler {
                let station = if to_cycler { to } else { from };
                let odtc = ensure_odtc(sessions, bench, station)?;
                if from_cycler {
                    // The plate is out; nothing holds temperature for it now.
                    odtc.stop()
                        .with_context(|| format!("could not stop {station} after retrieval"))?;
                }
                odtc.close_lid()
                    .with_context(|| format!("could not close the {station} door"))?;
                println!("{station} door is closed");
            }
            Ok(())
        }
        LoadedAction::Manual {
            title,
            instructions,
        } => {
            println!("\nby hand — {title}: {instructions}");
            if !crate::run::confirm("done, and the bench matches the plan? Continue [y/N] ")? {
                bail!("the operator declined the manual step");
            }
            Ok(())
        }
        LoadedAction::Program(LoadedProgram::Star {
            station,
            document,
            steps,
        }) => {
            ensure_star(sessions)?;
            let star = sessions.star.as_ref().expect("just ensured");
            println!("\n{station}: {} ({} frames)", document.title, steps.len());
            for (index, (command, description)) in steps.iter().enumerate() {
                println!("  [{:>3}] {description}", index + 1);
                if let Err(error) = star.execute_raw(command) {
                    let retract = RawCommand::parse("C0ZA")
                        .expect("the retract frame is a constant well-formed frame");
                    let _ = star.execute_raw(&retract);
                    bail!(
                        "firmware error at frame {}: {error}; channels were retracted to Z-safety",
                        index + 1
                    );
                }
            }
            Ok(())
        }
        LoadedAction::Program(LoadedProgram::Thermocycle { station, document }) => {
            let odtc = ensure_odtc(sessions, bench, station)?;
            document
                .profile
                .validate(&lab_inheco_odtc::odtc_limits())
                .with_context(|| format!("'{}' is outside the {station} envelope", document.id))?;
            println!(
                "\n{station}: {} ({} plateaus)",
                document.title,
                document.profile.total_steps()
            );
            let handle = odtc
                .run_profile(&document.profile)
                .with_context(|| format!("could not start '{}' on {station}", document.id))?;
            println!(
                "running; completion may take hours — the wave resumes with --resume if interrupted"
            );
            odtc.await_completion(handle)
                .with_context(|| format!("'{}' did not complete on {station}", document.id))?;
            for warning in odtc.take_warnings() {
                println!("{station} warning: {warning}");
            }
            if let Some(celsius) = document.final_hold_celsius {
                println!("holding the block at {celsius} °C until retrieval");
                odtc.hold_block(celsius, None)
                    .with_context(|| format!("could not hold {celsius} °C on {station}"))?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ledger_round_trips_and_reports_completed_nodes() {
        let directory = std::env::temp_dir().join(format!(
            "lab-ledger-test-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&directory).unwrap();
        append_ledger(&directory, "assembly_run", LedgerEvent::Started).unwrap();
        append_ledger(&directory, "assembly_run", LedgerEvent::Completed).unwrap();
        append_ledger(&directory, "assembly_thermocycle", LedgerEvent::Started).unwrap();
        let completed = completed_nodes(&directory).unwrap();
        assert!(
            completed.contains("assembly_run"),
            "a completed node is remembered"
        );
        assert!(
            !completed.contains("assembly_thermocycle"),
            "a started-but-unfinished node is not skipped on resume"
        );
        fs::remove_dir_all(&directory).unwrap();
    }
}
