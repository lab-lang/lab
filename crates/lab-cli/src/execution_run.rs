//! Terminal presentation and live-executor construction for reviewed facility plans.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::hamilton::star::StarTargetProfile;
use lab_runfmt::{EXECUTION_PLAN_FILE, STAR_RUN_FORMAT, THERMOCYCLE_RUN_FORMAT};
use lab_runtime::clock::WallClock;
use lab_runtime::device_executors::{HamiltonStarExecutor, OdtcExecutor};
use lab_runtime::events::{EventSink, ProgramExtent, RunEvent};
use lab_runtime::execution::{
    ExecutionOutcome, ExecutionRunConfig, ExecutorRegistry, LoadedExecutionAction,
    load_execution_directory, render_execution_dry_run, run_execution_plan,
};
use lab_runtime::operator::StdinOperator;

use crate::Output;

pub(crate) fn is_execution_directory(directory: &Path) -> bool {
    directory.join(EXECUTION_PLAN_FILE).is_file()
}

pub(crate) fn run_execution_command(
    directory: PathBuf,
    dry_run: bool,
    yes: bool,
    resume: bool,
    asset_addresses: Vec<String>,
    output: &Output,
) -> Result<()> {
    let loaded = load_execution_directory(&directory)?;
    if dry_run {
        return output.success(
            "dry-run",
            serde_json::json!({
                "format": loaded.plan.format,
                "plan_sha256": loaded.plan_sha256,
                "facility": loaded.plan.inventory.facility,
                "nodes": loaded.nodes.len(),
                "executable": loaded.is_executable(),
                "readiness_issues": loaded.readiness_issues(),
            }),
            render_execution_dry_run(&loaded),
        );
    }

    let addresses = parse_asset_addresses(&asset_addresses)?;
    let mut registry = build_hardware_registry(&loaded, &addresses)?;
    let mut operator = StdinOperator;
    let mut events = HumanSink;
    match run_execution_plan(
        &loaded,
        ExecutionRunConfig {
            assume_yes: yes,
            resume,
        },
        &mut registry,
        &mut operator,
        &mut events,
        &WallClock,
    )? {
        ExecutionOutcome::Completed { executed, skipped } => output.success(
            "run",
            serde_json::json!({
                "plan_sha256": loaded.plan_sha256,
                "executed": executed,
                "skipped": skipped,
            }),
            format!(
                "Completed reviewed facility plan: {executed} node(s) executed, {skipped} skipped"
            ),
        ),
        ExecutionOutcome::Cancelled => bail!("run cancelled before any motion"),
        ExecutionOutcome::Declined { node } => bail!(
            "node '{node}' stopped because the operator declined; resolve the facility and continue the same reviewed plan with --resume"
        ),
        ExecutionOutcome::Failed { node, error } => bail!(
            "node '{node}' failed: {error}; resolve the facility and continue the same reviewed plan with --resume"
        ),
    }
}

fn parse_asset_addresses(entries: &[String]) -> Result<BTreeMap<String, SocketAddr>> {
    let mut addresses = BTreeMap::new();
    for entry in entries {
        let Some((asset, address)) = entry.split_once('=') else {
            bail!("--station takes ASSET_IRI=ADDRESS for a facility execution plan");
        };
        let address = address.parse().with_context(|| {
            format!("'{address}' is not an <ip:port> address for Asset '{asset}'")
        })?;
        if addresses.insert(asset.to_owned(), address).is_some() {
            bail!("Asset '{asset}' has more than one --station address");
        }
    }
    Ok(addresses)
}

