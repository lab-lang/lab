//! Run-document formats: the schemas shared between the emitters that write
//! executable artifacts and the runner that replays them.
//!
//! Every format here is a reviewed execution boundary: the document is what
//! an operator approves, and the runner adds nothing but ids, timing, and
//! confirmations. Each format is versioned by its `format` string; a change
//! to what a document means is a new format version, not an edit.

use serde::{Deserialize, Serialize};

/// The format string every `lab.star-run.v0` document declares.
pub const STAR_RUN_FORMAT: &str = "lab.star-run.v0";

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
}
