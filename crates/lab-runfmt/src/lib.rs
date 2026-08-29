//! Run-document formats: the schemas shared between the emitters that write
//! executable artifacts and the runners that replay them.
//!
//! Every format here is a reviewed execution boundary: the document is what
//! an operator approves, and a runner adds nothing but ids, timing, and
//! confirmations. Each format is versioned by its `format` string; a change
//! to what a document means is a new format version, not an edit.
//!
//! Every interpreter of these documents loads them through the checked
//! loaders in this crate, so a wrong or missing format string fails the same
//! way everywhere.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

/// The format string every `lab.star-run.v0` document declares.
pub const STAR_RUN_FORMAT: &str = "lab.star-run.v0";

/// The format string every `lab.thermocycle-run.v0` document declares.
pub const THERMOCYCLE_RUN_FORMAT: &str = "lab.thermocycle-run.v0";

/// The format string every `lab.plate-read.v0` document declares.
pub const PLATE_READ_FORMAT: &str = "lab.plate-read.v0";

/// The format string every semantic capability simulation document declares.
pub const SIMULATION_RUN_FORMAT: &str = "lab.simulation-run.v1";

/// The reviewed-file format for a standalone Opentrons Python protocol.
pub const OPENTRONS_PYTHON_PROTOCOL_FORMAT: &str = "opentrons.python-protocol";

/// The reviewed, facility-wide execution plan format.
pub const EXECUTION_PLAN_FORMAT: &str = "lab.execution-plan.v4";

/// The well-known file name for a facility-wide reviewed plan.
pub const EXECUTION_PLAN_FILE: &str = "plan.execution.json";

/// Why a run document failed to load.
#[derive(Debug, thiserror::Error)]
pub enum RunDocumentError {
    #[error("cannot read {path}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a valid document")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} declares format '{found}', but this reader expects '{expected}'")]
    WrongFormat {
        path: String,
        expected: &'static str,
        found: String,
    },
    #[error("{path} is not a valid execution plan: {message}")]
    InvalidPlan { path: String, message: String },
}

fn load_document<T>(path: &Path) -> Result<T, RunDocumentError>
where
    T: serde::de::DeserializeOwned,
{
    let text = std::fs::read_to_string(path).map_err(|source| RunDocumentError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| RunDocumentError::Parse {
        path: path.display().to_string(),
        source,
    })
}

fn check_format(path: &Path, expected: &'static str, found: &str) -> Result<(), RunDocumentError> {
    if found == expected {
        Ok(())
    } else {
        Err(RunDocumentError::WrongFormat {
            path: path.display().to_string(),
            expected,
            found: found.to_string(),
        })
    }
}

/// Load and format-check one `lab.star-run.v0` document.
pub fn load_star_run(path: &Path) -> Result<StarRunDocument, RunDocumentError> {
    let document: StarRunDocument = load_document(path)?;
    check_format(path, STAR_RUN_FORMAT, &document.format)?;
    Ok(document)
}

/// Load and format-check one `lab.thermocycle-run.v0` document.
pub fn load_thermocycle(path: &Path) -> Result<ThermocycleRunDocument, RunDocumentError> {
    let document: ThermocycleRunDocument = load_document(path)?;
    check_format(path, THERMOCYCLE_RUN_FORMAT, &document.format)?;
    Ok(document)
}

/// Load and format-check one `lab.plate-read.v0` document.
pub fn load_plate_read(path: &Path) -> Result<PlateReadDocument, RunDocumentError> {
    let document: PlateReadDocument = load_document(path)?;
    check_format(path, PLATE_READ_FORMAT, &document.format)?;
    Ok(document)
}

/// Load and format-check one `lab.simulation-run.v1` document.
pub fn load_simulation_run(path: &Path) -> Result<SimulationRunDocument, RunDocumentError> {
    let document: SimulationRunDocument = load_document(path)?;
    check_format(path, SIMULATION_RUN_FORMAT, &document.format)?;
    Ok(document)
}

/// Load, format-check, and structurally validate one `lab.execution-plan.v4` document.
pub fn load_execution_plan(path: &Path) -> Result<ExecutionPlanDocument, RunDocumentError> {
    let document: ExecutionPlanDocument = load_document(path)?;
    check_format(path, EXECUTION_PLAN_FORMAT, &document.format)?;
    document
        .validate()
        .map_err(|message| RunDocumentError::InvalidPlan {
            path: path.display().to_string(),
            message,
        })?;
    Ok(document)
}