fn build_hardware_registry(
    loaded: &lab_runtime::execution::LoadedExecutionPlan,
    addresses: &BTreeMap<String, SocketAddr>,
) -> Result<ExecutorRegistry> {
    let mut bindings = BTreeMap::<(String, String, String), (String, String)>::new();
    for node in &loaded.nodes {
        let LoadedExecutionAction::Execute {
            requirement,
            document: Some(document),
        } = &node.action
        else {
            continue;
        };
        let Some(adapter) = &requirement.adapter else {
            continue;
        };
        let key = (
            requirement.asset.clone(),
            adapter.driver.clone(),
            document.format().to_owned(),
        );
        let profile = (adapter.profile_path.clone(), adapter.profile_sha256.clone());
        if let Some(prior) = bindings.insert(key.clone(), profile.clone())
            && prior != profile
        {
            bail!(
                "asset '{}' uses adapter '{}' and format '{}' with two different frozen profiles",
                key.0,
                key.1,
                key.2
            );
        }
    }

    let star_assets = bindings
        .keys()
        .filter(|(_, driver, _)| driver == "hamilton.star")
        .map(|(asset, _, _)| asset)
        .collect::<BTreeSet<_>>();
    if star_assets.len() > 1 {
        bail!(
            "this runtime can address only one Hamilton STAR over USB, but the reviewed plan binds {}",
            star_assets
                .iter()
                .copied()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let mut used_addresses = BTreeSet::new();
    let mut registry = ExecutorRegistry::new();
    for ((asset, driver, format), (profile_path, _profile_sha256)) in bindings {
        match (driver.as_str(), format.as_str()) {
            ("hamilton.star", STAR_RUN_FORMAT) => {
                let path = loaded.directory.join(&profile_path);
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let name = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .context("a STAR adapter profile needs a UTF-8 file name")?;
                let profile = StarTargetProfile::parse(name, &text).with_context(|| {
                    format!("failed to parse frozen profile {}", path.display())
                })?;
                registry.register(
                    &asset,
                    &driver,
                    &format,
                    Box::new(HamiltonStarExecutor::new(
                        &asset,
                        profile.run.autoload_park_track,
                    )),
                )?;
            }
            ("inheco.odtc", THERMOCYCLE_RUN_FORMAT) => {
                let address = addresses.get(&asset).with_context(|| {
                    format!(
                        "Inheco ODTC Asset '{asset}' has no runtime address; pass --station '{asset}=<ip:port>'"
                    )
                })?;
                used_addresses.insert(asset.clone());
                registry.register(
                    &asset,
                    &driver,
                    &format,
                    Box::new(OdtcExecutor::new(&asset, *address)),
                )?;
            }
            _ => bail!(
                "this Lab runtime has no live executor for asset '{asset}', adapter '{driver}', format '{format}'"
            ),
        }
    }
    if let Some(unused) = addresses
        .keys()
        .find(|asset| !used_addresses.contains(*asset))
    {
        bail!(
            "--station supplies an address for Asset '{unused}', which the reviewed plan does not use as a networked executor"
        );
    }
    Ok(registry)
}

struct HumanSink;

impl EventSink for HumanSink {
    fn emit(&mut self, event: RunEvent) {
        match event {
            RunEvent::Planned { pending, completed } => println!(
                "about to execute {pending} facility node(s){}",
                if completed == 0 {
                    String::new()
                } else {
                    format!(", resuming past {completed} completed")
                }
            ),
            RunEvent::Connecting { station, detail } => {
                println!("connecting to Asset {station} ({detail})")
            }
            RunEvent::Connected { station } => println!("connected; Asset {station} is ready"),
            RunEvent::NodeSkipped { id } => println!("skipping {id} (completed in the ledger)"),
            RunEvent::NodeStarted { .. } | RunEvent::NodeCompleted { .. } => {}
            RunEvent::DocumentStarted {
                asset,
                driver,
                format,
                title,
            } => println!("\n{title}\n  Asset: {asset}\n  Adapter: {driver}\n  Document: {format}"),
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
            RunEvent::ThermalRunning { .. } => {
                println!("running; resume uses the exact reviewed plan if interrupted")
            }
            RunEvent::ThermalWarning { station, warning } => {
                println!("{station} warning: {warning}")
            }
            RunEvent::ThermalHold { celsius, .. } => {
                println!("holding the block at {celsius} C until retrieval")
            }
            RunEvent::DoorOpened { station } => println!("{station} door is open"),
            RunEvent::DoorClosed { station } => println!("{station} door is closed"),
            RunEvent::AttentionRequired { prompt, .. } => println!("\nby hand: {prompt}"),
            RunEvent::AttentionReleased { .. } | RunEvent::LabwareMoved { .. } => {}
        }
    }
}
