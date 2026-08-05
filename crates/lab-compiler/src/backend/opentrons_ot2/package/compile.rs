//! Dependency-aware OT-2 package composition.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use serde::Serialize;
use thiserror::Error;

use crate::{ArtifactBundle, ArtifactError};

use crate::planning::{ArtifactResolution, BuildInventory, DependencyBuildManifest};
use crate::planning::{BuildGraph, BuildGraphNode, DependencyGraphError, resolve_dependency_graph};

use crate::backend::opentrons_ot2::{Ot2BuildArtifact, Ot2BuildError, Ot2BuildIr, compile_build};

const STANDARD_MATERIALS: [&str; 4] = [
    "T4_DNA_ligase",
    "T4_DNA_ligase_buffer",
    "nuclease_free_water",
    "recovery_medium",
];

#[derive(Clone, Debug)]
pub struct DependencyBuildBundle {
    manifest: DependencyBuildManifest,
    artifacts: ArtifactBundle,
}

impl DependencyBuildBundle {
    pub fn manifest(&self) -> &DependencyBuildManifest {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, DependencyBuildError> {
        pretty_json(&self.manifest)
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }
}

#[derive(Debug, Error)]
pub enum DependencyBuildError {
    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),
    #[error("failed to compile generated batch for '{artifact}': {source}")]
    Backend {
        artifact: String,
        #[source]
        source: Ot2BuildError,
    },
    #[error("failed to serialize dependency build manifest: {0}")]
    Serialization(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

/// Specialize a source-derived dependency graph into independently executable
/// OT-2 packages. Graph resolution itself is target-neutral; only the
/// requirements projected into each graph node and the emitted batches are
/// owned by this module.
pub fn compile_dependency_build(
    build: &Ot2BuildIr,
    inventory: &BuildInventory,
) -> Result<DependencyBuildBundle, DependencyBuildError> {
    let declared = build
        .artifacts()
        .iter()
        .map(|artifact| (artifact.name().to_owned(), artifact))
        .collect::<BTreeMap<_, _>>();
    let graph = BuildGraph {
        nodes: declared
            .iter()
            .map(|(name, artifact)| {
                let recipe = artifact.build_recipe();
                let dependencies = artifact
                    .dependencies
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let mut required_materials = recipe
                    .components()
                    .iter()
                    .filter(|component| !dependencies.contains(*component))
                    .cloned()
                    .collect::<BTreeSet<_>>();
                required_materials.insert(recipe.backbone().to_owned());
                required_materials.insert(recipe.restriction_enzyme().to_owned());
                required_materials.insert(recipe.host().to_owned());
                required_materials.insert(recipe.selection().to_owned());
                required_materials.extend(STANDARD_MATERIALS.into_iter().map(str::to_owned));
                (
                    name.clone(),
                    BuildGraphNode {
                        dependencies,
                        steps: recipe.steps().to_vec(),
                        required_materials,
                    },
                )
            })
            .collect(),
    };
    let manifest = resolve_dependency_graph(&graph, inventory)?;
    let mut scheduled = manifest
        .nodes
        .iter()
        .filter_map(|node| {
            node.generated_in_iteration
                .map(|iteration| (node.artifact.clone(), iteration))
        })
        .collect::<Vec<_>>();
    scheduled.sort_by(
        |(left_name, left_iteration), (right_name, right_iteration)| {
            (left_iteration, left_name).cmp(&(right_iteration, right_name))
        },
    );

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
    for (index, (artifact_name, iteration)) in scheduled.into_iter().enumerate() {
        let artifact: Ot2BuildArtifact = declared[&artifact_name].clone();
        let single = Ot2BuildIr::new(vec![artifact]).expect("one target artifact is a build");
        let automation =
            compile_build(&single).map_err(|source| DependencyBuildError::Backend {
                artifact: artifact_name.clone(),
                source,
            })?;
        let directory = format!("batch-{:03}-{}", index + 1, artifact_name);
        instruction_batches.push((
            index + 1,
            iteration,
            artifact_name.clone(),
            directory.clone(),
            automation.manual_protocol().to_owned(),
        ));
        for generated in automation.artifacts().iter() {
            artifacts.insert(crate::GeneratedArtifact::bytes(
                format!("{directory}/{}", generated.path()),
                generated.media_type(),
                generated.contents().to_vec(),
            )?)?;
        }
    }
    artifacts.insert_text(
        "manual_protocol.md",
        "text/markdown",
        render_full_build_instructions(&manifest, &instruction_batches),
    )?;

    Ok(DependencyBuildBundle {
        manifest,
        artifacts,
    })
}

fn render_full_build_instructions(
    manifest: &DependencyBuildManifest,
    batches: &[(usize, usize, String, String, String)],
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "# Lab dependency-driven build — human instructions\n"
    )
    .unwrap();
    writeln!(output, "> Generated concept protocol. Review and qualify every batch for the actual laboratory before execution. Planning success is not physical-build or acceptance evidence.\n").unwrap();
    writeln!(output, "## Build overview\n").unwrap();
    writeln!(output, "- Planning status: `{:?}`", manifest.status).unwrap();
    writeln!(output, "- Root artifacts: {}", manifest.roots.join(", ")).unwrap();
    writeln!(output, "- Generated batches: {}", batches.len()).unwrap();
    writeln!(
        output,
        "- Existing artifacts reused: {}\n",
        if manifest.existing_artifacts.is_empty() {
            "none".to_owned()
        } else {
            manifest.existing_artifacts.join(", ")
        }
    )
    .unwrap();
    writeln!(output, "Consult `dependency_manifest.json` for the machine-readable plan and `dependency_report.md` for dependency and blocker details. Do not begin a batch until all of its declared artifact dependencies have been physically produced or retrieved and accepted as suitable inputs. Batches in the same planning wave are dependency-independent, although actual parallel execution still depends on qualified laboratory capacity.\n").unwrap();
    writeln!(output, "This package does not automate DNA recovery or preparation between batches. Before a generated artifact is used downstream, prepare it in the form and concentration required by the later batch and record the corresponding acceptance evidence.\n").unwrap();

    writeln!(output, "## Execution order\n").unwrap();
    if batches.is_empty() {
        writeln!(output, "No generated batch is scheduled. The requested roots are either already available or unresolved; inspect the dependency report before proceeding.\n").unwrap();
    } else {
        writeln!(
            output,
            "| Batch | Planning wave | Artifact | Package directory |"
        )
        .unwrap();
        writeln!(output, "| ---: | ---: | --- | --- |").unwrap();
        for (batch, iteration, artifact, directory, _) in batches {
            writeln!(
                output,
                "| {batch:03} | {iteration} | `{artifact}` | `{directory}/` |"
            )
            .unwrap();
        }
        writeln!(output).unwrap();
    }

    for (batch, iteration, artifact, directory, manual) in batches {
        writeln!(output, "## Batch {batch:03} — `{artifact}`\n").unwrap();
        writeln!(output, "Planning wave: {iteration}. Robot protocols, the Lab automation manifest, and the standalone batch manual are in `{directory}/`.\n").unwrap();
        if let Some(node) = manifest
            .nodes
            .iter()
            .find(|node| node.artifact == *artifact)
            && !node.dependencies.is_empty()
        {
            writeln!(
                output,
                "Required generated or retrieved artifact inputs: {}.\n",
                node.dependencies
                    .iter()
                    .map(|dependency| format!("`{dependency}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }
        let node = manifest
            .nodes
            .iter()
            .find(|node| node.artifact == *artifact)
            .expect("scheduled artifact has a dependency node");
        writeln!(
            output,
            "Requested abstract steps: {}.\n",
            node.steps
                .iter()
                .map(|step| format!("`{step}`"))
                .collect::<Vec<_>>()
                .join(" → ")
        )
        .unwrap();
        if manifest
            .edges
            .iter()
            .any(|edge| edge.depends_on == *artifact)
        {
            writeln!(output, "After completing this batch, retain, prepare, and verify material for `{artifact}` before treating it as an input to a later batch.\n").unwrap();
        } else {
            writeln!(output, "After completing this batch, retain and verify the requested root artifact `{artifact}` and record its acceptance evidence.\n").unwrap();
        }
        for (line_index, line) in manual.lines().enumerate() {
            if line_index == 0 && line.starts_with("# ") {
                continue;
            }
            if line.starts_with('#') {
                writeln!(output, "#{line}").unwrap();
            } else {
                writeln!(output, "{line}").unwrap();
            }
        }
        writeln!(output).unwrap();
    }

    output
}

fn render_report(manifest: &DependencyBuildManifest) -> String {
    let mut output = String::new();
    writeln!(output, "# Dependency-driven build\n").unwrap();
    writeln!(output, "Status: `{:?}`\n", manifest.status).unwrap();
    writeln!(output, "Roots: {}\n", manifest.roots.join(", ")).unwrap();
    writeln!(
        output,
        "| Artifact | Dependencies | Resolution | Iteration |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | ---: |").unwrap();
    for node in &manifest.nodes {
        writeln!(
            output,
            "| {} | {} | {:?} | {} |",
            node.artifact,
            node.dependencies.join(", "),
            node.resolution,
            node.generated_in_iteration
                .map_or_else(|| "-".into(), |value| value.to_string())
        )
        .unwrap();
    }
    let blockers = manifest
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.resolution,
                ArtifactResolution::Blocked | ArtifactResolution::Cyclic
            )
        })
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        writeln!(output, "\n## Unresolved inputs\n").unwrap();
        for node in blockers {
            writeln!(
                output,
                "- `{}`: dependencies [{}]; materials [{}]; resolution `{:?}`",
                node.artifact,
                node.missing_dependencies.join(", "),
                node.missing_materials.join(", "),
                node.resolution
            )
            .unwrap();
        }
    }
    writeln!(output, "\n## Execution boundary\n").unwrap();
    writeln!(output, "Each generated artifact is packaged as an independently reviewable assembly, transformation, and plating batch. A product is added to the planning inventory only after its batch is scheduled; physical execution and acceptance evidence remain laboratory responsibilities.").unwrap();
    output
}

fn pretty_json(value: &impl Serialize) -> Result<String, DependencyBuildError> {
    serde_json::to_string_pretty(value)
        .map(|mut output| {
            output.push('\n');
            output
        })
        .map_err(|error| DependencyBuildError::Serialization(error.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::backend::opentrons_ot2::lower_build;
    use crate::planning::DependencyBuildStatus;
    use lab_language::compile_module;

    use crate::backend::opentrons_ot2::package::compile::*;

    const SOURCE: &str = r#"
use std.bio.build
use std.bio.inventory
use std.lab.plasmid_actions

record BuiltArtifact:
  product: Material<Plasmid>
  plate: Material<Plate>

terminal_part = part("terminal_part")
source_part = part("source_part")
receiver = backbone("receiver")
carrier = backbone("carrier")
BsaI = restriction_enzyme("BsaI")
BsmBI = restriction_enzyme("BsmBI")
DH5alpha = strain("DH5alpha")
chloramphenicol = antibiotic("chloramphenicol")

plasmid intermediate:
  sequence: dna("TGCA")
  backbone: carrier
  components: [source_part]
  restriction_enzyme: BsmBI
  host: DH5alpha
  selection: chloramphenicol
  require topology == circular
  accept sequence == design.sequence

plasmid final_artifact:
  sequence: dna("ACGT")
  backbone: receiver
  components: [intermediate, terminal_part]
  restriction_enzyme: BsaI
  host: DH5alpha
  selection: chloramphenicol
  require topology == circular
  accept sequence == design.sequence

workflow realize_intermediate() -> BuiltArtifact:
  dependencies = []
  product, construct <- realize intermediate from dependencies
  cells <- provision DH5alpha
  culture <- transform construct into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return BuiltArtifact{product: product, plate: plate}

workflow realize_final_artifact(intermediate: Material<Plasmid>) -> BuiltArtifact:
  dependencies = [intermediate]
  product, construct <- realize final_artifact from dependencies
  cells <- provision DH5alpha
  culture <- transform construct into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return BuiltArtifact{product: product, plate: plate}
"#;

    fn inventory() -> BuildInventory {
        BuildInventory {
            available_materials: [
                "terminal_part",
                "source_part",
                "receiver",
                "carrier",
                "BsaI",
                "BsmBI",
                "DH5alpha",
                "chloramphenicol",
                "T4_DNA_ligase",
                "T4_DNA_ligase_buffer",
                "nuclease_free_water",
                "recovery_medium",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            available_artifacts: BTreeSet::new(),
        }
    }

    #[test]
    fn derives_graph_waves_and_retries_from_checked_material_dataflow() {
        let build = lower_build(&compile_module(SOURCE).unwrap()).unwrap();
        let bundle = compile_dependency_build(&build, &inventory()).unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Complete);
        assert_eq!(bundle.manifest.roots, ["final_artifact"]);
        assert_eq!(
            bundle.manifest.generated_artifacts,
            ["intermediate", "final_artifact"]
        );
        let intermediate = bundle
            .manifest
            .nodes
            .iter()
            .find(|node| node.artifact == "intermediate")
            .unwrap();
        let final_artifact = bundle
            .manifest
            .nodes
            .iter()
            .find(|node| node.artifact == "final_artifact")
            .unwrap();
        assert_eq!(intermediate.generated_in_iteration, Some(1));
        assert_eq!(final_artifact.generated_in_iteration, Some(2));
        assert!(
            bundle
                .artifacts()
                .get("batch-001-intermediate/assembly_protocol.py")
                .is_some()
        );
        assert!(
            bundle
                .artifacts()
                .get("batch-002-final_artifact/assembly_protocol.py")
                .is_some()
        );
        assert!(bundle.artifacts().get("dependency_report.md").is_some());
        let instructions = bundle
            .artifacts()
            .get("manual_protocol.md")
            .unwrap()
            .text_contents()
            .unwrap();
        assert!(instructions.contains("## Execution order"));
        assert!(instructions.contains("## Batch 001 — `intermediate`"));
        assert!(instructions.contains("### Stage 1 — Golden Gate assembly"));
    }

    #[test]
    fn reports_missing_leaves_without_silent_success() {
        let build = lower_build(&compile_module(SOURCE).unwrap()).unwrap();
        let bundle = compile_dependency_build(&build, &BuildInventory::default()).unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Partial);
        assert!(bundle.manifest.generated_artifacts.is_empty());
        assert!(bundle.manifest.nodes.iter().any(|node| {
            node.artifact == "intermediate"
                && node.missing_materials.contains(&"source_part".to_owned())
        }));
    }

    #[test]
    fn reuses_existing_artifacts_without_resolving_their_recipe_leaves() {
        let build = lower_build(&compile_module(SOURCE).unwrap()).unwrap();
        let mut inventory = inventory();
        inventory.available_artifacts.insert("intermediate".into());
        inventory.available_materials.remove("source_part");
        inventory.available_materials.remove("carrier");
        let bundle = compile_dependency_build(&build, &inventory).unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Complete);
        let intermediate = bundle
            .manifest
            .nodes
            .iter()
            .find(|node| node.artifact == "intermediate")
            .unwrap();
        assert_eq!(intermediate.resolution, ArtifactResolution::Existing);
        assert_eq!(intermediate.generated_in_iteration, None);
        let final_artifact = bundle
            .manifest
            .nodes
            .iter()
            .find(|node| node.artifact == "final_artifact")
            .unwrap();
        assert_eq!(final_artifact.generated_in_iteration, Some(1));
    }

    #[test]
    fn detects_cycles_from_source_references() {
        let source = SOURCE.replace(
            "workflow realize_intermediate() -> BuiltArtifact:\n  dependencies = []",
            "workflow realize_intermediate(final_artifact: Material<Plasmid>) -> BuiltArtifact:\n  dependencies = [final_artifact]",
        );
        let build = lower_build(&compile_module(&source).unwrap()).unwrap();
        let bundle = compile_dependency_build(&build, &inventory()).unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Partial);
        assert!(
            bundle
                .manifest
                .nodes
                .iter()
                .all(|node| node.resolution == ArtifactResolution::Cyclic)
        );

        let mut inventory = inventory();
        inventory
            .available_artifacts
            .insert("final_artifact".into());
        let bundle = compile_dependency_build(&build, &inventory).unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Complete);
        assert_eq!(
            bundle
                .manifest
                .nodes
                .iter()
                .find(|node| node.artifact == "intermediate")
                .unwrap()
                .resolution,
            ArtifactResolution::Generated
        );
    }
}