/// One reviewed facility-wide plan. Runtime interpretation is restricted to these frozen facts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlanDocument {
    /// Always [`EXECUTION_PLAN_FORMAT`].
    pub format: String,
    pub inventory: ExecutionInventoryReference,
    /// Exact compiler decisions and intermediate artifacts, when this plan was compiler-derived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning: Option<ExecutionPlanningReference>,
    pub requirements: Vec<ExecutionRequirementBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<ExecutionMaterialBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ExecutionMaterialOutput>,
    /// Immutable whole-program adapter outputs that implement several semantic requirements
    /// together and therefore cannot be attached honestly to one Execute node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lowerings: Vec<ExecutionLoweringBundle>,
    pub nodes: Vec<ExecutionPlanNode>,
}

impl ExecutionPlanDocument {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != EXECUTION_PLAN_FORMAT {
            return Err(format!(
                "format is '{}', expected '{EXECUTION_PLAN_FORMAT}'",
                self.format
            ));
        }
        require_sha256("inventory source", &self.inventory.source_sha256)?;
        require_relative_path("inventory source", &self.inventory.document)?;
        if let Some(planning) = &self.planning {
            planning.validate()?;
        }

        let mut requirements = BTreeMap::new();
        for requirement in &self.requirements {
            if requirement.requirement_instance.is_empty() {
                return Err("a requirement binding has an empty instance ID".to_owned());
            }
            if requirements
                .insert(requirement.requirement_instance.as_str(), requirement)
                .is_some()
            {
                return Err(format!(
                    "requirement instance '{}' is bound more than once",
                    requirement.requirement_instance
                ));
            }
            if let Some(adapter) = &requirement.adapter {
                require_sha256(
                    &format!("adapter profile for '{}'", requirement.requirement_instance),
                    &adapter.profile_sha256,
                )?;
                require_relative_path("adapter profile", &adapter.profile_path)?;
            }
        }

        let mut materials = BTreeSet::new();
        for material in &self.materials {
            if material.id.is_empty() || !materials.insert(material.id.as_str()) {
                return Err(format!(
                    "material binding ID '{}' is empty or repeated",
                    material.id
                ));
            }
        }
        let mut material_lots = self
            .materials
            .iter()
            .map(|material| material.material_lot.as_str())
            .collect::<BTreeSet<_>>();
        for output in &self.outputs {
            if output.id.is_empty() || !materials.insert(output.id.as_str()) {
                return Err(format!(
                    "output material binding ID '{}' is empty or repeated",
                    output.id
                ));
            }
            if output.namespace.ends_with('/')
                || output.namespace.is_empty()
                || output.display_id.is_empty()
                || output.material_lot != format!("{}/{}", output.namespace, output.display_id)
            {
                return Err(format!(
                    "output material '{}' identity must equal namespace/display_id",
                    output.id
                ));
            }
            if !material_lots.insert(output.material_lot.as_str()) {
                return Err(format!(
                    "material lot IRI '{}' is bound more than once",
                    output.material_lot
                ));
            }
            for source in &output.derived_from {
                if !self.materials.iter().any(|material| material.id == *source) {
                    return Err(format!(
                        "output material '{}' derives from unknown input material '{}'",
                        output.id, source
                    ));
                }
            }
        }

