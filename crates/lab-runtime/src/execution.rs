//! Eager preflight for facility-wide reviewed execution plans.
//!
//! Loading is deliberately more than JSON parsing. It validates the exact inventory graph,
//! checks every frozen profile and child-document digest, projects every catalog binding back
//! onto the selected facility, validates every device document, and computes a deterministic
//! topological walk. A live runner receives only a [`LoadedExecutionPlan`], so it cannot discover
//! a bad document after an instrument has already moved.

use std::any::{Any, type_name};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_capability::{
    AbsoluteIri, ExactDecimal, ExactInteger, PropertyConstraint, PropertyKind, PropertyValue,
    ScalarValue, UnitIri,
};
use lab_inventory::{FacilityScalarValue, InventorySnapshot};
use lab_runfmt::{
    EXECUTION_PLAN_FILE, EXECUTION_PLAN_FORMAT, ExecutionParameterValue, ExecutionPlanAction,
    ExecutionPlanDocument, ExecutionPlanNode, ExecutionRequirementBinding,
};
use sbol3::{DisplayId, Iri, Namespace, Resource};
use sha2::{Digest, Sha256};

use crate::clock::Clock;
use crate::events::{EventSink, RunEvent};
use crate::ledger::{ExecutionLedger, LEDGER_FILE, LedgerEvent};
use crate::mode::ExecutionMode;
use crate::operator::{ConfirmKind, Operator};

/// One facility-wide plan after every frozen input and catalog binding has passed preflight.
#[derive(Debug)]
pub struct LoadedExecutionPlan {
    pub directory: PathBuf,
    pub plan: ExecutionPlanDocument,
    /// SHA-256 of the exact reviewed `plan.execution.json` bytes.
    pub plan_sha256: String,
    pub inventory: InventorySnapshot,
    /// Nodes in deterministic topological order, independent of their serialized order.
    pub nodes: Vec<LoadedExecutionNode>,
}

impl LoadedExecutionPlan {
    /// Reasons this valid reviewed plan cannot run in the requested mode.
    /// Planning-only plans remain useful and can still be rendered as dry runs.
    pub fn readiness_issues(&self, mode: ExecutionMode) -> Vec<String> {
        let mut issues = Vec::new();
        let minimum = match mode {
            ExecutionMode::Simulation => sbol_inventory::vocabulary::Qualification::Simulatable,
            ExecutionMode::Live => sbol_inventory::vocabulary::Qualification::Executable,
        };
        if mode == ExecutionMode::Simulation && !self.plan.outputs.is_empty() {
            issues.push(
                "simulation plans cannot mint physical output MaterialLots; remove plan outputs or execute the reviewed plan live"
                    .to_owned(),
            );
        }
        for node in &self.nodes {
            match &node.action {
                LoadedExecutionAction::Execute {
                    requirements,
                    document,
                } => {
                    for requirement in requirements {
                        check_execution_qualification(
                            &mut issues,
                            &node.id,
                            requirement,
                            minimum,
                            mode,
                        );
                    }
                    let requirement = requirements
                        .first()
                        .expect("execution-plan validation requires a non-empty binding set");
                    if requirement.adapter.is_none() {
                        issues.push(format!("node '{}' has no frozen runtime adapter", node.id));
                    }
                    if document.is_none() {
                        issues.push(format!("node '{}' has no reviewed run document", node.id));
                    }
                }
                LoadedExecutionAction::Manual { requirements, .. } => {
                    for requirement in requirements {
                        check_execution_qualification(
                            &mut issues,
                            &node.id,
                            requirement,
                            minimum,
                            mode,
                        );
                    }
                }
                LoadedExecutionAction::MoveMaterial { .. } => {}
            }
        }
        // Several requirements on one node routinely share an offering, and repeating one identical
        // sentence per requirement buries the distinct problems among the copies.
        let mut seen = std::collections::BTreeSet::new();
        issues.retain(|issue| seen.insert(issue.clone()));
        issues
    }

    pub fn is_ready(&self, mode: ExecutionMode) -> bool {
        self.readiness_issues(mode).is_empty()
    }
}

fn check_execution_qualification(
    issues: &mut Vec<String>,
    node: &str,
    requirement: &ExecutionRequirementBinding,
    minimum: sbol_inventory::vocabulary::Qualification,
    mode: ExecutionMode,
) {
    let qualification = sbol_inventory::vocabulary::Qualification::try_from(
        requirement.observed_qualification.as_str(),
    );
    if !qualification.is_ok_and(|value| value >= minimum) {
        issues.push(format!(
            "node '{}' binds offering '{}' at qualification '{}', below '{}' for {}",
            node,
            requirement.offering,
            requirement.observed_qualification,
            minimum.iri(),
            mode.as_str()
        ));
    }
}

#[derive(Debug)]
pub struct LoadedExecutionNode {
    pub id: String,
    pub after: Vec<String>,
    pub action: LoadedExecutionAction,
}

#[derive(Debug)]
pub enum LoadedExecutionAction {
    Execute {
        requirements: Vec<ExecutionRequirementBinding>,
        document: Option<LoadedReviewedDocument>,
    },
    MoveMaterial {
        material: String,
        from: String,
        to: String,
        instructions: String,
    },
    Manual {
        requirements: Vec<ExecutionRequirementBinding>,
        title: String,
        instructions: String,
    },
}

/// One eagerly validated reviewed document with an adapter-defined typed payload.
///
/// The runtime core owns only the stable format and presentation metadata. A loader registered by
/// the application owns parsing and semantic validation, and stores whatever payload its exact
/// executor needs. That keeps new document formats out of a central runtime enum.
pub struct LoadedReviewedDocument {
    format: String,
    title: String,
    payload: Box<dyn Any + Send + Sync>,
}

impl LoadedReviewedDocument {
    pub fn new<T>(format: impl Into<String>, title: impl Into<String>, payload: T) -> Result<Self>
    where
        T: Any + Send + Sync,
    {
        let format = format.into();
        let title = title.into();
        if format.is_empty() || title.is_empty() {
            bail!("a loaded reviewed document requires a non-empty format and title");
        }
        Ok(Self {
            format,
            title,
            payload: Box::new(payload),
        })
    }

    pub fn format(&self) -> &str {
        &self.format
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Borrows the adapter-defined payload when it has the requested exact Rust type.
    pub fn payload<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }

    /// Borrows the adapter-defined payload or reports a precise executor/loader mismatch.
    pub fn require_payload<T: Any>(&self) -> Result<&T> {
        self.payload::<T>().with_context(|| {
            format!(
                "reviewed document '{}' does not carry payload type '{}'",
                self.format,
                type_name::<T>()
            )
        })
    }
}

impl fmt::Debug for LoadedReviewedDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedReviewedDocument")
            .field("format", &self.format)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

/// Immutable context passed to one exact reviewed-document loader.
#[derive(Clone, Copy, Debug)]
pub struct ReviewedDocumentLoadRequest<'a> {
    pub adapter_id: &'a str,
    pub format: &'a str,
    pub expected_capability_kind: &'a str,
    pub bytes: &'a [u8],
    pub path: &'a Path,
}

type ReviewedDocumentLoader =
    dyn for<'a> Fn(ReviewedDocumentLoadRequest<'a>) -> Result<LoadedReviewedDocument> + Send + Sync;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReviewedDocumentLoaderKey {
    adapter_id: String,
    format: String,
}

/// Exact reviewed-document loaders linked into one runtime application.
///
/// There is no fallback by capability, manufacturer, filename, or similar format. The reviewed
/// plan must name an adapter ID and format explicitly registered by the composition root.
#[derive(Default)]
pub struct ReviewedDocumentLoaderRegistry {
    loaders: BTreeMap<ReviewedDocumentLoaderKey, Box<ReviewedDocumentLoader>>,
}

