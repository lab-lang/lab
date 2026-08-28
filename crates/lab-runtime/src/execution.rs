//! Eager preflight for facility-wide reviewed execution plans.
//!
//! Loading is deliberately more than JSON parsing. It validates the exact inventory graph,
//! checks every frozen profile and child-document digest, projects every catalog binding back
//! onto the selected facility, validates every device document, and computes a deterministic
//! topological walk. A live runner receives only a [`LoadedExecutionPlan`], so it cannot discover
//! a bad document after an instrument has already moved.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use hamilton_star::RawCommand;
use lab_inventory::{FacilityScalarValue, InventorySnapshot};
use lab_runfmt::{
    EXECUTION_PLAN_FILE, EXECUTION_PLAN_FORMAT, ExecutionParameterValue, ExecutionPlanAction,
    ExecutionPlanDocument, ExecutionPlanNode, ExecutionRequirementBinding, PLATE_READ_FORMAT,
    PlateReadDocument, STAR_RUN_FORMAT, StarRunDocument, THERMOCYCLE_RUN_FORMAT,
    ThermocycleRunDocument,
};
use sha2::{Digest, Sha256};

use crate::clock::Clock;
use crate::events::{EventSink, RunEvent};
use crate::ledger::{ExecutionLedger, LEDGER_FILE, LedgerEvent};
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
    /// Reasons this valid reviewed plan cannot yet be run against physical devices.
    /// Planning-only plans remain useful and can still be rendered as dry runs.
    pub fn readiness_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        for node in &self.nodes {
            let LoadedExecutionAction::Execute {
                requirement,
                document,
            } = &node.action
            else {
                continue;
            };
            let qualification = sbol_inventory::vocabulary::Qualification::try_from(
                requirement.observed_qualification.as_str(),
            );
            if !qualification
                .is_ok_and(|value| value >= sbol_inventory::vocabulary::Qualification::Executable)
            {
                issues.push(format!(
                    "node '{}' is bound only at qualification '{}'",
                    node.id, requirement.observed_qualification
                ));
            }
            if requirement.adapter.is_none() {
                issues.push(format!("node '{}' has no frozen runtime adapter", node.id));
            }
            if document.is_none() {
                issues.push(format!("node '{}' has no reviewed run document", node.id));
            }
        }
        issues
    }

    pub fn is_executable(&self) -> bool {
        self.readiness_issues().is_empty()
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
        requirement: Box<ExecutionRequirementBinding>,
        document: Option<LoadedReviewedDocument>,
    },
    MoveMaterial {
        material: String,
        from: String,
        to: String,
        instructions: String,
    },
    Manual {
        title: String,
        instructions: String,
    },
}

#[derive(Debug)]
pub enum LoadedReviewedDocument {
    Star {
        document: StarRunDocument,
        commands: Vec<RawCommand>,
    },
    Thermocycle(ThermocycleRunDocument),
    PlateRead(PlateReadDocument),
}

impl LoadedReviewedDocument {
    pub fn format(&self) -> &'static str {
        match self {
            Self::Star { .. } => STAR_RUN_FORMAT,
            Self::Thermocycle(_) => THERMOCYCLE_RUN_FORMAT,
            Self::PlateRead(_) => PLATE_READ_FORMAT,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Star { document, .. } => &document.title,
            Self::Thermocycle(document) => &document.title,
            Self::PlateRead(document) => &document.title,
        }
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
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Completed { executed: usize, skipped: usize },
    Cancelled,
    Declined { node: String },
    Failed { node: String, error: String },
}