        let mut lowering_ids = BTreeSet::new();
        let mut lowered_requirements = BTreeSet::new();
        let mut lowering_artifact_paths = BTreeSet::new();
        for lowering in &self.lowerings {
            if lowering.id.is_empty() || !lowering_ids.insert(lowering.id.as_str()) {
                return Err(format!(
                    "adapter lowering ID '{}' is empty or repeated",
                    lowering.id
                ));
            }
            if lowering.asset.is_empty() {
                return Err(format!(
                    "adapter lowering '{}' has an empty Asset IRI",
                    lowering.id
                ));
            }
            require_relative_path("adapter lowering profile", &lowering.adapter.profile_path)?;
            require_sha256(
                &format!("adapter lowering profile for '{}'", lowering.id),
                &lowering.adapter.profile_sha256,
            )?;
            if lowering.requirements.is_empty() {
                return Err(format!(
                    "adapter lowering '{}' does not identify any triggering requirements",
                    lowering.id
                ));
            }
            let mut route_requirements = BTreeSet::new();
            for requirement_id in &lowering.requirements {
                if !route_requirements.insert(requirement_id.as_str()) {
                    return Err(format!(
                        "adapter lowering '{}' repeats requirement '{}'",
                        lowering.id, requirement_id
                    ));
                }
                if !lowered_requirements.insert(requirement_id.as_str()) {
                    return Err(format!(
                        "requirement '{}' belongs to more than one adapter lowering",
                        requirement_id
                    ));
                }
                let requirement = requirements.get(requirement_id.as_str()).ok_or_else(|| {
                    format!(
                        "adapter lowering '{}' references unknown requirement '{}'",
                        lowering.id, requirement_id
                    )
                })?;
                if requirement.asset != lowering.asset {
                    return Err(format!(
                        "adapter lowering '{}' binds Asset '{}', but requirement '{}' binds '{}'",
                        lowering.id, lowering.asset, requirement_id, requirement.asset
                    ));
                }
                if requirement.adapter.as_ref() != Some(&lowering.adapter) {
                    return Err(format!(
                        "adapter lowering '{}' does not match the frozen adapter for requirement '{}'",
                        lowering.id, requirement_id
                    ));
                }
            }
            if lowering.artifacts.is_empty() {
                return Err(format!(
                    "adapter lowering '{}' has no reviewed artifacts",
                    lowering.id
                ));
            }
            let mut device_protocols = 0;
            for artifact in &lowering.artifacts {
                require_relative_path("reviewed lowering artifact", &artifact.path)?;
                require_sha256(
                    &format!(
                        "reviewed lowering artifact '{}' in '{}'",
                        artifact.path, lowering.id
                    ),
                    &artifact.sha256,
                )?;
                if artifact.media_type.is_empty() {
                    return Err(format!(
                        "reviewed lowering artifact '{}' in '{}' has no media type",
                        artifact.path, lowering.id
                    ));
                }
                if !lowering_artifact_paths.insert(artifact.path.as_str()) {
                    return Err(format!(
                        "reviewed lowering artifact path '{}' is repeated",
                        artifact.path
                    ));
                }
                match artifact.role {
                    ReviewedLoweringArtifactRole::DeviceProtocol => {
                        device_protocols += 1;
                        if artifact.format.as_deref().is_none_or(str::is_empty) {
                            return Err(format!(
                                "device protocol '{}' in '{}' has no run-document format",
                                artifact.path, lowering.id
                            ));
                        }
                    }
                    ReviewedLoweringArtifactRole::OperatorDocument
                    | ReviewedLoweringArtifactRole::Support => {
                        if artifact.format.is_some() {
                            return Err(format!(
                                "non-protocol artifact '{}' in '{}' declares a run-document format",
                                artifact.path, lowering.id
                            ));
                        }
                    }
                }
            }
            if device_protocols == 0 {
                return Err(format!(
                    "adapter lowering '{}' has no reviewed device protocol",
                    lowering.id
                ));
            }
        }