impl ReviewedDocumentLoaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(
        &mut self,
        adapter_id: impl Into<String>,
        format: impl Into<String>,
        loader: F,
    ) -> Result<()>
    where
        F: for<'a> Fn(ReviewedDocumentLoadRequest<'a>) -> Result<LoadedReviewedDocument>
            + Send
            + Sync
            + 'static,
    {
        let key = ReviewedDocumentLoaderKey {
            adapter_id: adapter_id.into(),
            format: format.into(),
        };
        if key.adapter_id.is_empty() || key.format.is_empty() {
            bail!("a document loader key requires a non-empty adapter ID and format");
        }
        if self.loaders.insert(key.clone(), Box::new(loader)).is_some() {
            bail!(
                "a reviewed-document loader is already registered for adapter '{}' and format '{}'",
                key.adapter_id,
                key.format
            );
        }
        Ok(())
    }

    pub fn load(
        &self,
        adapter_id: &str,
        format: &str,
        expected_capability_kind: &str,
        bytes: &[u8],
        path: &Path,
    ) -> Result<LoadedReviewedDocument> {
        let key = ReviewedDocumentLoaderKey {
            adapter_id: adapter_id.to_owned(),
            format: format.to_owned(),
        };
        let loader = self.loaders.get(&key).with_context(|| {
            format!("adapter '{adapter_id}' has no reviewed-document loader for format '{format}'")
        })?;
        let document = loader(ReviewedDocumentLoadRequest {
            adapter_id,
            format,
            expected_capability_kind,
            bytes,
            path,
        })?;
        if document.format() != format {
            bail!(
                "the loader registered for adapter '{adapter_id}' and format '{format}' returned format '{}'",
                document.format()
            );
        }
        Ok(document)
    }
}

/// An implementation of one exact reviewed-document binding.
pub trait DocumentExecutor {
    fn execute(
        &mut self,
        document: &LoadedReviewedDocument,
        events: &mut dyn EventSink,
    ) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExecutorKey {
    asset: String,
    driver: String,
    format: String,
}

/// Runtime executors keyed by the frozen Asset IRI, adapter ID, and document format.
/// There is no lookup by manufacturer, model, capability kind, or nearest match.
#[derive(Default)]
pub struct ExecutorRegistry {
    executors: BTreeMap<ExecutorKey, Box<dyn DocumentExecutor>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        asset: impl Into<String>,
        driver: impl Into<String>,
        format: impl Into<String>,
        executor: Box<dyn DocumentExecutor>,
    ) -> Result<()> {
        let key = ExecutorKey {
            asset: asset.into(),
            driver: driver.into(),
            format: format.into(),
        };
        if key.asset.is_empty() || key.driver.is_empty() || key.format.is_empty() {
            bail!("an executor key requires a non-empty Asset IRI, adapter ID, and format");
        }
        if self.executors.insert(key.clone(), executor).is_some() {
            bail!(
                "an executor is already registered for asset '{}', adapter '{}', format '{}'",
                key.asset,
                key.driver,
                key.format
            );
        }
        Ok(())
    }

    fn contains(&self, asset: &str, driver: &str, format: &str) -> bool {
        self.executors.contains_key(&ExecutorKey {
            asset: asset.to_owned(),
            driver: driver.to_owned(),
            format: format.to_owned(),
        })
    }

