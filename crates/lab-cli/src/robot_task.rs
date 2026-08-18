//! Projection of one reviewed workcell handoff into `lab.robot-task.v0`.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_runfmt::{
    ROBOT_TASK_FORMAT, RobotTaskAction, RobotTaskCompletion, RobotTaskDocument, RobotTaskEndpoint,
    RobotTaskObject, WORKCELL_PLAN_FILE, WorkcellAction,
};
use lab_scene::{Scene, Semantic};
use serde::Serialize;

use crate::Output;

#[derive(Serialize)]
struct RobotTaskReport {
    id: String,
    object: String,
    source: String,
    destination: String,
    task: String,
}

fn safe_file_stem(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn document_reference(path: &Path, artifact_directory: &Path) -> String {
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let base = artifact_directory
        .canonicalize()
        .unwrap_or_else(|_| artifact_directory.to_path_buf());
    let target_components: Vec<Component<'_>> = target.components().collect();
    let base_components: Vec<Component<'_>> = base.components().collect();
    let common = target_components
        .iter()
        .zip(&base_components)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return target.to_string_lossy().into_owned();
    }
    let mut reference = PathBuf::new();
    for _ in common..base_components.len() {
        reference.push("..");
    }
    for component in &target_components[common..] {
        reference.push(component.as_os_str());
    }
    if reference.as_os_str().is_empty() {
        ".".to_string()
    } else {
        reference.to_string_lossy().into_owned()
    }
}

fn load_scene(path: &Path) -> Result<Scene> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "no semantic scene at {}; run `lab scene` on this wave first",
            path.display()
        )
    })?;
    let scene: Scene = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if scene.format != lab_scene::scene::SCENE_FORMAT {
        bail!(
            "{} declares format '{}'; this reader expects '{}'",
            path.display(),
            scene.format,
            lab_scene::scene::SCENE_FORMAT
        );
    }
    Ok(scene)
}

fn validate_scene_node(
    scene: &Scene,
    id: &str,
    expected: &str,
    accepts: impl Fn(&Semantic) -> bool,
) -> Result<()> {
    let mut identities = 0usize;
    let mut matching = 0usize;
    scene.root.walk(&mut |node, _| {
        if node.id == id {
            identities += 1;
            if accepts(&node.semantic) {
                matching += 1;
            }
        }
    });
    match (identities, matching) {
        (0, _) => bail!("scene has no node '{id}' for the task's {expected}"),
        (_, 0) => bail!("scene node '{id}' is not a {expected}"),
        (_, 1) => Ok(()),
        (_, count) => {
            bail!("scene has {count} {expected} nodes named '{id}'; task identities must be unique")
        }
    }
}

pub(crate) fn robot_task(
    directory: PathBuf,
    node_id: String,
    scene_path: Option<PathBuf>,
    out_path: Option<PathBuf>,
    output: &Output,
) -> Result<()> {
    if !lab_runtime::workcell::is_workcell_directory(&directory) {
        bail!(
            "{} is not a workcell wave: no {}",
            directory.display(),
            WORKCELL_PLAN_FILE
        );
    }
    let plan = lab_runfmt::load_workcell_plan(&directory)?;
    let mut matching_nodes = plan.nodes.iter().filter(|node| node.id == node_id);
    let node = matching_nodes
        .next()
        .with_context(|| format!("workcell plan has no node '{node_id}'"))?;
    if matching_nodes.next().is_some() {
        bail!("workcell plan has more than one node named '{node_id}'");
    }
    let WorkcellAction::Handoff {
        from,
        to,
        labware,
        instructions,
    } = &node.action
    else {
        bail!(
            "workcell node '{}' is not a handoff and cannot become a robot transfer task",
            node.id
        );
    };

    let scene_path = scene_path.unwrap_or_else(|| directory.join("scene.json"));
    let scene = load_scene(&scene_path)?;
    validate_scene_node(&scene, from, "station", |semantic| {
        matches!(semantic, Semantic::Station { .. })
    })?;
    validate_scene_node(&scene, to, "station", |semantic| {
        matches!(semantic, Semantic::Station { .. })
    })?;
    validate_scene_node(&scene, labware, "labware object", |semantic| {
        matches!(semantic, Semantic::Labware { .. })
    })?;

    let out_path = out_path.unwrap_or_else(|| {
        directory
            .join("robot-tasks")
            .join(format!("{}.json", safe_file_stem(&node.id)))
    });
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let artifact_directory = out_path.parent().unwrap_or_else(|| Path::new("."));
    let document = RobotTaskDocument {
        format: ROBOT_TASK_FORMAT.to_string(),
        id: node.id.clone(),
        plan: document_reference(&directory.join(WORKCELL_PLAN_FILE), artifact_directory),
        scene: document_reference(&scene_path, artifact_directory),
        after: node.after.clone(),
        action: RobotTaskAction::Transfer {
            object: RobotTaskObject {
                labware: labware.clone(),
                scene_node: labware.clone(),
            },
            source: RobotTaskEndpoint {
                station: from.clone(),
                scene_node: from.clone(),
            },
            destination: RobotTaskEndpoint {
                station: to.clone(),
                scene_node: to.clone(),
            },
            instructions: instructions.clone(),
            completion: RobotTaskCompletion {
                relation: "object-at-station".to_string(),
                object: labware.clone(),
                target: to.clone(),
            },
        },
    };
    let text = format!("{}\n", serde_json::to_string_pretty(&document)?);
    crate::stamp::write_if_changed(&out_path, &text)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    let report = RobotTaskReport {
        id: node.id.clone(),
        object: labware.clone(),
        source: from.clone(),
        destination: to.clone(),
        task: out_path.display().to_string(),
    };
    output.success(
        "robot-task",
        &report,
        format!(
            "robot task '{}': {} from {} to {}\n  {}",
            report.id,
            report.object,
            report.source,
            report.destination,
            out_path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_becomes_a_safe_file_name() {
        assert_eq!(
            safe_file_stem("assembly_thermocycle.to/odtc-1"),
            "assembly_thermocycle-to-odtc-1"
        );
    }

    #[test]
    fn document_paths_are_relative_to_the_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let wave = directory.path().join("wave-001");
        let tasks = wave.join("robot-tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        let scene = wave.join("scene.json");
        std::fs::write(&scene, "{}").unwrap();

        assert_eq!(document_reference(&scene, &tasks), "../scene.json");
    }
}