/// Renders the fully preflighted facility walk without requiring runtime connectors.
pub fn render_execution_dry_run(loaded: &LoadedExecutionPlan) -> String {
    use std::fmt::Write as _;

    let issues = loaded.readiness_issues();
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
                requirement,
                document,
            } => {
                let description = document.as_ref().map_or_else(
                    || "no reviewed run document".to_owned(),
                    |document| format!("{} ({})", document.title(), document.format()),
                );
                let adapter = requirement
                    .adapter
                    .as_ref()
                    .map_or("no runtime adapter", |adapter| adapter.driver.as_str());
                let _ = writeln!(
                    text,
                    "\n[{}] {} - {} on {} through {}: {}",
                    index + 1,
                    node.id,
                    requirement.capability_kind,
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
                title,
                instructions,
            } => {
                let _ = writeln!(
                    text,
                    "\n[{}] {} - by hand: {}: {}",
                    index + 1,
                    node.id,
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
    let mut readiness = loaded.readiness_issues();
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
            "reviewed plan is not executable:\n  - {}",
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
        )?)
    } else {
        let path = loaded.directory.join(LEDGER_FILE);
        if path.exists() {
            bail!(
                "{} already exists; resume the reviewed plan instead of replacing durable physical state",
                path.display()
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
            "proceed with the exact reviewed facility plan? Devices may move. [y/N] ",
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
    Ok(ExecutionOutcome::Completed {
        executed,
        skipped: completed.len(),
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
            requirement,
            document: Some(document),
        } => {
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
            title,
            instructions,
        } => {
            events.emit(RunEvent::AttentionRequired {
                node: node.id.clone(),
                prompt: format!("{title}: {instructions}"),
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
pub fn load_execution_directory(directory: &Path) -> Result<LoadedExecutionPlan> {
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
                requirement,
                document,
            } => {
                let binding = requirements
                    .get(requirement.as_str())
                    .expect("execution-plan validation resolved every requirement");
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
                        Some(load_reviewed_document(
                            &adapter.driver,
                            &document.format,
                            &bytes,
                            &directory.join(&document.path),
                        )?)
                    }
                    None => None,
                };
                LoadedExecutionAction::Execute {
                    requirement: Box::new((*binding).clone()),
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
                title,
                instructions,
            } => LoadedExecutionAction::Manual {
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
            if parameter.relation != "exact"
                || observed.property_kind.as_str() != parameter.property_kind
                || !scalar_equal(&parameter.observed, &observed.value)
                || observed.unit.as_ref().map(|unit| unit.as_str())
                    != parameter.observed_unit.as_deref()
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
        let component = sbol3::Iri::new(material.component.clone())
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
    Ok(())
}

fn scalar_equal(expected: &ExecutionParameterValue, observed: &FacilityScalarValue) -> bool {
    match (expected, observed) {
        (ExecutionParameterValue::Text(left), FacilityScalarValue::Text(right))
        | (ExecutionParameterValue::Integer(left), FacilityScalarValue::Integer(right))
        | (ExecutionParameterValue::Real(left), FacilityScalarValue::Real(right)) => left == right,
        (ExecutionParameterValue::Boolean(left), FacilityScalarValue::Boolean(right)) => {
            left == right
        }
        (ExecutionParameterValue::Iri(left), FacilityScalarValue::Iri(right)) => {
            left == right.as_str()
        }
        _ => false,
    }
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

fn load_reviewed_document(
    driver: &str,
    format: &str,
    bytes: &[u8],
    path: &Path,
) -> Result<LoadedReviewedDocument> {
    match (driver, format) {
        ("hamilton.star", STAR_RUN_FORMAT) => {
            let document: StarRunDocument = parse_json_document(bytes, path)?;
            if document.format != STAR_RUN_FORMAT {
                bail!(
                    "{} declares format '{}', expected '{}'",
                    path.display(),
                    document.format,
                    STAR_RUN_FORMAT
                );
            }
            let commands = document
                .steps
                .iter()
                .map(|step| {
                    RawCommand::parse(&step.frame).with_context(|| {
                        format!("{} carries an unreplayable STAR frame", path.display())
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LoadedReviewedDocument::Star { document, commands })
        }
        ("inheco.odtc", THERMOCYCLE_RUN_FORMAT) => {
            let document: ThermocycleRunDocument = parse_json_document(bytes, path)?;
            if document.format != THERMOCYCLE_RUN_FORMAT {
                bail!(
                    "{} declares format '{}', expected '{}'",
                    path.display(),
                    document.format,
                    THERMOCYCLE_RUN_FORMAT
                );
            }
            document
                .profile
                .validate(&lab_instruments::odtc_thermal_limits())
                .with_context(|| {
                    format!("{} is outside the Inheco ODTC envelope", path.display())
                })?;
            Ok(LoadedReviewedDocument::Thermocycle(document))
        }
        ("byonoy.absorbance96", PLATE_READ_FORMAT) => {
            let document: PlateReadDocument = parse_json_document(bytes, path)?;
            if document.format != PLATE_READ_FORMAT {
                bail!(
                    "{} declares format '{}', expected '{}'",
                    path.display(),
                    document.format,
                    PLATE_READ_FORMAT
                );
            }
            Ok(LoadedReviewedDocument::PlateRead(document))
        }
        _ => bail!(
            "adapter '{driver}' has no runtime executor for reviewed document format '{format}'"
        ),
    }
}

fn parse_json_document<T: serde::de::DeserializeOwned>(bytes: &[u8], path: &Path) -> Result<T> {
    serde_json::from_slice(bytes)
        .with_context(|| format!("{} is not a valid reviewed run document", path.display()))
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
mod tests {
    use std::sync::{Arc, Mutex};

    use lab_runfmt::{
        ExecutionAdapterBinding, ExecutionInventoryReference, ExecutionPlanAction,
        ExecutionPlanNode, ExecutionRequirementBinding, ReviewedRunDocument, RunStep,
        STAR_RUN_FORMAT, StarRunDocument,
    };

    use super::*;
    use crate::clock::Clock;
    use crate::events::{RecordingSink, RunEvent};
    use crate::operator::AutoOperator;

    const INVENTORY: &str = r#"@prefix cap: <https://draggon.org/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://draggon.org/ns/facility#> .
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

    fn write_execution_package() -> tempfile::TempDir {
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
            requirements: vec![ExecutionRequirementBinding {
                requirement_instance: "workflow/main/liquid".to_owned(),
                requirement_template: "workflow::main::liquid".to_owned(),
                capability_kind: "https://draggon.org/ns/capability#LiquidHandling".to_owned(),
                offering: "https://example.org/facility/star/liquid_handling".to_owned(),
                asset: "https://example.org/facility/star".to_owned(),
                minimum_qualification: "https://draggon.org/ns/facility#Executable".to_owned(),
                observed_qualification: "https://draggon.org/ns/facility#Executable".to_owned(),
                control_mode: "https://draggon.org/ns/facility#ReviewedFileControl".to_owned(),
                parameters: Vec::new(),
                adapter: Some(ExecutionAdapterBinding {
                    driver: "hamilton.star".to_owned(),
                    profile_path: "adapters/star.toml".to_owned(),
                    profile_sha256: sha256_hex(b""),
                }),
            }],
            materials: Vec::new(),
            // Serialized order is intentionally not dependency order.
            nodes: vec![
                ExecutionPlanNode {
                    id: "execute-0001".to_owned(),
                    after: vec!["prepare".to_owned()],
                    action: ExecutionPlanAction::Execute {
                        requirement: "workflow/main/liquid".to_owned(),
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

        let loaded = load_execution_directory(directory.path()).unwrap();

        assert_eq!(loaded.plan_sha256, sha256_hex(&plan_bytes));
        assert_eq!(loaded.nodes[0].id, "prepare");
        assert_eq!(loaded.nodes[1].id, "execute-0001");
        assert!(loaded.is_executable());
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
        let error = load_execution_directory(inventory.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("reviewed plan requires"), "{error}");

        let profile = write_execution_package();
        fs::write(
            profile.path().join("adapters/star.toml"),
            "changed = true\n",
        )
        .unwrap();
        let error = load_execution_directory(profile.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("adapter profile"), "{error}");
        assert!(error.contains("reviewed plan requires"), "{error}");

        let document = write_execution_package();
        fs::write(document.path().join("runs/transfer.star.json"), "{}\n").unwrap();
        let error = load_execution_directory(document.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("reviewed run document"), "{error}");
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

        let error = load_execution_directory(directory.path())
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

        let error = load_execution_directory(directory.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not own"), "{error}");
    }

    #[test]
    fn the_generic_runner_uses_only_the_exact_registered_executor_and_resumes() {
        let directory = write_execution_package();
        let loaded = load_execution_directory(directory.path()).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = registry(Arc::clone(&calls));
        let mut events = RecordingSink::default();

        let outcome = run_execution_plan(
            &loaded,
            ExecutionRunConfig {
                assume_yes: true,
                resume: false,
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
                skipped: 0
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
                skipped: 2
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
        let loaded = load_execution_directory(directory.path()).unwrap();
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
        let loaded = load_execution_directory(directory.path()).unwrap();
        let mut registry = registry(Arc::new(Mutex::new(Vec::new())));

        let outcome = run_execution_plan(
            &loaded,
            ExecutionRunConfig {
                assume_yes: false,
                resume: false,
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
