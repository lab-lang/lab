//! Terminal presentation and live-executor construction for reviewed facility plans.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use lab_adapters::AdapterRuntimeRegistryExt;
use lab_runtime::clock::WallClock;
use lab_runtime::events::{EventSink, ProgramExtent, RunEvent};
use lab_runtime::execution::{
    ExecutionOutcome, ExecutionRunConfig, load_execution_directory, render_execution_dry_run,
    run_execution_plan,
};
use lab_runtime::mode::ExecutionMode;
use lab_runtime::operator::StdinOperator;
use lab_runtime::provenance::{inventory_result_file, write_inventory_result};

use crate::Output;

pub(crate) fn run_execution_command(
    directory: PathBuf,
    dry_run: bool,
    simulate: bool,
    yes: bool,
    resume: bool,
    asset_endpoints: Vec<String>,
    output: &Output,
) -> Result<()> {
    let adapters = lab_project::application_extensions()
        .context("failed to compose the adapter registry")?
        .adapters;
    let document_loaders = adapters.reviewed_document_loaders()?;
    let loaded = load_execution_directory(&directory, &document_loaders)?;
    if dry_run {
        return output.success(
            "dry-run",
            serde_json::json!({
                "format": loaded.plan.format,
                "plan_sha256": loaded.plan_sha256,
                "facility": loaded.plan.inventory.facility,
                "nodes": loaded.nodes.len(),
                "simulatable": loaded.is_ready(ExecutionMode::Simulation),
                "simulation_readiness_issues": loaded.readiness_issues(ExecutionMode::Simulation),
                "executable": loaded.is_ready(ExecutionMode::Live),
                "execution_readiness_issues": loaded.readiness_issues(ExecutionMode::Live),
            }),
            render_execution_dry_run(&loaded),
        );
    }

    let mode = if simulate {
        if !asset_endpoints.is_empty() {
            bail!("--asset-endpoint is only meaningful for live execution");
        }
        ExecutionMode::Simulation
    } else {
        ExecutionMode::Live
    };
    let mut registry = match mode {
        ExecutionMode::Simulation => adapters.simulation_executors(&loaded)?,
        ExecutionMode::Live => {
            let addresses = parse_asset_endpoints(&asset_endpoints)?;
            adapters.live_executors(&loaded, &addresses)?
        }
    };
    let mut operator = StdinOperator;
    let mut events = HumanSink;
    match run_execution_plan(
        &loaded,
        ExecutionRunConfig {
            assume_yes: yes,
            resume,
            mode,
        },
        &mut registry,
        &mut operator,
        &mut events,
        &WallClock,
    )? {
        ExecutionOutcome::Completed {
            executed,
            skipped,
            started_at_unix_seconds,
            ended_at_unix_seconds,
        } => {
            let existing = loaded.directory.join(inventory_result_file(mode));
            let result = if executed == 0 && existing.is_file() {
                None
            } else {
                Some(write_inventory_result(
                    &loaded,
                    mode,
                    started_at_unix_seconds,
                    ended_at_unix_seconds,
                )?)
            };
            let result_path = result
                .as_ref()
                .map_or(existing.as_path(), |result| result.path.as_path());
            output.success(
                mode.as_str(),
                serde_json::json!({
                    "mode": mode.as_str(),
                    "plan_sha256": loaded.plan_sha256,
                    "executed": executed,
                    "skipped": skipped,
                    "inventory_result": result_path,
                    "activity": result.as_ref().map(|result| result.activity.as_str()),
                    "output_materials": result.as_ref().map(|result| &result.output_materials),
                }),
                format!(
                    "Completed reviewed facility {}: {executed} node(s) executed, {skipped} skipped\n  Inventory result: {}",
                    mode.as_str(),
                    result_path.display()
                ),
            )
        }
        ExecutionOutcome::Cancelled => bail!("run cancelled before any motion"),
        ExecutionOutcome::Declined { node } => bail!(
            "node '{node}' stopped because the operator declined; resolve the facility and continue the same reviewed plan with --resume"
        ),
        ExecutionOutcome::Failed { node, error } => bail!(
            "node '{node}' failed: {error}; resolve the facility and continue the same reviewed plan with --resume"
        ),
    }
}

fn parse_asset_endpoints(entries: &[String]) -> Result<BTreeMap<String, SocketAddr>> {
    let mut addresses = BTreeMap::new();
    for entry in entries {
        let Some((asset, address)) = entry.split_once('=') else {
            bail!("--asset-endpoint takes ASSET_IRI=ADDRESS for a facility execution plan");
        };
        let address = address.parse().with_context(|| {
            format!("'{address}' is not an <ip:port> address for Asset '{asset}'")
        })?;
        if addresses.insert(asset.to_owned(), address).is_some() {
            bail!("Asset '{asset}' has more than one --asset-endpoint address");
        }
    }
    Ok(addresses)
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
            RunEvent::Connecting { asset, detail } => {
                println!("connecting to Asset {asset} ({detail})")
            }
            RunEvent::Connected { asset } => println!("connected; Asset {asset} is ready"),
            RunEvent::NodeSkipped { id } => println!("skipping {id} (completed in the ledger)"),
            RunEvent::NodeStarted { .. } | RunEvent::NodeCompleted { .. } => {}
            RunEvent::DocumentStarted {
                asset,
                driver,
                format,
                title,
            } => println!("\n{title}\n  Asset: {asset}\n  Adapter: {driver}\n  Document: {format}"),
            RunEvent::ProgramStarted {
                asset,
                title,
                extent,
            } => match extent {
                ProgramExtent::Frames { frames } => {
                    println!("\n{asset}: {title} ({frames} frames)")
                }
                ProgramExtent::Plateaus { plateaus, .. } => {
                    println!("\n{asset}: {title} ({plateaus} plateaus)")
                }
            },
            RunEvent::Frame {
                index, description, ..
            } => println!("  [{index:>3}] {description}"),
            RunEvent::ThermalRunning { .. } => {
                println!("running; resume uses the exact reviewed plan if interrupted")
            }
            RunEvent::ThermalWarning { asset, warning } => {
                println!("{asset} warning: {warning}")
            }
            RunEvent::ThermalHold { celsius, .. } => {
                println!("holding the block at {celsius} C until retrieval")
            }
            RunEvent::DoorOpened { asset } => println!("{asset} door is open"),
            RunEvent::DoorClosed { asset } => println!("{asset} door is closed"),
            RunEvent::AttentionRequired { prompt, .. } => println!("\nby hand: {prompt}"),
            RunEvent::AttentionReleased { .. } | RunEvent::LabwareMoved { .. } => {}
        }
    }
}