        let mut nodes = BTreeMap::new();
        let mut scheduled_requirements = BTreeSet::new();
        for node in &self.nodes {
            if node.id.is_empty() || nodes.insert(node.id.as_str(), node).is_some() {
                return Err(format!("node ID '{}' is empty or repeated", node.id));
            }
        }
        for node in &self.nodes {
            let mut dependencies = BTreeSet::new();
            for dependency in &node.after {
                if !dependencies.insert(dependency) {
                    return Err(format!(
                        "node '{}' repeats dependency '{}'",
                        node.id, dependency
                    ));
                }
                if dependency == &node.id {
                    return Err(format!("node '{}' depends on itself", node.id));
                }
                if !nodes.contains_key(dependency.as_str()) {
                    return Err(format!(
                        "node '{}' depends on unknown node '{}'",
                        node.id, dependency
                    ));
                }
            }
            match &node.action {
                ExecutionPlanAction::Execute {
                    requirement,
                    document,
                } => {
                    let binding = requirements.get(requirement.as_str()).ok_or_else(|| {
                        format!(
                            "execute node '{}' references unknown requirement '{}'",
                            node.id, requirement
                        )
                    })?;
                    if binding.control_mode == lab_capability::ControlMode::Manual.iri() {
                        return Err(format!(
                            "execute node '{}' represents manual-control requirement '{}'; use a manual node",
                            node.id, requirement
                        ));
                    }
                    if !scheduled_requirements.insert(requirement.as_str()) {
                        return Err(format!(
                            "requirement '{}' is scheduled by more than one execution node",
                            requirement
                        ));
                    }
                    if let Some(document) = document {
                        if lowered_requirements.contains(requirement.as_str()) {
                            return Err(format!(
                                "execute node '{}' attaches a single run document to requirement '{}', which already belongs to whole-program adapter lowering",
                                node.id, requirement
                            ));
                        }
                        require_relative_path("reviewed run document", &document.path)?;
                        require_sha256(
                            &format!("reviewed run document for node '{}'", node.id),
                            &document.sha256,
                        )?;
                        if document.format.is_empty() {
                            return Err(format!(
                                "reviewed run document for node '{}' has no format",
                                node.id
                            ));
                        }
                    }
                }
                ExecutionPlanAction::MoveMaterial { material, .. } => {
                    if !materials.contains(material.as_str()) {
                        return Err(format!(
                            "material-movement node '{}' references unknown material binding '{}'",
                            node.id, material
                        ));
                    }
                }
                ExecutionPlanAction::Manual {
                    requirement,
                    title,
                    instructions,
                } => {
                    let binding = requirements.get(requirement.as_str()).ok_or_else(|| {
                        format!(
                            "manual node '{}' references unknown requirement '{}'",
                            node.id, requirement
                        )
                    })?;
                    if binding.control_mode != lab_capability::ControlMode::Manual.iri() {
                        return Err(format!(
                            "manual node '{}' references requirement '{}' with non-manual control mode '{}'",
                            node.id, requirement, binding.control_mode
                        ));
                    }
                    if binding.adapter.is_some() {
                        return Err(format!(
                            "manual node '{}' references requirement '{}' with a runtime adapter",
                            node.id, requirement
                        ));
                    }
                    if lowered_requirements.contains(requirement.as_str()) {
                        return Err(format!(
                            "manual requirement '{}' also belongs to a whole-program adapter lowering",
                            requirement
                        ));
                    }
                    if !scheduled_requirements.insert(requirement.as_str()) {
                        return Err(format!(
                            "requirement '{}' is scheduled by more than one execution node",
                            requirement
                        ));
                    }
                    if title.trim().is_empty() {
                        return Err(format!("manual node '{}' has an empty title", node.id));
                    }
                    if instructions.trim().is_empty() {
                        return Err(format!("manual node '{}' has empty instructions", node.id));
                    }
                }
            }
        }
        if let Some(requirement) = requirements
            .keys()
            .find(|requirement| !scheduled_requirements.contains(**requirement))
        {
            return Err(format!(
                "requirement '{}' is not scheduled by an execution node",
                requirement
            ));
        }
        validate_acyclic(&nodes)
    }
}

/// Exact compiler provenance frozen into a reviewed execution plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlanningReference {
    pub problem_sha256: String,
    pub allocated_lair_sha256: String,
    pub planning_problem: ExecutionPlanningArtifact,
    pub facility_solution: ExecutionPlanningArtifact,
    pub allocated_lair: ExecutionPlanningArtifact,
    pub adapter_invocations: ExecutionPlanningArtifact,
    pub methods: Vec<ExecutionMethodSelection>,
}

impl ExecutionPlanningReference {
    pub fn artifacts(&self) -> [(&'static str, &ExecutionPlanningArtifact); 4] {
        [
            ("planning problem", &self.planning_problem),
            ("facility solution", &self.facility_solution),
            ("allocated LAIR", &self.allocated_lair),
            ("adapter invocations", &self.adapter_invocations),
        ]
    }

    fn validate(&self) -> Result<(), String> {
        require_sha256("planning problem", &self.problem_sha256)?;
        require_sha256("allocated LAIR", &self.allocated_lair_sha256)?;
        for (label, artifact) in self.artifacts() {
            require_relative_path(label, &artifact.path)?;
            require_sha256(label, &artifact.sha256)?;
        }
        if self.allocated_lair.sha256 != self.allocated_lair_sha256 {
            return Err(
                "allocated LAIR artifact digest must equal the selected LAIR digest".to_owned(),
            );
        }
        if self.methods.is_empty() {
            return Err("compiler-derived planning contains no selected Methods".to_owned());
        }
        let mut choices = BTreeSet::new();
        for method in &self.methods {
            if method.choice.is_empty()
                || method.source_operation.is_empty()
                || method.method.is_empty()
                || method.tasks.is_empty()
                || !choices.insert(method.choice.as_str())
            {
                return Err(format!(
                    "selected Method choice '{}' is empty, repeated, or incomplete",
                    method.choice
                ));
            }
        }
        Ok(())
    }
}

/// One immutable compiler artifact staged next to the reviewed plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlanningArtifact {
    pub path: String,
    pub sha256: String,
}

