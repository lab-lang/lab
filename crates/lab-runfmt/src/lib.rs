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

/// The format string every `lab.workcell-run.v0` document declares.
pub const WORKCELL_RUN_FORMAT: &str = "lab.workcell-run.v0";

/// The file name a wave directory's coordination plan is stored under.
pub const WORKCELL_PLAN_FILE: &str = "plan.workcell.json";

/// The reviewed, facility-wide execution plan format.
pub const EXECUTION_PLAN_FORMAT: &str = "lab.execution-plan.v1";

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

/// Load and format-check the coordination plan in a wave directory.
pub fn load_workcell_plan(directory: &Path) -> Result<WorkcellRunDocument, RunDocumentError> {
    let path = directory.join(WORKCELL_PLAN_FILE);
    let document: WorkcellRunDocument = load_document(&path)?;
    check_format(&path, WORKCELL_RUN_FORMAT, &document.format)?;
    Ok(document)
}

/// Load, format-check, and structurally validate one `lab.execution-plan.v1` document.
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
    pub requirements: Vec<ExecutionRequirementBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<ExecutionMaterialBinding>,
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

        let mut nodes = BTreeMap::new();
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
                    if !requirements.contains_key(requirement.as_str()) {
                        return Err(format!(
                            "execute node '{}' references unknown requirement '{}'",
                            node.id, requirement
                        ));
                    }
                    if let Some(document) = document {
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
                ExecutionPlanAction::Manual { title, .. } if title.is_empty() => {
                    return Err(format!("manual node '{}' has an empty title", node.id));
                }
                ExecutionPlanAction::Manual { .. } => {}
            }
        }
        validate_acyclic(&nodes)
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMaterialBinding {
    pub id: String,
    pub component: String,
    pub material_lot: String,
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
/// for one plate. The station's kind decides which instrument executes it;
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

/// One `lab.workcell-run.v0` document: the coordination plan for one wave
/// of a multi-station build. Nodes execute in dependency order; every
/// physical plate movement is an explicit handoff node the operator
/// confirms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkcellRunDocument {
    /// Always [`WORKCELL_RUN_FORMAT`]; readers reject any other value.
    pub format: String,
    pub stations: Vec<WorkcellStation>,
    pub nodes: Vec<WorkcellNode>,
}

/// One station as the coordination plan sees it: a name, the kind that
/// selects its executor, and where its program documents live relative to
/// the wave directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkcellStation {
    pub name: String,
    /// The station kind string, e.g. `hamilton.star` or `inheco.odtc`.
    pub kind: String,
    pub program_dir: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkcellNode {
    /// Stable, human-readable identity, e.g. `assembly_run` or
    /// `assembly_thermocycle.to-odtc-1`.
    pub id: String,
    /// Node ids that must complete first.
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(flatten)]
    pub action: WorkcellAction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum WorkcellAction {
    /// Execute one station program document.
    StationProgram {
        station: String,
        /// The document path relative to the wave directory.
        document: String,
    },
    /// A human moves labware between stations and confirms.
    Handoff {
        from: String,
        to: String,
        labware: String,
        instructions: String,
    },
    /// A human performs a step that is not a movement, and confirms.
    Manual { title: String, instructions: String },
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
            requirements: vec![ExecutionRequirementBinding {
                requirement_instance: "example::main/body[0]".to_owned(),
                requirement_template: "example::main::body[0]".to_owned(),
                capability_kind: "https://draggon.org/ns/capability#Incubation".to_owned(),
                offering: "https://example.org/incubator/incubation".to_owned(),
                asset: "https://example.org/incubator".to_owned(),
                minimum_qualification: "https://draggon.org/ns/facility#Plannable".to_owned(),
                observed_qualification: "https://draggon.org/ns/facility#Executable".to_owned(),
                control_mode: "https://draggon.org/ns/facility#ReviewedFileControl".to_owned(),
                parameters: Vec::new(),
                adapter: Some(ExecutionAdapterBinding {
                    driver: "example.incubator".to_owned(),
                    profile_path: "adapters/incubator.toml".to_owned(),
                    profile_sha256: "b".repeat(64),
                }),
            }],
            materials: Vec::new(),
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
        cyclic.nodes.push(ExecutionPlanNode {
            id: "execute-0002".to_owned(),
            after: vec!["execute-0001".to_owned()],
            action: ExecutionPlanAction::Manual {
                title: "inspect".to_owned(),
                instructions: "confirm".to_owned(),
            },
        });
        cyclic.nodes[0].after.push("execute-0002".to_owned());
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

    #[test]
    fn the_workcell_plan_loader_reads_from_its_well_known_file_name() {
        let directory = tempfile::tempdir().expect("the test directory is creatable");
        std::fs::write(
            directory.path().join(WORKCELL_PLAN_FILE),
            r#"{ "format": "lab.workcell-run.v0", "stations": [], "nodes": [] }"#,
        )
        .expect("the fixture writes");
        let plan = load_workcell_plan(directory.path()).expect("the plan loads");
        assert!(plan.nodes.is_empty(), "the empty plan round-trips");
    }
}
