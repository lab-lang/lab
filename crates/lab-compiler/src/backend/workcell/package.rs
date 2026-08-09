//! Workcell package composition: the liquid handler's own planner runs
//! unchanged, and this module owns only the split — which after-run work
//! moves to an instrument station, the handoffs that carry labware there
//! and back, and the coordination plan that sequences all of it.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::runfmt::{
    THERMOCYCLE_RUN_FORMAT, ThermocycleRunDocument, WORKCELL_RUN_FORMAT, WorkcellAction,
    WorkcellNode, WorkcellRunDocument, WorkcellStation,
};
use crate::{ArtifactBundle, ArtifactError, ProtocolLairProgram};

use crate::backend::hamilton::star::StarBundle;
use crate::backend::hamilton::star::plan::{
    StarBuildError, StarExecutionPlan, ThermalRequirement, plan_selected_build,
};
use crate::backend::hamilton::star::profile::StarTargetProfile;
use crate::backend::package::{render_full_build_instructions, render_report};
use crate::backend::workcell::profile::WorkcellProfile;
use crate::planning::{BuildInventory, DependencyBuildManifest};
use crate::planning::{DependencyGraphError, resolve_dependency_graph};

#[derive(Clone, Debug)]
pub struct WorkcellDependencyBuildBundle {
    manifest: DependencyBuildManifest,
    artifacts: ArtifactBundle,
}