/// One Method decision from the global facility solution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMethodSelection {
    pub choice: String,
    pub source_operation: String,
    pub method: String,
    pub tasks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionInventoryReference {
    /// Exact source graph copied into the reviewed execution package.
    pub document: String,
    pub source_sha256: String,
    pub facility: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRequirementBinding {
    pub requirement_instance: String,
    pub requirement_template: String,
    pub capability_kind: String,
    pub offering: String,
    pub asset: String,
    pub minimum_qualification: String,
    pub observed_qualification: String,
    pub control_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ExecutionParameterBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<ExecutionAdapterBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionParameterBinding {
    pub argument: String,
    pub property_kind: String,
    pub relation: String,
    #[serde(flatten)]
    pub required: ExecutionParameterValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_unit: Option<String>,
    pub offering_parameter: String,
    pub observed: ExecutionParameterValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "value_type", content = "value", rename_all = "snake_case")]
pub enum ExecutionParameterValue {
    Text(String),
    Integer(String),
    Real(String),
    Boolean(bool),
    Iri(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionAdapterBinding {
    pub driver: String,
    pub profile_path: String,
    pub profile_sha256: String,
}

/// One immutable adapter invocation whose device artifacts jointly realize several requirements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLoweringBundle {
    pub id: String,
    pub asset: String,
    pub adapter: ExecutionAdapterBinding,
    pub requirements: Vec<String>,
    pub artifacts: Vec<ReviewedLoweringArtifact>,
}

/// One hash-addressed child of a reviewed whole-program adapter lowering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedLoweringArtifact {
    pub path: String,
    pub media_type: String,
    pub sha256: String,
    pub role: ReviewedLoweringArtifactRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedLoweringArtifactRole {
    DeviceProtocol,
    OperatorDocument,
    Support,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMaterialBinding {
    pub id: String,
    pub component: String,
    pub material_lot: String,
}

/// One new MaterialLot whose exact identity and lineage are frozen before execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMaterialOutput {
    pub id: String,
    pub material_lot: String,
    pub namespace: String,
    pub display_id: String,
    pub component: String,
    pub material_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub located_in: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlanNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    #[serde(flatten)]
    pub action: ExecutionPlanAction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ExecutionPlanAction {
    Execute {
        requirement: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document: Option<ReviewedRunDocument>,
    },
    MoveMaterial {
        material: String,
        from: String,
        to: String,
        instructions: String,
    },
    Manual {
        requirement: String,
        title: String,
        instructions: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedRunDocument {
    pub path: String,
    pub format: String,
    pub sha256: String,
}

fn require_sha256(label: &str, value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("{label} SHA-256 must be 64 hexadecimal characters"))
    }
}

fn require_relative_path(label: &str, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    let invalid = value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if invalid {
        Err(format!(
            "{label} path '{value}' must be a non-empty relative path without '..'"
        ))
    } else {
        Ok(())
    }
}

fn validate_acyclic(nodes: &BTreeMap<&str, &ExecutionPlanNode>) -> Result<(), String> {
    let mut indegree = nodes
        .iter()
        .map(|(id, node)| (*id, node.after.len()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    for (id, node) in nodes {
        for dependency in &node.after {
            dependents.entry(dependency).or_default().push(id);
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        for dependent in dependents.get(id).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("every dependent is a declared node");
            *degree -= 1;
            if *degree == 0 {
                ready.push_back(dependent);
            }
        }
    }
    if visited == nodes.len() {
        Ok(())
    } else {
        let cyclic = indegree
            .into_iter()
            .filter_map(|(id, degree)| (degree > 0).then_some(id))
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "execution plan contains a dependency cycle among {cyclic}"
        ))
    }
}

/// One `lab.thermocycle-run.v0` document: a device-neutral thermal program
/// for one plate. The exact Asset and adapter binding selects the executor;
/// the document never names a vendor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThermocycleRunDocument {
    /// Always [`THERMOCYCLE_RUN_FORMAT`]; readers reject any other value.
    pub format: String,
    /// The program's identity within its wave, e.g. `assembly_thermocycle`.
    pub id: String,
    pub title: String,
    /// The labware resource that rides through the program.
    pub plate: String,
    pub profile: lab_instruments::ThermalProfile,
    /// Temperature held after the profile ends, until retrieval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_hold_celsius: Option<f64>,
    /// Approximate per-well fill, for volume-dependent control classes.
    pub fill_volume_ul: f64,
}

/// One `lab.plate-read.v0` document: a device-neutral plate acquisition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlateReadDocument {
    /// Always [`PLATE_READ_FORMAT`]; readers reject any other value.
    pub format: String,
    pub id: String,
    pub title: String,
    /// The labware resource being measured.
    pub plate: String,
    pub mode: PlateReadMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum PlateReadMode {
    Absorbance { wavelength_nm: u16 },
    Luminescence { integration_seconds: f64 },
}

/// One reviewed semantic simulation step.
///
/// This document records what a simulator is asked to model. It is never a hardware protocol and
/// never implies that a physical Asset has a compatible control path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationRunDocument {
    /// Always [`SIMULATION_RUN_FORMAT`].
    pub format: String,
    pub id: String,
    pub title: String,
    /// Exact capability-kind IRI this simulation models.
    pub capability_kind: String,
    /// Human-readable scope or assumptions reviewed with the simulation.
    pub assumptions: Vec<String>,
}

/// One replayable Hamilton STAR step: the id-less firmware frame and the
/// operator's view of it. `module` and `code` repeat the frame's first four
/// characters so a reviewer can scan the document without decoding frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStep {
    pub frame: String,
    pub module: String,
    pub code: String,
    pub description: String,
}

