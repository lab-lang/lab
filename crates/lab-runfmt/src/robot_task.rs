//! Backend-neutral robot tasks projected from reviewed workcell plans.
//!
//! These documents preserve the semantic intent of one physical handoff.
//! Robot models, controllers, collision geometry, calibrated poses, and
//! randomization belong to a simulator binding, not to this format.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{RunDocumentError, check_format, load_document};

/// The format string every `lab.robot-task.v0` document declares.
pub const ROBOT_TASK_FORMAT: &str = "lab.robot-task.v0";

/// One robot-learning task projected from a workcell-plan node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobotTaskDocument {
    /// Always [`ROBOT_TASK_FORMAT`]; readers reject any other value.
    pub format: String,
    /// The source node's stable identity.
    pub id: String,
    /// The reviewed plan this task was projected from.
    pub plan: String,
    /// The semantic scene whose stable node identities the task uses.
    pub scene: String,
    /// Plan-node identities that must complete before this task begins.
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(flatten)]
    pub action: RobotTaskAction,
}

/// The physical intent a robot policy must accomplish.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum RobotTaskAction {
    /// Transfer one named object between two semantic station endpoints.
    Transfer {
        object: RobotTaskObject,
        source: RobotTaskEndpoint,
        destination: RobotTaskEndpoint,
        instructions: String,
        completion: RobotTaskCompletion,
    },
}

/// A labware object and its stable node in the semantic scene.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobotTaskObject {
    pub labware: String,
    pub scene_node: String,
}

/// A workcell station and its stable node in the semantic scene.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobotTaskEndpoint {
    pub station: String,
    pub scene_node: String,
}

/// A semantic success condition. Simulator bindings turn this relation into
/// measurable position, orientation, contact, and settling tolerances.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobotTaskCompletion {
    pub relation: String,
    pub object: String,
    pub target: String,
}

/// Load and format-check one `lab.robot-task.v0` document.
pub fn load_robot_task(path: &Path) -> Result<RobotTaskDocument, RunDocumentError> {
    let document: RobotTaskDocument = load_document(path)?;
    check_format(path, ROBOT_TASK_FORMAT, &document.format)?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_task() -> RobotTaskDocument {
        RobotTaskDocument {
            format: ROBOT_TASK_FORMAT.to_string(),
            id: "assembly_thermocycle.to-odtc-1".to_string(),
            plan: "plan.workcell.json".to_string(),
            scene: "scene.json".to_string(),
            after: vec!["assembly_run".to_string()],
            action: RobotTaskAction::Transfer {
                object: RobotTaskObject {
                    labware: "reaction_plate".to_string(),
                    scene_node: "reaction_plate".to_string(),
                },
                source: RobotTaskEndpoint {
                    station: "star-1".to_string(),
                    scene_node: "star-1".to_string(),
                },
                destination: RobotTaskEndpoint {
                    station: "odtc-1".to_string(),
                    scene_node: "odtc-1".to_string(),
                },
                instructions: "Seal and transfer the plate.".to_string(),
                completion: RobotTaskCompletion {
                    relation: "object-at-station".to_string(),
                    object: "reaction_plate".to_string(),
                    target: "odtc-1".to_string(),
                },
            },
        }
    }

    #[test]
    fn a_robot_task_round_trips_through_json() {
        let document = transfer_task();
        let text = serde_json::to_string_pretty(&document).expect("the task serializes");
        let back: RobotTaskDocument = serde_json::from_str(&text).expect("the task parses");
        assert_eq!(back, document);
    }

    #[test]
    fn the_loader_rejects_an_unknown_format() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("task.json");
        let mut value = serde_json::to_value(transfer_task()).unwrap();
        value["format"] = serde_json::json!("lab.robot-task.v1");
        std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

        let error = load_robot_task(&path).unwrap_err().to_string();
        assert!(error.contains("expects 'lab.robot-task.v0'"), "{error}");
    }
}