    fn executor_mut(
        &mut self,
        asset: &str,
        driver: &str,
        format: &str,
    ) -> Option<&mut (dyn DocumentExecutor + '_)> {
        let key = ExecutorKey {
            asset: asset.to_owned(),
            driver: driver.to_owned(),
            format: format.to_owned(),
        };
        match self.executors.get_mut(&key) {
            Some(executor) => Some(executor.as_mut()),
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionRunConfig {
    pub assume_yes: bool,
    pub resume: bool,
    pub mode: ExecutionMode,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Completed {
        executed: usize,
        skipped: usize,
        started_at_unix_seconds: u64,
        ended_at_unix_seconds: u64,
    },
    Cancelled,
    Declined {
        node: String,
    },
    Failed {
        node: String,
        error: String,
    },
}

/// Renders the fully preflighted facility walk without requiring runtime connectors.
pub fn render_execution_dry_run(loaded: &LoadedExecutionPlan) -> String {
    use std::fmt::Write as _;

    let issues = loaded.readiness_issues(ExecutionMode::Live);
    let mut text = String::new();
    let _ = writeln!(
        text,
        "dry run: {} facility node(s), all frozen inputs validated",
        loaded.nodes.len()
    );
    if !issues.is_empty() {
        let _ = writeln!(text, "planning-only bindings:");
        for issue in issues {
            let _ = writeln!(text, "  - {issue}");
        }
    }
    for (index, node) in loaded.nodes.iter().enumerate() {
        match &node.action {
            LoadedExecutionAction::Execute {
                requirements,
                document,
            } => {
                let requirement = requirements
                    .first()
                    .expect("execution-plan validation requires a non-empty binding set");
                let description = document.as_ref().map_or_else(
                    || "no reviewed run document".to_owned(),
                    |document| format!("{} ({})", document.title(), document.format()),
                );
                let mut seen = std::collections::BTreeSet::new();
                let capabilities = requirements
                    .iter()
                    .map(|binding| binding.capability_kind.as_str())
                    .filter(|kind| seen.insert(*kind))
                    .collect::<Vec<_>>()
                    .join(", ");
                let adapter = requirement
                    .adapter
                    .as_ref()
                    .map_or("no runtime adapter", |adapter| adapter.driver.as_str());
                let _ = writeln!(
                    text,
                    "\n[{}] {} - {} on {} through {}: {}",
                    index + 1,
                    node.id,
                    capabilities,
                    requirement.asset,
                    adapter,
                    description
                );
            }
            LoadedExecutionAction::MoveMaterial {
                material,
                from,
                to,
                instructions,
            } => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} - move {} from {} to {}: {}",
                    index + 1,
                    node.id,
                    material,
                    from,
                    to,
                    instructions
                );
            }
            LoadedExecutionAction::Manual {
                requirements,
                title,
                instructions,
            } => {
                let asset = &requirements[0].asset;
                let capabilities = requirements
                    .iter()
                    .map(|requirement| requirement.capability_kind.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(
                    text,
                    "\n[{}] {} - by hand on {} for {}: {}: {}",
                    index + 1,
                    node.id,
                    asset,
                    capabilities,
                    title,
                    instructions
                );
            }
        }
    }
    text
}

/// Executes a preflighted plan without ever re-querying the inventory or changing a binding.
#[allow(clippy::too_many_arguments)]
pub fn run_execution_plan(
    loaded: &LoadedExecutionPlan,
    config: ExecutionRunConfig,
    registry: &mut ExecutorRegistry,
    operator: &mut dyn Operator,
    events: &mut dyn EventSink,
    clock: &dyn Clock,
) -> Result<ExecutionOutcome> {
    let mut readiness = loaded.readiness_issues(config.mode);
    for node in &loaded.nodes {
        let LoadedExecutionAction::Execute {
            requirements,
            document: Some(document),
        } = &node.action
        else {
            continue;
        };
        let requirement = requirements
            .first()
            .expect("execution-plan validation requires a non-empty binding set");
        let Some(adapter) = &requirement.adapter else {
            continue;
        };
        if !registry.contains(&requirement.asset, &adapter.driver, document.format()) {
            readiness.push(format!(
                "node '{}' has no registered executor for asset '{}', adapter '{}', format '{}'",
                node.id,
                requirement.asset,
                adapter.driver,
                document.format()
            ));
        }
    }
    if !readiness.is_empty() {
        bail!(
            "reviewed plan is not ready for {}:\n  - {}",
            config.mode.as_str(),
            readiness.join("\n  - ")
        );
    }

    let valid_nodes = loaded
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let inventory_sha256 = loaded.inventory.source_sha256();
    let mut ledger = if config.resume {
        Some(ExecutionLedger::resume(
            &loaded.directory,
            &loaded.plan_sha256,
            inventory_sha256,
            valid_nodes.clone(),
            config.mode,
        )?)
    } else {
        let path = loaded.directory.join(LEDGER_FILE);
        if path.exists() {
            bail!(
                "{} already exists; resume the reviewed plan instead of replacing durable {} state",
                path.display(),
                config.mode.as_str()
            );
        }
        None
    };
    let completed = ledger
        .as_ref()
        .map(|ledger| ledger.completed_nodes().clone())
        .unwrap_or_default();
    let pending = loaded.nodes.len() - completed.len();
    events.emit(RunEvent::Planned {
        pending,
        completed: completed.len(),
    });
    if !config.assume_yes
        && !operator.confirm(
            ConfirmKind::PreRun,
            match config.mode {
                ExecutionMode::Simulation => {
                    "proceed with the exact reviewed facility simulation? [y/N] "
                }
                ExecutionMode::Live => {
                    "proceed with the exact reviewed facility plan? Devices may move. [y/N] "
                }
            },
        )?
    {
        return Ok(ExecutionOutcome::Cancelled);
    }
    if ledger.is_none() {
        ledger = Some(ExecutionLedger::create(
            &loaded.directory,
            &loaded.plan_sha256,
            inventory_sha256,
            valid_nodes,
            config.mode,
            clock,
        )?);
    }
    let ledger = ledger.as_mut().expect("the pre-run gate opened a ledger");

    let mut executed = 0usize;
    for node in &loaded.nodes {
        if completed.contains(&node.id) {
            events.emit(RunEvent::NodeSkipped {
                id: node.id.clone(),
            });
            continue;
        }
        ledger.append(&node.id, LedgerEvent::Started, clock)?;
        events.emit(RunEvent::NodeStarted {
            id: node.id.clone(),
        });
        match execute_execution_node(node, registry, operator, events) {
            Ok(NodeExecution::Done) => {
                ledger.append(&node.id, LedgerEvent::Completed, clock)?;
                events.emit(RunEvent::NodeCompleted {
                    id: node.id.clone(),
                });
                executed += 1;
            }
            Ok(NodeExecution::Declined) => {
                ledger.append(&node.id, LedgerEvent::Failed, clock)?;
                return Ok(ExecutionOutcome::Declined {
                    node: node.id.clone(),
                });
            }
            Err(error) => {
                ledger.append(&node.id, LedgerEvent::Failed, clock)?;
                return Ok(ExecutionOutcome::Failed {
                    node: node.id.clone(),
                    error: format!("{error:#}"),
                });
            }
        }
    }
    let ended_at_unix_seconds = ledger
        .last_completed_at_unix_seconds()
        .unwrap_or_else(|| clock.now_unix());
    Ok(ExecutionOutcome::Completed {
        executed,
        skipped: completed.len(),
        started_at_unix_seconds: ledger.started_at_unix_seconds(),
        ended_at_unix_seconds,
    })
}

enum NodeExecution {
    Done,
    Declined,
}

fn execute_execution_node(
    node: &LoadedExecutionNode,
    registry: &mut ExecutorRegistry,
    operator: &mut dyn Operator,
    events: &mut dyn EventSink,
) -> Result<NodeExecution> {
    match &node.action {
        LoadedExecutionAction::Execute {
            requirements,
            document: Some(document),
        } => {
            let requirement = requirements
                .first()
                .expect("execution-plan validation requires a non-empty binding set");
            let adapter = requirement
                .adapter
                .as_ref()
                .expect("runtime readiness requires an adapter");
            events.emit(RunEvent::DocumentStarted {
                asset: requirement.asset.clone(),
                driver: adapter.driver.clone(),
                format: document.format().to_owned(),
                title: document.title().to_owned(),
            });
            registry
                .executor_mut(&requirement.asset, &adapter.driver, document.format())
                .expect("runtime readiness resolved the exact executor")
                .execute(document, events)
                .with_context(|| {
                    format!(
                        "asset '{}' failed through adapter '{}' for node '{}'",
                        requirement.asset, adapter.driver, node.id
                    )
                })?;
            Ok(NodeExecution::Done)
        }
        LoadedExecutionAction::Execute { document: None, .. } => {
            unreachable!("runtime readiness rejects planning-only execute nodes")
        }
        LoadedExecutionAction::MoveMaterial {
            material,
            from,
            to,
            instructions,
        } => {
            let prompt = format!("{instructions} ({material}: {from} -> {to})");
            events.emit(RunEvent::AttentionRequired {
                node: node.id.clone(),
                prompt,
            });
            let confirmed = operator.confirm(
                ConfirmKind::Handoff,
                "done, and the facility matches the reviewed plan? Continue [y/N] ",
            )?;
            events.emit(RunEvent::AttentionReleased {
                node: node.id.clone(),
            });
            if !confirmed {
                return Ok(NodeExecution::Declined);
            }
            events.emit(RunEvent::LabwareMoved {
                labware: material.clone(),
                from: from.clone(),
                to: to.clone(),
            });
            Ok(NodeExecution::Done)
        }
        LoadedExecutionAction::Manual {
            requirements,
            title,
            instructions,
        } => {
            let asset = &requirements[0].asset;
            let offerings = requirements
                .iter()
                .map(|requirement| requirement.offering.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            events.emit(RunEvent::AttentionRequired {
                node: node.id.clone(),
                prompt: format!(
                    "{title} on Asset '{asset}' using CapabilityOfferings [{offerings}]: {instructions}"
                ),
            });
            let confirmed = operator.confirm(
                ConfirmKind::Manual,
                "done, and the facility matches the reviewed plan? Continue [y/N] ",
            )?;
            events.emit(RunEvent::AttentionReleased {
                node: node.id.clone(),
            });
            if confirmed {
                Ok(NodeExecution::Done)
            } else {
                Ok(NodeExecution::Declined)
            }
        }
    }
}

/// Loads and eagerly validates the well-known reviewed plan in `directory`.
///
/// Document semantics come exclusively from `document_loaders`, the exact set of adapter-format
/// implementations linked by the calling application.
pub fn load_execution_directory(
    directory: &Path,
    document_loaders: &ReviewedDocumentLoaderRegistry,
) -> Result<LoadedExecutionPlan> {
    let directory = fs::canonicalize(directory).with_context(|| {
        format!(
            "failed to resolve execution directory {}",
            directory.display()
        )
    })?;
    let plan_path = directory.join(EXECUTION_PLAN_FILE);
    let plan_bytes = fs::read(&plan_path)
        .with_context(|| format!("failed to read reviewed plan {}", plan_path.display()))?;
    let plan_sha256 = sha256_hex(&plan_bytes);
    let plan: ExecutionPlanDocument = serde_json::from_slice(&plan_bytes)
        .with_context(|| format!("{} is not a valid execution plan", plan_path.display()))?;
    if plan.format != EXECUTION_PLAN_FORMAT {
        bail!(
            "{} declares format '{}', expected '{}'",
            plan_path.display(),
            plan.format,
            EXECUTION_PLAN_FORMAT
        );
    }
    plan.validate()
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("{} is not a valid execution plan", plan_path.display()))?;

    let inventory = InventorySnapshot::load(
        &directory,
        &plan.inventory.document,
        Some(&plan.inventory.facility),
    )
    .with_context(|| {
        format!(
            "failed to validate frozen inventory source '{}'",
            plan.inventory.document
        )
    })?;
    if inventory.source_sha256() != plan.inventory.source_sha256 {
        bail!(
            "frozen inventory source '{}' has SHA-256 {}, but the reviewed plan requires {}",
            plan.inventory.document,
            inventory.source_sha256(),
            plan.inventory.source_sha256
        );
    }

    if let Some(planning) = &plan.planning {
        for (label, artifact) in planning.artifacts() {
            read_frozen_input(
                &directory,
                &artifact.path,
                &artifact.sha256,
                &format!("compiler {label}"),
            )?;
        }
    }

    validate_catalog_bindings(&plan, &inventory)?;
    for requirement in &plan.requirements {
        if let Some(adapter) = &requirement.adapter {
            read_frozen_input(
                &directory,
                &adapter.profile_path,
                &adapter.profile_sha256,
                &format!(
                    "adapter profile for requirement '{}'",
                    requirement.requirement_instance
                ),
            )?;
        }
    }
    let requirements = plan
        .requirements
        .iter()
        .map(|requirement| (requirement.requirement_instance.as_str(), requirement))
        .collect::<BTreeMap<_, _>>();
    let ordered = topological_nodes(&plan.nodes);
    let mut nodes = Vec::with_capacity(ordered.len());
    for node in ordered {
        let action = match &node.action {
            ExecutionPlanAction::Execute {
                requirements: node_requirements,
                document,
            } => {
                let bindings = node_requirements
                    .iter()
                    .map(|requirement| {
                        (*requirements
                            .get(requirement.as_str())
                            .expect("execution-plan validation resolved every requirement"))
                        .clone()
                    })
                    .collect::<Vec<_>>();
                let binding = bindings
                    .first()
                    .expect("execution-plan validation requires a non-empty binding set");
                let loaded = match document {
                    Some(document) => {
                        let adapter = binding.adapter.as_ref().with_context(|| {
                            format!(
                                "execute node '{}' has a reviewed document but no adapter binding",
                                node.id
                            )
                        })?;
                        let bytes = read_frozen_input(
                            &directory,
                            &document.path,
                            &document.sha256,
                            &format!("reviewed run document for node '{}'", node.id),
                        )?;
                        Some(document_loaders.load(
                            &adapter.driver,
                            &document.format,
                            &binding.capability_kind,
                            &bytes,
                            &directory.join(&document.path),
                        )?)
                    }
                    None => None,
                };
                LoadedExecutionAction::Execute {
                    requirements: bindings,
                    document: loaded,
                }
            }
            ExecutionPlanAction::MoveMaterial {
                material,
                from,
                to,
                instructions,
            } => LoadedExecutionAction::MoveMaterial {
                material: material.clone(),
                from: from.clone(),
                to: to.clone(),
                instructions: instructions.clone(),
            },
            ExecutionPlanAction::Manual {
                requirements: node_requirements,
                title,
                instructions,
            } => LoadedExecutionAction::Manual {
                requirements: node_requirements
                    .iter()
                    .map(|requirement| {
                        (*requirements
                            .get(requirement.as_str())
                            .expect("execution-plan validation resolved every requirement"))
                        .clone()
                    })
                    .collect(),
                title: title.clone(),
                instructions: instructions.clone(),
            },
        };
        nodes.push(LoadedExecutionNode {
            id: node.id.clone(),
            after: node.after.clone(),
            action,
        });
    }

    Ok(LoadedExecutionPlan {
        directory,
        plan,
        plan_sha256,
        inventory,
        nodes,
    })
}

fn validate_catalog_bindings(
    plan: &ExecutionPlanDocument,
    inventory: &InventorySnapshot,
) -> Result<()> {
    for binding in &plan.requirements {
        let asset = inventory.facility_asset(&binding.asset).with_context(|| {
            format!(
                "requirement '{}' binds invalid asset '{}'",
                binding.requirement_instance, binding.asset
            )
        })?;
        let offering = asset
            .offerings
            .iter()
            .find(|offering| offering.identity.as_str() == binding.offering)
            .with_context(|| {
                format!(
                    "requirement '{}' binds offering '{}', which asset '{}' does not own",
                    binding.requirement_instance, binding.offering, binding.asset
                )
            })?;
        if !offering.effectively_active {
            bail!(
                "requirement '{}' binds inactive offering '{}'",
                binding.requirement_instance,
                binding.offering
            );
        }
        if offering.capability_kind.as_str() != binding.capability_kind {
            bail!(
                "requirement '{}' records capability '{}', but offering '{}' exposes '{}'",
                binding.requirement_instance,
                binding.capability_kind,
                binding.offering,
                offering.capability_kind
            );
        }
        if offering.qualification.iri() != binding.observed_qualification {
            bail!(
                "requirement '{}' records qualification '{}', but offering '{}' has '{}'",
                binding.requirement_instance,
                binding.observed_qualification,
                binding.offering,
                offering.qualification.iri()
            );
        }
        let minimum = sbol_inventory::vocabulary::Qualification::try_from(
            binding.minimum_qualification.as_str(),
        )
        .with_context(|| {
            format!(
                "requirement '{}' has an unknown minimum qualification",
                binding.requirement_instance
            )
        })?;
        if offering.qualification < minimum {
            bail!(
                "requirement '{}' needs qualification '{}' but offering '{}' has only '{}'",
                binding.requirement_instance,
                minimum.iri(),
                binding.offering,
                offering.qualification.iri()
            );
        }
        if offering.control_mode.iri() != binding.control_mode {
            bail!(
                "requirement '{}' records control mode '{}', but offering '{}' has '{}'",
                binding.requirement_instance,
                binding.control_mode,
                binding.offering,
                offering.control_mode.iri()
            );
        }
        for parameter in &binding.parameters {
            let observed = offering
                .parameters
                .iter()
                .find(|candidate| candidate.identity.as_str() == parameter.offering_parameter)
                .with_context(|| {
                    format!(
                        "requirement '{}' binds missing offering parameter '{}'",
                        binding.requirement_instance, parameter.offering_parameter
                    )
                })?;
            let frozen_observed =
                execution_property_value(&parameter.observed, parameter.observed_unit.as_deref())
                    .with_context(|| {
                    format!(
                        "requirement '{}' records an invalid observed parameter value",
                        binding.requirement_instance
                    )
                })?;
            let catalog_observed = facility_property_value(
                &observed.value,
                observed.unit.as_ref().map(|unit| unit.as_str()),
            )
            .with_context(|| {
                format!(
                    "offering parameter '{}' has a value outside the execution type system",
                    observed.identity
                )
            })?;
            let constraint = PropertyConstraint {
                property_kind: PropertyKind::new(parameter.property_kind.clone())?,
                relation: parameter.relation,
                required: execution_property_value(
                    &parameter.required,
                    parameter.required_unit.as_deref(),
                )
                .with_context(|| {
                    format!(
                        "requirement '{}' records an invalid required parameter value",
                        binding.requirement_instance
                    )
                })?,
            };
            if observed.property_kind.as_str() != parameter.property_kind
                || !frozen_observed.semantically_equals(&catalog_observed)
                || !constraint.is_satisfied_by(&catalog_observed)?
            {
                bail!(
                    "requirement '{}' has a parameter binding inconsistent with '{}'",
                    binding.requirement_instance,
                    parameter.offering_parameter
                );
            }
        }
    }

    let lots = inventory.active_material_lots()?;
    for material in &plan.materials {
        let component = Iri::new(material.component.clone())
            .with_context(|| format!("material '{}' has an invalid Component IRI", material.id))?;
        if !lots
            .candidates(&component)
            .iter()
            .any(|lot| lot.as_str() == material.material_lot)
        {
            bail!(
                "material '{}' binds lot '{}', which is not an active realization of '{}' in the selected facility",
                material.id,
                material.material_lot,
                material.component
            );
        }
    }
    validate_output_bindings(plan, inventory)?;
    Ok(())
}

fn validate_output_bindings(
    plan: &ExecutionPlanDocument,
    inventory: &InventorySnapshot,
) -> Result<()> {
    let document = inventory.document();
    let selected_facility = Resource::Iri(inventory.facility().clone());
    let mut planned_occupancy = BTreeSet::new();
    for output in &plan.outputs {
        let namespace = Namespace::new(output.namespace.clone()).with_context(|| {
            format!(
                "output material '{}' has an invalid SBOL namespace",
                output.id
            )
        })?;
        DisplayId::new(output.display_id.clone()).with_context(|| {
            format!(
                "output material '{}' has an invalid SBOL displayId",
                output.id
            )
        })?;
        let identity = Resource::Iri(Iri::new(output.material_lot.clone()).with_context(|| {
            format!(
                "output material '{}' has an invalid MaterialLot IRI",
                output.id
            )
        })?);
        if identity.to_string() != format!("{}/{}", namespace.as_str(), output.display_id) {
            bail!(
                "output material '{}' identity is inconsistent with its namespace and displayId",
                output.id
            );
        }
        if document.as_sbol_document().get(&identity).is_some() {
            bail!(
                "output MaterialLot '{}' already exists in the reviewed inventory",
                output.material_lot
            );
        }
        Iri::new(output.material_kind.clone()).with_context(|| {
            format!(
                "output material '{}' has an invalid material-kind IRI",
                output.id
            )
        })?;
        let component = Resource::Iri(Iri::new(output.component.clone()).with_context(|| {
            format!(
                "output material '{}' has an invalid Component IRI",
                output.id
            )
        })?);
        let component_object = document
            .as_sbol_document()
            .get(&component)
            .with_context(|| {
                format!(
                    "output material '{}' references missing Component '{}'",
                    output.id, output.component
                )
            })?;
        if !component_object
            .rdf_types()
            .iter()
            .any(|kind| kind.as_str() == sbol_inventory::vocabulary::SBOL_COMPONENT)
        {
            bail!(
                "output material '{}' built identity '{}' is not an SBOL Component",
                output.id,
                output.component
            );
        }

        let Some(location) = output.located_in.as_ref() else {
            if output.position.is_some() {
                bail!(
                    "output material '{}' has a position without a location",
                    output.id
                );
            }
            continue;
        };
        let location = Resource::Iri(Iri::new(location.clone()).with_context(|| {
            format!(
                "output material '{}' has an invalid location IRI",
                output.id
            )
        })?);
        if let Some(zone) = document.zone(&location) {
            if zone.facility_id() != Some(&selected_facility) {
                bail!(
                    "output material '{}' is located in a Zone outside the selected facility",
                    output.id
                );
            }
            if output.position.is_some() {
                bail!(
                    "output material '{}' cannot name a position when located directly in a Zone",
                    output.id
                );
            }
            continue;
        }
        let asset = document.asset(&location).with_context(|| {
            format!(
                "output material '{}' location '{}' is not a local Zone or Asset",
                output.id, location
            )
        })?;
        if asset.facility_id() != Some(&selected_facility) {
            bail!(
                "output material '{}' is located in an Asset outside the selected facility",
                output.id
            );
        }
        let allowed = asset.allowed_positions().collect::<BTreeSet<_>>();
        if !allowed.is_empty()
            && output
                .position
                .as_deref()
                .is_none_or(|position| !allowed.contains(position))
        {
            bail!(
                "output material '{}' needs one of Asset '{}' positions: {}",
                output.id,
                location,
                allowed.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
        if let Some(position) = output.position.as_deref() {
            if position.trim().is_empty() {
                bail!("output material '{}' has a blank position", output.id);
            }
            let occupied_by_asset = document.assets().any(|candidate| {
                candidate.located_in_id() == Some(&location)
                    && candidate.position() == Some(position)
            });
            let occupied_by_material = document.material_lots().any(|candidate| {
                candidate.located_in_id() == Some(&location)
                    && candidate.position() == Some(position)
            });
            if occupied_by_asset || occupied_by_material {
                bail!(
                    "output material '{}' targets occupied position '{}' on Asset '{}'",
                    output.id,
                    position,
                    location
                );
            }
            if !planned_occupancy.insert((location.clone(), position.to_owned())) {
                bail!(
                    "several output materials target position '{}' on Asset '{}'",
                    position,
                    location
                );
            }
        }
    }
    Ok(())
}

fn execution_property_value(
    value: &ExecutionParameterValue,
    unit: Option<&str>,
) -> Result<PropertyValue> {
    let value = match value {
        ExecutionParameterValue::Text(value) => ScalarValue::Text(value.clone()),
        ExecutionParameterValue::Integer(value) => {
            ScalarValue::Integer(ExactInteger::parse(value)?)
        }
        ExecutionParameterValue::Real(value) => ScalarValue::Real(ExactDecimal::parse(value)?),
        ExecutionParameterValue::Boolean(value) => ScalarValue::Boolean(*value),
        ExecutionParameterValue::Iri(value) => ScalarValue::Iri(AbsoluteIri::new(value.clone())?),
    };
    let unit = unit.map(UnitIri::new).transpose()?;
    Ok(PropertyValue::new(value, unit)?)
}

fn facility_property_value(
    value: &FacilityScalarValue,
    unit: Option<&str>,
) -> Result<PropertyValue> {
    let value = match value {
        FacilityScalarValue::Text(value) => ScalarValue::Text(value.clone()),
        FacilityScalarValue::Integer(value) => ScalarValue::Integer(ExactInteger::parse(value)?),
        FacilityScalarValue::Real(value) => ScalarValue::Real(ExactDecimal::parse(value)?),
        FacilityScalarValue::Boolean(value) => ScalarValue::Boolean(*value),
        FacilityScalarValue::Iri(value) => {
            ScalarValue::Iri(AbsoluteIri::new(value.as_str().to_owned())?)
        }
    };
    let unit = unit.map(UnitIri::new).transpose()?;
    Ok(PropertyValue::new(value, unit)?)
}

fn read_frozen_input(
    directory: &Path,
    relative: &str,
    expected_sha256: &str,
    label: &str,
) -> Result<Vec<u8>> {
    let joined = directory.join(relative);
    let resolved = fs::canonicalize(&joined)
        .with_context(|| format!("failed to resolve {label} at {}", joined.display()))?;
    if !resolved.starts_with(directory) {
        bail!("{label} path '{relative}' resolves outside the execution directory");
    }
    let bytes = fs::read(&resolved)
        .with_context(|| format!("failed to read {label} at {}", resolved.display()))?;
    let observed = sha256_hex(&bytes);
    if observed != expected_sha256 {
        bail!(
            "{label} at '{}' has SHA-256 {observed}, but the reviewed plan requires {expected_sha256}",
            relative
        );
    }
    Ok(bytes)
}

fn topological_nodes(nodes: &[ExecutionPlanNode]) -> Vec<&ExecutionPlanNode> {
    let by_id = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut indegree = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.after.len()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    for node in nodes {
        for dependency in &node.after {
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(node.id.as_str());
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(nodes.len());
    while let Some(id) = ready.pop_first() {
        ordered.push(by_id[id]);
        for dependent in dependents.get(id).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("execution-plan validation resolved every dependency");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(dependent);
            }
        }
    }
    debug_assert_eq!(ordered.len(), nodes.len());
    ordered
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use lab_runfmt::{
        ExecutionAdapterBinding, ExecutionInventoryReference, ExecutionMaterialBinding,
        ExecutionMaterialOutput, ExecutionMethodSelection, ExecutionPlanAction, ExecutionPlanNode,
        ExecutionPlanningArtifact, ExecutionPlanningReference, ExecutionRequirementBinding,
        ReviewedRunDocument, RunStep, STAR_RUN_FORMAT, StarRunDocument,
    };

    use super::*;
    use crate::clock::Clock;
    use crate::events::{RecordingSink, RunEvent};
    use crate::operator::AutoOperator;
    use crate::reviewed_documents::{
        LoadedExternalFile, load_opentrons_protocol_designer, load_opentrons_python_protocol,
        load_plate_read, load_simulation_run, load_star_run, load_thermocycle_run,
    };

    const INVENTORY: &str = r#"@prefix cap: <https://sbol.io/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix inv: <https://sbol.io/ns/inventory#> .
@prefix sbol: <http://sbols.org/v3#> .

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/facility> .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:star a sbol:TopLevel, fac:Asset ; sbol:displayId "star" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:Instrument ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/facility/star/liquid_handling> .
<https://example.org/facility/star/liquid_handling>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "liquid_handling" ;
    fac:capabilityKind cap:LiquidHandling ; fac:qualification fac:Executable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true .
ex:manual_workstation a sbol:TopLevel, fac:Asset ; sbol:displayId "manual_workstation" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:Workstation ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/facility/manual_workstation/material_provisioning> .
<https://example.org/facility/manual_workstation/material_provisioning>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "material_provisioning" ;
    fac:capabilityKind cap:MaterialProvisioning ; fac:qualification fac:Executable ;
    fac:controlMode fac:ManualControl ; fac:isActive true .
ex:design a sbol:Component ; sbol:displayId "design" ;
    sbol:hasNamespace <https://example.org/facility> ;
    sbol:type <https://identifiers.org/SBO:0000251> .
ex:input_lot a sbol:Implementation ; sbol:displayId "input_lot" ;
    sbol:hasNamespace <https://example.org/facility> ; sbol:built ex:design ;
    fac:materialKind inv:DnaSample ; fac:facility ex:facility ; fac:isActive true ;
    fac:locatedIn ex:room .
"#;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_unix(&self) -> u64 {
            1_725_000_000
        }
    }

    struct RecordingExecutor {
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct CustomReviewedPayload(String);

    fn load_custom_document(
        request: ReviewedDocumentLoadRequest<'_>,
    ) -> Result<LoadedReviewedDocument> {
        LoadedReviewedDocument::new(
            request.format,
            "Custom reviewed document",
            CustomReviewedPayload(String::from_utf8(request.bytes.to_vec())?),
        )
    }

    pub(crate) fn document_loaders() -> ReviewedDocumentLoaderRegistry {
        let mut registry = ReviewedDocumentLoaderRegistry::new();
        registry
            .register("hamilton.star", STAR_RUN_FORMAT, load_star_run)
            .unwrap();
        registry
            .register(
                "inheco.odtc",
                lab_runfmt::THERMOCYCLE_RUN_FORMAT,
                load_thermocycle_run,
            )
            .unwrap();
        registry
            .register(
                "byonoy.absorbance96",
                lab_runfmt::PLATE_READ_FORMAT,
                load_plate_read,
            )
            .unwrap();
        registry
            .register(
                "lab.simulator",
                lab_runfmt::SIMULATION_RUN_FORMAT,
                load_simulation_run,
            )
            .unwrap();
        registry
            .register(
                "opentrons.ot2",
                lab_runfmt::OPENTRONS_PYTHON_PROTOCOL_FORMAT,
                load_opentrons_python_protocol,
            )
            .unwrap();
        registry
            .register(
                "opentrons.flex",
                lab_runfmt::OPENTRONS_PROTOCOL_DESIGNER_FORMAT,
                load_opentrons_protocol_designer,
            )
            .unwrap();
        registry
    }

    #[test]
    fn reviewed_document_loaders_are_exact_and_payloads_are_extensible() {
        let mut registry = ReviewedDocumentLoaderRegistry::new();
        registry
            .register(
                "example.custom",
                "example.reviewed.v1",
                load_custom_document,
            )
            .unwrap();

        let loaded = registry
            .load(
                "example.custom",
                "example.reviewed.v1",
                "https://example.org/capability",
                b"typed payload",
                Path::new("custom.reviewed"),
            )
            .unwrap();
        assert_eq!(loaded.format(), "example.reviewed.v1");
        assert_eq!(
            loaded.payload::<CustomReviewedPayload>(),
            Some(&CustomReviewedPayload("typed payload".to_owned()))
        );

        let error = registry
            .load(
                "different.adapter",
                "example.reviewed.v1",
                "https://example.org/capability",
                b"typed payload",
                Path::new("custom.reviewed"),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("no reviewed-document loader"), "{error}");

        let duplicate = registry
            .register(
                "example.custom",
                "example.reviewed.v1",
                load_custom_document,
            )
            .unwrap_err()
            .to_string();
        assert!(duplicate.contains("already registered"), "{duplicate}");
    }

    #[test]
    fn a_loader_cannot_return_a_different_format_than_its_exact_key() {
        let mut registry = ReviewedDocumentLoaderRegistry::new();
        registry
            .register("example.custom", "example.reviewed.v1", |_request| {
                LoadedReviewedDocument::new("wrong.format", "Wrong", ())
            })
            .unwrap();

        let error = registry
            .load(
                "example.custom",
                "example.reviewed.v1",
                "https://example.org/capability",
                b"",
                Path::new("custom.reviewed"),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("returned format 'wrong.format'"), "{error}");
    }

    #[test]
    fn preflight_validates_external_ot2_protocols_without_claiming_a_live_executor() {
        let source = br#"from opentrons import protocol_api
PLAN_JSON = "{}"  # LAB:INVOCATION_PLAN
def run(protocol: protocol_api.ProtocolContext) -> None:
    pass
"#;
        let loaded = document_loaders()
            .load(
                "opentrons.ot2",
                "opentrons.python-protocol",
                "https://sbol.io/ns/capability#LiquidHandling",
                source,
                Path::new("automation_protocol.py"),
            )
            .unwrap();

        assert_eq!(loaded.format(), "opentrons.python-protocol");
        assert_eq!(loaded.title(), "Opentrons OT-2 LiquidHandling protocol");
        assert_eq!(
            loaded
                .payload::<LoadedExternalFile>()
                .expect("the Opentrons loader returns its reviewed file")
                .contents,
            source
        );

        let error = document_loaders()
            .load(
                "opentrons.ot2",
                "opentrons.python-protocol",
                "https://sbol.io/ns/capability#LiquidHandling",
                b"def run(): pass\n",
                Path::new("automation_protocol.py"),
            )
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("missing required Opentrons protocol marker"),
            "{error}"
        );
    }

    impl DocumentExecutor for RecordingExecutor {
        fn execute(
            &mut self,
            document: &LoadedReviewedDocument,
            _events: &mut dyn EventSink,
        ) -> Result<()> {
            self.calls.lock().unwrap().push(document.title().to_owned());
            Ok(())
        }
    }

    fn registry(calls: Arc<Mutex<Vec<String>>>) -> ExecutorRegistry {
        let mut registry = ExecutorRegistry::new();
        registry
            .register(
                "https://example.org/facility/star",
                "hamilton.star",
                STAR_RUN_FORMAT,
                Box::new(RecordingExecutor { calls }),
            )
            .unwrap();
        registry
    }

    pub(crate) fn write_execution_package() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("adapters")).unwrap();
        fs::create_dir_all(directory.path().join("runs")).unwrap();
        fs::write(directory.path().join("inventory-source.ttl"), INVENTORY).unwrap();
        fs::write(directory.path().join("adapters/star.toml"), "").unwrap();