/// A step the operator performs by hand between machine runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualStep {
    pub title: String,
    pub instructions: String,
}

/// One `lab.star-run.v0` document: an ordered list of reviewed firmware
/// frames for a single machine session, with the manual steps that follow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarRunDocument {
    /// Always [`STAR_RUN_FORMAT`]; readers reject any other value.
    pub format: String,
    /// The run's identity within its build, e.g. `assembly_run`.
    pub run: String,
    pub title: String,
    /// The machine variant name the plan targeted.
    pub machine: String,
    /// The channel count the frames were encoded for.
    pub channels: usize,
    pub steps: Vec<RunStep>,
    #[serde(default)]
    pub manual_after: Vec<ManualStep>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execution_plan() -> ExecutionPlanDocument {
        ExecutionPlanDocument {
            format: EXECUTION_PLAN_FORMAT.to_owned(),
            inventory: ExecutionInventoryReference {
                document: "inventory-source.ttl".to_owned(),
                source_sha256: "a".repeat(64),
                facility: "https://example.org/facility".to_owned(),
            },
            planning: None,
            requirements: vec![ExecutionRequirementBinding {
                requirement_instance: "example::main/body[0]".to_owned(),
                requirement_template: "example::main::body[0]".to_owned(),
                capability_kind: "https://sbol.io/ns/capability#Incubation".to_owned(),
                offering: "https://example.org/incubator/incubation".to_owned(),
                asset: "https://example.org/incubator".to_owned(),
                minimum_qualification: "https://sbol.io/ns/facility#Plannable".to_owned(),
                observed_qualification: "https://sbol.io/ns/facility#Executable".to_owned(),
                control_mode: "https://sbol.io/ns/facility#ReviewedFileControl".to_owned(),
                parameters: Vec::new(),
                adapter: Some(ExecutionAdapterBinding {
                    driver: "example.incubator".to_owned(),
                    profile_path: "adapters/incubator.toml".to_owned(),
                    profile_sha256: "b".repeat(64),
                }),
            }],
            materials: Vec::new(),
            outputs: Vec::new(),
            lowerings: Vec::new(),
            nodes: vec![ExecutionPlanNode {
                id: "execute-0001".to_owned(),
                after: Vec::new(),
                action: ExecutionPlanAction::Execute {
                    requirement: "example::main/body[0]".to_owned(),
                    document: None,
                },
            }],
        }
    }

    fn planning_reference() -> ExecutionPlanningReference {
        ExecutionPlanningReference {
            problem_sha256: "c".repeat(64),
            allocated_lair_sha256: "d".repeat(64),
            planning_problem: ExecutionPlanningArtifact {
                path: "compiler/planning-problem.json".to_owned(),
                sha256: "c".repeat(64),
            },
            facility_solution: ExecutionPlanningArtifact {
                path: "compiler/facility-solution.json".to_owned(),
                sha256: "e".repeat(64),
            },
            allocated_lair: ExecutionPlanningArtifact {
                path: "compiler/allocated.lair".to_owned(),
                sha256: "d".repeat(64),
            },
            adapter_invocations: ExecutionPlanningArtifact {
                path: "compiler/adapter-invocations.json".to_owned(),
                sha256: "f".repeat(64),
            },
            methods: vec![ExecutionMethodSelection {
                choice: "main::body[0]".to_owned(),
                source_operation: "std.bio.build.realize".to_owned(),
                method: "https://example.org/method#automated".to_owned(),
                tasks: vec!["main::body[0]::setup".to_owned()],
            }],
        }
    }

    #[test]
    fn an_execution_plan_round_trips_and_validates() {
        let plan = execution_plan();
        plan.validate().unwrap();
        let text = serde_json::to_string_pretty(&plan).unwrap();
        assert_eq!(
            serde_json::from_str::<ExecutionPlanDocument>(&text).unwrap(),
            plan
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(EXECUTION_PLAN_FILE);
        std::fs::write(&path, text).unwrap();
        assert_eq!(load_execution_plan(&path).unwrap(), plan);
    }

    #[test]
    fn compiler_derived_plans_freeze_method_and_intermediate_artifact_identity() {
        let mut plan = execution_plan();
        plan.planning = Some(planning_reference());
        plan.validate().unwrap();

        let mut changed = plan;
        changed.planning.as_mut().unwrap().allocated_lair.sha256 = "0".repeat(64);
        assert!(
            changed
                .validate()
                .unwrap_err()
                .contains("allocated LAIR artifact digest")
        );
    }

    #[test]
    fn execution_plan_validation_rejects_dangling_and_cyclic_dependencies() {
        let mut dangling = execution_plan();
        dangling.nodes[0].after.push("missing".to_owned());
        assert!(
            dangling
                .validate()
                .unwrap_err()
                .contains("depends on unknown node")
        );

        let mut cyclic = execution_plan();
        let mut manual = cyclic.requirements[0].clone();
        manual.requirement_instance = "example::main/body[1]".to_owned();
        manual.requirement_template = "example::main::body[1]".to_owned();
        manual.control_mode = lab_capability::ControlMode::Manual.iri().to_owned();
        manual.adapter = None;
        cyclic.requirements.push(manual);
        cyclic.nodes.push(ExecutionPlanNode {
            id: "manual-0002".to_owned(),
            after: vec!["execute-0001".to_owned()],
            action: ExecutionPlanAction::Manual {
                requirement: "example::main/body[1]".to_owned(),
                title: "inspect".to_owned(),
                instructions: "confirm".to_owned(),
            },
        });
        cyclic.nodes[0].after.push("manual-0002".to_owned());
        assert!(cyclic.validate().unwrap_err().contains("dependency cycle"));
    }

    #[test]
    fn execution_plan_validation_checks_exact_references_and_digests() {
        let mut plan = execution_plan();
        let ExecutionPlanAction::Execute { requirement, .. } = &mut plan.nodes[0].action else {
            unreachable!()
        };
        *requirement = "missing".to_owned();
        assert!(plan.validate().unwrap_err().contains("unknown requirement"));

        let mut plan = execution_plan();
        plan.inventory.source_sha256 = "not-a-digest".to_owned();
        assert!(plan.validate().unwrap_err().contains("SHA-256"));
    }

    #[test]
    fn execution_plan_validation_freezes_whole_program_adapter_lowerings() {
        let mut plan = execution_plan();
        let adapter = plan.requirements[0].adapter.clone().unwrap();
        plan.lowerings.push(ExecutionLoweringBundle {
            id: "example-incubator-a1b2c3d4e5f6".to_owned(),
            asset: "https://example.org/incubator".to_owned(),
            adapter,
            requirements: vec!["example::main/body[0]".to_owned()],
            artifacts: vec![ReviewedLoweringArtifact {
                path: "lowerings/incubator/run.json".to_owned(),
                media_type: "application/json".to_owned(),
                sha256: "c".repeat(64),
                role: ReviewedLoweringArtifactRole::DeviceProtocol,
                format: Some("example.incubator-run.v1".to_owned()),
            }],
        });
        plan.validate().unwrap();

        let mut wrong_asset = plan.clone();
        wrong_asset.lowerings[0].asset = "https://example.org/other".to_owned();
        assert!(
            wrong_asset
                .validate()
                .unwrap_err()
                .contains("but requirement")
        );

        let mut missing_format = plan;
        missing_format.lowerings[0].artifacts[0].format = None;
        assert!(
            missing_format
                .validate()
                .unwrap_err()
                .contains("no run-document format")
        );
    }

    #[test]
    fn execution_plan_validation_freezes_output_material_identity_and_lineage() {
        let mut plan = execution_plan();
        plan.materials.push(ExecutionMaterialBinding {
            id: "input".to_owned(),
            component: "https://example.org/design".to_owned(),
            material_lot: "https://example.org/input".to_owned(),
        });
        plan.outputs.push(ExecutionMaterialOutput {
            id: "output".to_owned(),
            material_lot: "https://example.org/results/output".to_owned(),
            namespace: "https://example.org/results".to_owned(),
            display_id: "output".to_owned(),
            component: "https://example.org/design".to_owned(),
            material_kind: "https://sbol.io/ns/inventory#DnaSample".to_owned(),
            located_in: None,
            position: None,
            derived_from: vec!["input".to_owned()],
        });
        plan.validate().unwrap();

        let mut wrong_identity = plan.clone();
        wrong_identity.outputs[0].material_lot = "https://example.org/results/other".to_owned();
        assert!(
            wrong_identity
                .validate()
                .unwrap_err()
                .contains("namespace/display_id")
        );

        let mut unknown_source = plan;
        unknown_source.outputs[0].derived_from = vec!["missing".to_owned()];
        assert!(
            unknown_source
                .validate()
                .unwrap_err()
                .contains("unknown input material")
        );
    }

    #[test]
    fn a_star_run_document_round_trips_through_json() {
        let document = StarRunDocument {
            format: STAR_RUN_FORMAT.to_string(),
            run: "assembly_run".to_string(),
            title: "Golden Gate assembly".to_string(),
            machine: "STARlet".to_string(),
            channels: 8,
            steps: vec![RunStep {
                frame: "C0ZA".to_string(),
                module: "C0".to_string(),
                code: "ZA".to_string(),
                description: "retract all channels to Z-safety".to_string(),
            }],
            manual_after: vec![ManualStep {
                title: "thermocycle".to_string(),
                instructions: "move the reaction plate to the cycler".to_string(),
            }],
        };
        let text = serde_json::to_string_pretty(&document).expect("the document serializes");
        let back: StarRunDocument = serde_json::from_str(&text).expect("the document parses");
        assert_eq!(back, document, "emitter and runner read the same schema");
    }

    #[test]
    fn a_capability_simulation_document_round_trips() {
        let document = SimulationRunDocument {
            format: SIMULATION_RUN_FORMAT.to_owned(),
            id: "growth".to_owned(),
            title: "Simulate plate growth".to_owned(),
            capability_kind: "https://sbol.io/ns/capability#Incubation".to_owned(),
            assumptions: vec!["No physical hardware is contacted.".to_owned()],
        };
        let text = serde_json::to_string_pretty(&document).unwrap();
        assert_eq!(
            serde_json::from_str::<SimulationRunDocument>(&text).unwrap(),
            document
        );
    }

    #[test]
    fn a_document_without_manual_steps_parses_with_an_empty_list() {
        let text = r#"{
            "format": "lab.star-run.v0",
            "run": "r", "title": "t", "machine": "STAR", "channels": 8,
            "steps": []
        }"#;
        let document: StarRunDocument = serde_json::from_str(text).expect("manual_after defaults");
        assert!(
            document.manual_after.is_empty(),
            "absent manual steps mean none"
        );
    }

    #[test]
    fn a_loader_rejects_a_document_with_the_wrong_format() {
        let directory = tempfile::tempdir().expect("the test directory is creatable");
        let path = directory.path().join("wrong.star.json");
        std::fs::write(
            &path,
            r#"{ "format": "lab.star-run.v99", "run": "r", "title": "t",
                 "machine": "STAR", "channels": 8, "steps": [] }"#,
        )
        .expect("the fixture writes");
        let error = load_star_run(&path).expect_err("a wrong format string is rejected");
        assert!(
            matches!(error, RunDocumentError::WrongFormat { expected, .. } if expected == STAR_RUN_FORMAT),
            "the error names the expected format: {error}"
        );
    }
}
