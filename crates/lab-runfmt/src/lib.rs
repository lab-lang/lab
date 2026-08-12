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

pub mod facility;
mod trace;

pub use trace::{
    AttentionWindow, ProgramExtent, RunEvent, SIM_TRACE_FORMAT, SimSummary, SimTraceDocument,
    StationSummary, TimedEvent, summarize,
};

use std::path::Path;

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