        let run = StarRunDocument {
            format: STAR_RUN_FORMAT.to_owned(),
            run: "transfer".to_owned(),
            title: "Transfer liquids".to_owned(),
            machine: "STARlet".to_owned(),
            channels: 8,
            steps: vec![RunStep {
                frame: "C0ZA".to_owned(),
                module: "C0".to_owned(),
                code: "ZA".to_owned(),
                description: "Retract channels".to_owned(),
            }],
            manual_after: Vec::new(),
        };
        let mut run_bytes = serde_json::to_vec_pretty(&run).unwrap();
        run_bytes.push(b'\n');
        fs::write(directory.path().join("runs/transfer.star.json"), &run_bytes).unwrap();

        let plan = ExecutionPlanDocument {
            format: EXECUTION_PLAN_FORMAT.to_owned(),
            inventory: ExecutionInventoryReference {
                document: "inventory-source.ttl".to_owned(),
                source_sha256: sha256_hex(INVENTORY.as_bytes()),
                facility: "https://example.org/facility/facility".to_owned(),
            },
            planning: None,
            requirements: vec![
                ExecutionRequirementBinding {
                    requirement_instance: "workflow/main/liquid".to_owned(),
                    requirement_template: "workflow::main::liquid".to_owned(),
                    capability_kind: "https://sbol.io/ns/capability#LiquidHandling".to_owned(),
                    offering: "https://example.org/facility/star/liquid_handling".to_owned(),
                    asset: "https://example.org/facility/star".to_owned(),
                    minimum_qualification: "https://sbol.io/ns/facility#Executable".to_owned(),
                    observed_qualification: "https://sbol.io/ns/facility#Executable".to_owned(),
                    control_mode: "https://sbol.io/ns/facility#ReviewedFileControl".to_owned(),
                    procedure_implementation: None,
                    parameters: Vec::new(),
                    adapter: Some(ExecutionAdapterBinding {
                        driver: "hamilton.star".to_owned(),
                        profile_path: "adapters/star.toml".to_owned(),
                        profile_sha256: sha256_hex(b""),
                    }),
                },
                ExecutionRequirementBinding {
                    requirement_instance: "workflow/main/deck-preparation".to_owned(),
                    requirement_template: "workflow::main::deck-preparation".to_owned(),
                    capability_kind: "https://sbol.io/ns/capability#MaterialProvisioning"
                        .to_owned(),
                    offering:
                        "https://example.org/facility/manual_workstation/material_provisioning"
                            .to_owned(),
                    asset: "https://example.org/facility/manual_workstation".to_owned(),
                    minimum_qualification: "https://sbol.io/ns/facility#Executable".to_owned(),
                    observed_qualification: "https://sbol.io/ns/facility#Executable".to_owned(),
                    control_mode: "https://sbol.io/ns/facility#ManualControl".to_owned(),
                    procedure_implementation: None,
                    parameters: Vec::new(),
                    adapter: None,
                },
            ],
            materials: vec![ExecutionMaterialBinding {
                id: "input".to_owned(),
                component: "https://example.org/facility/design".to_owned(),
                material_lot: "https://example.org/facility/input_lot".to_owned(),
            }],
            outputs: vec![ExecutionMaterialOutput {
                id: "output".to_owned(),
                material_lot: "https://example.org/results/output_lot".to_owned(),
                namespace: "https://example.org/results".to_owned(),
                display_id: "output_lot".to_owned(),
                component: "https://example.org/facility/design".to_owned(),
                material_kind: "https://sbol.io/ns/inventory#DnaSample".to_owned(),
                located_in: Some("https://example.org/facility/room".to_owned()),
                position: None,
                derived_from: vec!["input".to_owned()],
            }],
            // Serialized order is intentionally not dependency order.
            nodes: vec![
                ExecutionPlanNode {
                    id: "execute-0001".to_owned(),
                    after: vec!["prepare".to_owned()],
                    action: ExecutionPlanAction::Execute {
                        requirements: vec!["workflow/main/liquid".to_owned()],
                        document: Some(ReviewedRunDocument {
                            path: "runs/transfer.star.json".to_owned(),
                            format: STAR_RUN_FORMAT.to_owned(),
                            sha256: sha256_hex(&run_bytes),
                        }),
                    },
                },
                ExecutionPlanNode {
                    id: "prepare".to_owned(),
                    after: Vec::new(),
                    action: ExecutionPlanAction::Manual {
                        requirements: vec!["workflow/main/deck-preparation".to_owned()],
                        title: "Prepare the deck".to_owned(),
                        instructions: "Confirm the reviewed deck layout.".to_owned(),
                    },
                },
            ],
        };
        let mut plan_bytes = serde_json::to_vec_pretty(&plan).unwrap();
        plan_bytes.push(b'\n');
        fs::write(directory.path().join(EXECUTION_PLAN_FILE), plan_bytes).unwrap();
        directory
    }

    #[test]
    fn preflight_validates_every_frozen_input_and_orders_the_dag() {
        let directory = write_execution_package();
        let plan_bytes = fs::read(directory.path().join(EXECUTION_PLAN_FILE)).unwrap();

        let loaded = load_execution_directory(directory.path(), &document_loaders()).unwrap();

        assert_eq!(loaded.plan_sha256, sha256_hex(&plan_bytes));
        assert_eq!(loaded.nodes[0].id, "prepare");
        assert_eq!(loaded.nodes[1].id, "execute-0001");
        assert!(loaded.is_ready(ExecutionMode::Live));
        let LoadedExecutionAction::Execute {
            document: Some(document),
            ..
        } = &loaded.nodes[1].action
        else {
            panic!("the execute node should hold its prevalidated document")
        };
        assert_eq!(document.format(), STAR_RUN_FORMAT);
    }

    #[test]
    fn preflight_refuses_changed_inventory_profiles_and_documents() {
        let inventory = write_execution_package();
        fs::write(
            inventory.path().join("inventory-source.ttl"),
            format!("{INVENTORY}\n# changed\n"),
        )
        .unwrap();
        let error = load_execution_directory(inventory.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("reviewed plan requires"), "{error}");

        let profile = write_execution_package();
        fs::write(
            profile.path().join("adapters/star.toml"),
            "changed = true\n",
        )
        .unwrap();
        let error = load_execution_directory(profile.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("adapter profile"), "{error}");
        assert!(error.contains("reviewed plan requires"), "{error}");

        let document = write_execution_package();
        fs::write(document.path().join("runs/transfer.star.json"), "{}\n").unwrap();
        let error = load_execution_directory(document.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("reviewed run document"), "{error}");
        assert!(error.contains("reviewed plan requires"), "{error}");
    }

    #[test]
    fn preflight_freezes_every_compiler_intermediate() {
        let directory = write_execution_package();
        let compiler = directory.path().join("compiler");
        fs::create_dir_all(&compiler).unwrap();
        let artifacts = [
            ("planning-problem.json", b"problem\n".as_slice()),
            ("facility-solution.json", b"solution\n".as_slice()),
            ("allocated.lair", b"allocated\n".as_slice()),
            ("adapter-invocations.json", b"invocations\n".as_slice()),
        ];
        for (name, contents) in artifacts {
            fs::write(compiler.join(name), contents).unwrap();
        }
        let plan_path = directory.path().join(EXECUTION_PLAN_FILE);
        let mut plan: ExecutionPlanDocument =
            serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
        let artifact = |name: &str| ExecutionPlanningArtifact {
            path: format!("compiler/{name}"),
            sha256: sha256_hex(&fs::read(compiler.join(name)).unwrap()),
        };
        let problem = artifact("planning-problem.json");
        let allocated = artifact("allocated.lair");
        plan.planning = Some(ExecutionPlanningReference {
            problem_sha256: problem.sha256.clone(),
            allocated_lair_sha256: allocated.sha256.clone(),
            planning_problem: problem,
            facility_solution: artifact("facility-solution.json"),
            allocated_lair: allocated,
            adapter_invocations: artifact("adapter-invocations.json"),
            methods: vec![ExecutionMethodSelection {
                choice: "main::body[0]".to_owned(),
                source_operation: "std.bio.build.realize".to_owned(),
                method: "https://example.org/method#automated".to_owned(),
                tasks: vec!["main::body[0]::setup".to_owned()],
            }],
        });
        let mut bytes = serde_json::to_vec_pretty(&plan).unwrap();
        bytes.push(b'\n');
        fs::write(&plan_path, bytes).unwrap();
        load_execution_directory(directory.path(), &document_loaders()).unwrap();

        fs::write(compiler.join("allocated.lair"), "changed\n").unwrap();
        let error = load_execution_directory(directory.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("compiler allocated LAIR"), "{error}");
        assert!(error.contains("reviewed plan requires"), "{error}");
    }

    #[test]
    fn preflight_parses_document_contents_before_any_executor_can_open() {
        let directory = write_execution_package();
        let document_path = directory.path().join("runs/transfer.star.json");
        let mut document: StarRunDocument =
            serde_json::from_slice(&fs::read(&document_path).unwrap()).unwrap();
        document.steps[0].frame = "not a STAR frame".to_owned();
        let mut document_bytes = serde_json::to_vec_pretty(&document).unwrap();
        document_bytes.push(b'\n');
        fs::write(&document_path, &document_bytes).unwrap();

        let plan_path = directory.path().join(EXECUTION_PLAN_FILE);
        let mut plan: ExecutionPlanDocument =
            serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
        let ExecutionPlanAction::Execute {
            document: Some(frozen),
            ..
        } = &mut plan.nodes[0].action
        else {
            panic!("the fixture should carry a reviewed document")
        };
        frozen.sha256 = sha256_hex(&document_bytes);
        let mut plan_bytes = serde_json::to_vec_pretty(&plan).unwrap();
        plan_bytes.push(b'\n');
        fs::write(&plan_path, plan_bytes).unwrap();

        let error = load_execution_directory(directory.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("unreplayable STAR frame"), "{error}");
    }

    #[test]
    fn preflight_reprojects_exact_asset_and_offering_bindings() {
        let directory = write_execution_package();
        let plan_path = directory.path().join(EXECUTION_PLAN_FILE);
        let mut plan: ExecutionPlanDocument =
            serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
        plan.requirements[0].offering = "https://example.org/facility/star/missing".to_owned();
        let mut bytes = serde_json::to_vec_pretty(&plan).unwrap();
        bytes.push(b'\n');
        fs::write(&plan_path, bytes).unwrap();

        let error = load_execution_directory(directory.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not own"), "{error}");
    }

    #[test]
    fn preflight_validates_planned_output_materials_before_execution() {
        let directory = write_execution_package();
        let plan_path = directory.path().join(EXECUTION_PLAN_FILE);
        let mut plan: ExecutionPlanDocument =
            serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
        plan.outputs[0].component = "https://example.org/facility/missing".to_owned();
        let mut bytes = serde_json::to_vec_pretty(&plan).unwrap();
        bytes.push(b'\n');
        fs::write(&plan_path, bytes).unwrap();

        let error = load_execution_directory(directory.path(), &document_loaders())
            .unwrap_err()
            .to_string();
        assert!(error.contains("references missing Component"), "{error}");
        assert!(!directory.path().join(LEDGER_FILE).exists());
    }

    #[test]
    fn the_generic_runner_uses_only_the_exact_registered_executor_and_resumes() {
        let directory = write_execution_package();
        let loaded = load_execution_directory(directory.path(), &document_loaders()).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = registry(Arc::clone(&calls));
        let mut events = RecordingSink::default();

        let outcome = run_execution_plan(
            &loaded,
            ExecutionRunConfig {
                assume_yes: true,
                resume: false,
                mode: ExecutionMode::Live,
            },
            &mut registry,
            &mut AutoOperator { answer: true },
            &mut events,
            &FixedClock,
        )
        .unwrap();

        assert_eq!(
            outcome,
            ExecutionOutcome::Completed {
                executed: 2,
                skipped: 0,
                started_at_unix_seconds: 1_725_000_000,
                ended_at_unix_seconds: 1_725_000_000,
            }
        );
        assert_eq!(*calls.lock().unwrap(), ["Transfer liquids"]);
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                RunEvent::DocumentStarted { asset, driver, format, .. }
                    if asset == "https://example.org/facility/star"
                        && driver == "hamilton.star"
                        && format == STAR_RUN_FORMAT
            )
        }));

        let resumed = run_execution_plan(
            &loaded,
            ExecutionRunConfig {
                assume_yes: true,
                resume: true,
                mode: ExecutionMode::Live,
            },
            &mut registry,
            &mut AutoOperator { answer: true },
            &mut RecordingSink::default(),
            &FixedClock,
        )
        .unwrap();
        assert_eq!(
            resumed,
            ExecutionOutcome::Completed {
                executed: 0,
                skipped: 2,
                started_at_unix_seconds: 1_725_000_000,
                ended_at_unix_seconds: 1_725_000_000,
            }
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "resume never repeats a completed document"
        );
    }

    #[test]
    fn missing_or_inexact_executor_bindings_fail_before_a_ledger_exists() {
        let directory = write_execution_package();
        let loaded = load_execution_directory(directory.path(), &document_loaders()).unwrap();
        let mut wrong_registry = ExecutorRegistry::new();
        wrong_registry
            .register(
                "https://example.org/facility/another-star",
                "hamilton.star",
                STAR_RUN_FORMAT,
                Box::new(RecordingExecutor {
                    calls: Arc::new(Mutex::new(Vec::new())),
                }),
            )
            .unwrap();

        let error = run_execution_plan(
            &loaded,
            ExecutionRunConfig {
                assume_yes: true,
                resume: false,
                mode: ExecutionMode::Live,
            },
            &mut wrong_registry,
            &mut AutoOperator { answer: true },
            &mut RecordingSink::default(),
            &FixedClock,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("no registered executor"), "{error}");
        assert!(
            error.contains("https://example.org/facility/star"),
            "{error}"
        );
        assert!(!directory.path().join(LEDGER_FILE).exists());
    }

    #[test]
    fn declining_the_pre_run_gate_leaves_no_execution_ledger() {
        let directory = write_execution_package();
        let loaded = load_execution_directory(directory.path(), &document_loaders()).unwrap();
        let mut registry = registry(Arc::new(Mutex::new(Vec::new())));

        let outcome = run_execution_plan(
            &loaded,
            ExecutionRunConfig {
                assume_yes: false,
                resume: false,
                mode: ExecutionMode::Live,
            },
            &mut registry,
            &mut AutoOperator { answer: false },
            &mut RecordingSink::default(),
            &FixedClock,
        )
        .unwrap();

        assert_eq!(outcome, ExecutionOutcome::Cancelled);
        assert!(!directory.path().join(LEDGER_FILE).exists());
    }
}