impl WorkcellDependencyBuildBundle {
    pub fn manifest(&self) -> &DependencyBuildManifest {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, WorkcellBuildError> {
        pretty_json(&self.manifest)
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }
}

#[derive(Debug, Error)]
pub enum WorkcellBuildError {
    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),
    #[error("failed to compile generated batch for '{artifact}': {source}")]
    Backend {
        artifact: String,
        #[source]
        source: StarBuildError,
    },
    #[error("failed to serialize workcell document: {0}")]
    Serialization(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

fn pretty_json<T: serde::Serialize>(value: &T) -> Result<String, WorkcellBuildError> {
    serde_json::to_string_pretty(value)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| WorkcellBuildError::Serialization(error.to_string()))
}

/// Compile a dependency-driven build for a workcell: per wave, the liquid
/// handler's package under its station directory, one thermocycle document
/// per lifted thermal requirement, and the coordination plan that orders
/// programs, handoffs, and remaining human steps.
pub fn compile_dependency_build(
    protocol: &ProtocolLairProgram,
    profile: &WorkcellProfile,
    star_profile: &StarTargetProfile,
    inventory: &BuildInventory,
) -> Result<WorkcellDependencyBuildBundle, WorkcellBuildError> {
    let graph = crate::backend::graph::protocol_build_graph(protocol).map_err(|source| {
        WorkcellBuildError::Backend {
            artifact: "<protocol>".into(),
            source: StarBuildError::Planning(source.into()),
        }
    })?;
    let manifest = resolve_dependency_graph(&graph, inventory)?;
    let mut waves = BTreeMap::<usize, BTreeSet<String>>::new();
    for node in &manifest.nodes {
        if let Some(iteration) = node.generated_in_iteration {
            waves
                .entry(iteration)
                .or_default()
                .insert(node.artifact.clone());
        }
    }

    let mut artifacts = ArtifactBundle::new();
    artifacts.insert_text(
        "dependency_manifest.json",
        "application/json",
        pretty_json(&manifest)?,
    )?;
    artifacts.insert_text(
        "dependency_report.md",
        "text/markdown",
        render_report(&manifest),
    )?;

    let mut instruction_batches = Vec::new();
    for (index, (iteration, selected)) in waves.into_iter().enumerate() {
        let label = selected.iter().cloned().collect::<Vec<_>>().join(", ");
        let mut plan =
            plan_selected_build(protocol, star_profile, Some(&selected)).map_err(|source| {
                WorkcellBuildError::Backend {
                    artifact: label.clone(),
                    source: StarBuildError::Planning(source),
                }
            })?;
        let assignment = assign_wave(&mut plan, profile);
        let automation =
            StarBundle::from_plan(plan).map_err(|source| WorkcellBuildError::Backend {
                artifact: label.clone(),
                source: StarBuildError::Emission(source),
            })?;

        let directory = format!("wave-{:03}", index + 1);
        let handler = profile.liquid_handler().name.clone();
        for generated in automation.artifacts().iter() {
            artifacts.insert(crate::GeneratedArtifact::bytes(
                format!("{directory}/stations/{handler}/{}", generated.path()),
                generated.media_type(),
                generated.contents().to_vec(),
            )?)?;
        }
        for document in &assignment.thermocycle_documents {
            let cycler = profile
                .thermocycler()
                .expect("a thermocycle document exists only when the workcell has a cycler")
                .name
                .clone();
            artifacts.insert_text(
                format!("{directory}/stations/{cycler}/{}.odtc.json", document.id),
                "application/json",
                pretty_json(document)?,
            )?;
        }
        let coordination = WorkcellRunDocument {
            format: WORKCELL_RUN_FORMAT.to_string(),
            stations: profile
                .stations
                .iter()
                .map(|station| WorkcellStation {
                    name: station.name.clone(),
                    kind: station.kind.as_str().to_string(),
                    program_dir: format!("stations/{}", station.name),
                })
                .collect(),
            nodes: assignment.nodes,
        };
        artifacts.insert_text(
            format!("{directory}/plan.workcell.json"),
            "application/json",
            pretty_json(&coordination)?,
        )?;
        let wave_manual = render_wave_manual(&coordination, &assignment.narrative, &handler);
        artifacts.insert_text(
            format!("{directory}/manual_protocol.md"),
            "text/markdown",
            wave_manual.clone(),
        )?;
        instruction_batches.push((index + 1, iteration, label, directory, wave_manual));
    }

    artifacts.insert_text(
        "manual_protocol.md",
        "text/markdown",
        render_full_build_instructions(&manifest, &instruction_batches),
    )?;

    Ok(WorkcellDependencyBuildBundle {
        manifest,
        artifacts,
    })
}

/// The result of assigning one wave's plan across stations.
struct WaveAssignment {
    nodes: Vec<WorkcellNode>,
    thermocycle_documents: Vec<ThermocycleRunDocument>,
    /// One human-readable line per node, for the wave manual.
    narrative: Vec<String>,
}

/// Splits a planned wave across the workcell's stations. Runs stay on the
/// liquid handler; each thermal requirement moves to the thermocycler
/// station when one exists (bracketed by handoffs) and otherwise remains
/// its operator prose. Every after-run step leaves the run documents —
/// sequencing in a workcell belongs to the coordination plan, not to any
/// one station's package.
fn assign_wave(plan: &mut StarExecutionPlan, profile: &WorkcellProfile) -> WaveAssignment {
    let handler = profile.liquid_handler().name.clone();
    let cycler = profile.thermocycler().map(|station| station.name.clone());
    let mut nodes: Vec<WorkcellNode> = Vec::new();
    let mut narrative = Vec::new();
    let mut thermocycle_documents = Vec::new();
    let mut previous: Option<String> = None;

    let mut push = |node: WorkcellNode, line: String, previous: &mut Option<String>| {
        *previous = Some(node.id.clone());
        narrative.push(line);
        nodes.push(node);
    };

    for run in &mut plan.runs {
        push(
            WorkcellNode {
                id: run.id.clone(),
                after: previous.iter().cloned().collect(),
                action: WorkcellAction::StationProgram {
                    station: handler.clone(),
                    document: format!("stations/{handler}/{}.star.json", run.id),
                },
            },
            format!("[{handler}] {} — run `{}.star.json`", run.title, run.id),
            &mut previous,
        );

        let lifted: BTreeMap<usize, ThermalRequirement> = match &cycler {
            Some(_) => run
                .thermal_after
                .iter()
                .map(|requirement| (requirement.fallback_index, requirement.clone()))
                .collect(),
            None => BTreeMap::new(),
        };

        for (index, manual) in run.manual_after.iter().enumerate() {
            if let (Some(cycler_name), Some(requirement)) = (&cycler, lifted.get(&index)) {
                push(
                    WorkcellNode {
                        id: format!("{}.to-{cycler_name}", requirement.id),
                        after: previous.iter().cloned().collect(),
                        action: WorkcellAction::Handoff {
                            from: handler.clone(),
                            to: cycler_name.clone(),
                            labware: requirement.plate.clone(),
                            instructions: format!(
                                "Seal the {} and move it from {handler} to {cycler_name}; close the door.",
                                requirement.plate
                            ),
                        },
                    },
                    format!(
                        "[handoff] seal the {} and carry it to {cycler_name}",
                        requirement.plate
                    ),
                    &mut previous,
                );
                push(
                    WorkcellNode {
                        id: requirement.id.clone(),
                        after: previous.iter().cloned().collect(),
                        action: WorkcellAction::StationProgram {
                            station: cycler_name.clone(),
                            document: format!(
                                "stations/{cycler_name}/{}.odtc.json",
                                requirement.id
                            ),
                        },
                    },
                    format!(
                        "[{cycler_name}] {} — run `{}.odtc.json`",
                        requirement.title, requirement.id
                    ),
                    &mut previous,
                );
                push(
                    WorkcellNode {
                        id: format!("{}.return", requirement.id),
                        after: previous.iter().cloned().collect(),
                        action: WorkcellAction::Handoff {
                            from: cycler_name.clone(),
                            to: handler.clone(),
                            labware: requirement.plate.clone(),
                            instructions: format!(
                                "Retrieve the {} from {cycler_name} and return it to the {handler} deck position it came from.",
                                requirement.plate
                            ),
                        },
                    },
                    format!(
                        "[handoff] return the {} to the {handler} deck",
                        requirement.plate
                    ),
                    &mut previous,
                );
                thermocycle_documents.push(ThermocycleRunDocument {
                    format: THERMOCYCLE_RUN_FORMAT.to_string(),
                    id: requirement.id.clone(),
                    title: requirement.title.clone(),
                    plate: requirement.plate.clone(),
                    profile: requirement.profile.clone(),
                    final_hold_celsius: requirement.final_hold_celsius,
                    fill_volume_ul: requirement.fill_volume_ul,
                });
            } else {
                push(
                    WorkcellNode {
                        id: format!("{}.manual-{}", run.id, index + 1),
                        after: previous.iter().cloned().collect(),
                        action: WorkcellAction::Manual {
                            title: manual.title.clone(),
                            instructions: manual.instructions.clone(),
                        },
                    },
                    format!("[by hand] {}", manual.title),
                    &mut previous,
                );
            }
        }
        run.manual_after.clear();
        run.thermal_after.clear();
    }

    WaveAssignment {
        nodes,
        thermocycle_documents,
        narrative,
    }
}

/// The wave's human-readable coordination narrative. Deck loading and
/// source fills stay in the liquid handler's own manual under its station
/// directory; this document owns only the order of work.
fn render_wave_manual(
    coordination: &WorkcellRunDocument,
    narrative: &[String],
    handler: &str,
) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    let _ = writeln!(text, "# Lab workcell wave — coordination\n");
    let _ = writeln!(
        text,
        "Load the {handler} deck and sources first: see `stations/{handler}/manual_protocol.md`.\n"
    );
    let _ = writeln!(text, "## Stations\n");
    let _ = writeln!(text, "| Station | Kind | Programs |");
    let _ = writeln!(text, "|---|---|---|");
    for station in &coordination.stations {
        let _ = writeln!(
            text,
            "| {} | {} | `{}/` |",
            station.name, station.kind, station.program_dir
        );
    }
    let _ = writeln!(text, "\n## Sequence\n");
    for (index, line) in narrative.iter().enumerate() {
        let _ = writeln!(text, "{}. {line}", index + 1);
    }
    let _ = writeln!(
        text,
        "\nEvery handoff and manual step is confirmed by the operator before the next node starts; `lab run` walks this same sequence."
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::hamilton::star::plan::plan_build;
    use crate::backend::hamilton::star::profile::StarTargetProfile;
    use crate::test_support::golden_gate_protocol;

    const CELL: &str = r#"
[target]
backend = "workcell"

[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"

[[station]]
name = "odtc-1"
kind = "inheco.odtc"
"#;

    const CELL_WITHOUT_CYCLER: &str = r#"
[target]
backend = "workcell"

[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"
"#;

    fn planned() -> StarExecutionPlan {
        let protocol = golden_gate_protocol();
        let profile =
            StarTargetProfile::parse("hamilton-star", "").expect("the reference bench parses");
        plan_build(&protocol, &profile).expect("the example plans")
    }

    #[test]
    fn a_cycler_station_lifts_every_thermal_step_and_its_prose() {
        let mut plan = planned();
        let profile = WorkcellProfile::parse("cell", CELL).expect("the workcell parses");
        let assignment = assign_wave(&mut plan, &profile);

        assert_eq!(
            assignment.thermocycle_documents.len(),
            3,
            "assembly cycling, heat shock, and recovery all move to the cycler"
        );
        assert!(
            plan.runs.iter().all(|run| run.manual_after.is_empty()),
            "sequencing leaves the run documents entirely in a workcell build"
        );
        let ids: Vec<&str> = assignment
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        let position = |id: &str| {
            ids.iter()
                .position(|candidate| *candidate == id)
                .unwrap_or_else(|| panic!("node '{id}' is in the plan: {ids:?}"))
        };
        assert_eq!(position("assembly_run"), 0, "the wave opens on the handler");
        assert!(
            position("assembly_thermocycle.to-odtc-1") < position("assembly_thermocycle"),
            "the plate is handed to the cycler before its program runs"
        );
        assert!(
            position("assembly_thermocycle.return") < position("assembly_run.manual-2"),
            "the plate returns before the operator stages the next stage"
        );
        assert!(
            position("transformation_heat_shock") < position("transformation_recovery_run"),
            "the heat shock finishes before the recovery run's liquid handling"
        );
        let program_documents: Vec<&str> = assignment
            .nodes
            .iter()
            .filter_map(|node| match &node.action {
                WorkcellAction::StationProgram { document, .. } => Some(document.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            program_documents.contains(&"stations/odtc-1/assembly_thermocycle.odtc.json"),
            "cycler programs live under the cycler's station directory: {program_documents:?}"
        );
    }

    #[test]
    fn without_a_cycler_every_after_step_stays_operator_prose() {
        let mut plan = planned();
        let manual_steps: usize = plan.runs.iter().map(|run| run.manual_after.len()).sum();
        let profile =
            WorkcellProfile::parse("cell", CELL_WITHOUT_CYCLER).expect("the workcell parses");
        let assignment = assign_wave(&mut plan, &profile);
        assert!(
            assignment.thermocycle_documents.is_empty(),
            "nothing lifts without a station to receive it"
        );
        let manual_nodes = assignment
            .nodes
            .iter()
            .filter(|node| matches!(node.action, WorkcellAction::Manual { .. }))
            .count();
        assert_eq!(
            manual_nodes, manual_steps,
            "each manual step becomes one coordination node"
        );
    }
}
