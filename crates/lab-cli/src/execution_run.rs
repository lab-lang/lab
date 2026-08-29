//! Terminal presentation and live-executor construction for reviewed facility plans.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use lab_compiler::backend::{adapter_catalog, hamilton::star::StarAdapterProfile};
use lab_runfmt::{STAR_RUN_FORMAT, THERMOCYCLE_RUN_FORMAT};
use lab_runtime::clock::WallClock;
use lab_runtime::device_executors::{
    HamiltonStarExecutor, OdtcExecutor, ReviewedDocumentSimulationExecutor,
};
use lab_runtime::events::{EventSink, ProgramExtent, RunEvent};
use lab_runtime::execution::{
    ExecutionOutcome, ExecutionRunConfig, ExecutorRegistry, LoadedExecutionAction,
    load_execution_directory, render_execution_dry_run, run_execution_plan,
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
    let loaded = load_execution_directory(&directory)?;
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
        ExecutionMode::Simulation => build_simulation_registry(&loaded)?,
        ExecutionMode::Live => {
            let addresses = parse_asset_endpoints(&asset_endpoints)?;
            build_hardware_registry(&loaded, &addresses)?
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

fn build_simulation_registry(
    loaded: &lab_runtime::execution::LoadedExecutionPlan,
) -> Result<ExecutorRegistry> {
    let catalog = adapter_catalog().context("failed to load the compiler adapter catalog")?;
    let descriptors = catalog
        .adapters
        .iter()
        .map(|descriptor| (descriptor.id.as_str(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let mut keys = BTreeSet::new();
    let mut registry = ExecutorRegistry::new();
    for node in &loaded.nodes {
        let LoadedExecutionAction::Execute {
            requirement,
            document: Some(document),
        } = &node.action
        else {
            continue;
        };
        let adapter = requirement
            .adapter
            .as_ref()
            .context("simulation requires a frozen adapter binding")?;
        let descriptor = descriptors.get(adapter.driver.as_str()).with_context(|| {
            format!(
                "adapter '{}' is not present in this compiler build",
                adapter.driver
            )
        })?;
        if !descriptor.services.simulation {
            bail!("adapter '{}' does not provide simulation", adapter.driver);
        }
        if !descriptor
            .capabilities
            .iter()
            .any(|kind| kind.as_str() == requirement.capability_kind)
        {
            bail!(
                "adapter '{}' does not simulate capability '{}'",
                adapter.driver,
                requirement.capability_kind
            );
        }
        if !descriptor
            .control_modes
            .iter()
            .any(|mode| mode.iri() == requirement.control_mode)
        {
            bail!(
                "adapter '{}' does not accept control mode '{}'",
                adapter.driver,
                requirement.control_mode
            );
        }
        if !descriptor.accepted_run_formats.contains(document.format()) {
            bail!(
                "adapter '{}' does not simulate reviewed format '{}'",
                adapter.driver,
                document.format()
            );
        }
        let key = (
            requirement.asset.clone(),
            adapter.driver.clone(),
            document.format().to_owned(),
        );
        if keys.insert(key.clone()) {
            registry.register(
                key.0,
                key.1,
                key.2,
                Box::<ReviewedDocumentSimulationExecutor>::default(),
            )?;
        }
    }
    Ok(registry)
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
                let profile = StarAdapterProfile::parse(name, &text).with_context(|| {
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
                        "Inheco ODTC Asset '{asset}' has no runtime address; pass --asset-endpoint '{asset}=<ip:port>'"
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
            "--asset-endpoint supplies an address for Asset '{unused}', which the reviewed plan does not use as a networked executor"
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
