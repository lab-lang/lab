//! Dependency-aware OT-2 package composition.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{ArtifactBundle, ArtifactError, ProtocolLairProgram};

use crate::backend::opentrons_ot2::profile::Ot2TargetProfile;
use crate::planning::{BuildInventory, DependencyBuildManifest};
use crate::planning::{DependencyGraphError, resolve_dependency_graph};

use crate::backend::opentrons_ot2::plan::{plan_selected_build, protocol_build_graph};
use crate::backend::opentrons_ot2::{Ot2BuildError, Ot2Bundle};

use super::report::{pretty_json, render_full_build_instructions, render_report};

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
    protocol: &ProtocolLairProgram,
    profile: &Ot2TargetProfile,
    inventory: &BuildInventory,
) -> Result<DependencyBuildBundle, DependencyBuildError> {
    let graph = protocol_build_graph(protocol).map_err(|source| DependencyBuildError::Backend {
        artifact: "<protocol>".into(),
        source: Ot2BuildError::Planning(source),
    })?;
    let manifest = resolve_dependency_graph(&graph, inventory)?;
    // Artifacts generated in the same iteration have no ordering constraint
    // between them, so they share one robot run: one deck, one plate, one pass
    // per stage. Dependencies still force a wave boundary, and the operator
    // must finish a wave before starting the next.
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
        let plan = plan_selected_build(protocol, profile, Some(&selected)).map_err(|source| {
            DependencyBuildError::Backend {
                artifact: label.clone(),
                source: Ot2BuildError::Planning(source),
            }
        })?;
        let automation =
            Ot2Bundle::from_plan(plan).map_err(|source| DependencyBuildError::Backend {
                artifact: label.clone(),
                source: Ot2BuildError::Emission(source),
            })?;
        let directory = format!("wave-{:03}", index + 1);
        instruction_batches.push((
            index + 1,
            iteration,
            label,
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

#[cfg(test)]
mod tests {
    use crate::planning::{ArtifactResolution, DependencyBuildStatus};
    use lab_language::compile_module;

    use crate::backend::opentrons_ot2::package::compile::*;

    const SOURCE: &str = r#"
use std.bio.build
use std.bio.designs
use std.bio.golden_gate
use std.lab.plasmid

buy part terminal_part
buy part source_part
buy backbone receiver
buy backbone carrier
buy restriction_enzyme BsaI
buy restriction_enzyme BsmBI
buy chassis DH5alpha
buy antibiotic chloramphenicol

plasmid intermediate:
  sequence = dna("TGCA")
  backbone = carrier
  components = [source_part]
  restriction_enzyme = BsmBI
  require topology == circular
  accept sequence == design.sequence

plasmid final_artifact:
  sequence = dna("ACGT")
  backbone = receiver
  components = [intermediate, terminal_part]
  restriction_enzyme = BsaI
  require topology == circular
  accept sequence == design.sequence

strain final_host:
  chassis = DH5alpha
  plasmids = [final_artifact]
  selection = chloramphenicol

workflow assemble_intermediate() -> Material<Plasmid>:
  dependencies = []
  product <- realize intermediate from dependencies
  return product

workflow assemble_final_artifact(
  intermediate: Material<Plasmid>,
) -> Material<Plasmid>:
  dependencies = [intermediate]
  product <- realize final_artifact from dependencies
  return product

workflow build_final_host(
  final_artifact: Material<Plasmid>,
) -> (
  strain: Material<Strain>,
  plate: Material<Plate>,
):
  dependencies = [final_artifact]
  cells <- provision DH5alpha
  strain, culture <- transform final_host from dependencies into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return strain, plate
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
        let checked = compile_module(SOURCE).unwrap();
        let protocol = crate::PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap();
        let bundle =
            compile_dependency_build(&protocol, &Ot2TargetProfile::default(), &inventory())
                .unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Complete);
        assert_eq!(bundle.manifest.roots, ["final_host"]);
        assert_eq!(
            bundle.manifest.generated_artifacts,
            ["intermediate", "final_artifact", "final_host"]
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
                .get("wave-001/assembly_protocol.py")
                .is_some()
        );
        assert!(
            bundle
                .artifacts()
                .get("wave-002/assembly_protocol.py")
                .is_some()
        );
        assert!(
            bundle
                .artifacts()
                .get("wave-003/transformation_protocol.py")
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
        assert!(instructions.contains("## Run 001 — `intermediate`"));
        assert!(instructions.contains("### Stage 1 — Golden Gate assembly"));
    }

    #[test]
    fn reports_missing_leaves_without_silent_success() {
        let checked = compile_module(SOURCE).unwrap();
        let protocol = crate::PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap();
        let bundle = compile_dependency_build(
            &protocol,
            &Ot2TargetProfile::default(),
            &BuildInventory::default(),
        )
        .unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Partial);
        assert!(bundle.manifest.generated_artifacts.is_empty());
        assert!(bundle.manifest.nodes.iter().any(|node| {
            node.artifact == "intermediate"
                && node.missing_materials.contains(&"source_part".to_owned())
        }));
    }

    #[test]
    fn reuses_existing_artifacts_without_resolving_their_recipe_leaves() {
        let checked = compile_module(SOURCE).unwrap();
        let protocol = crate::PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap();
        let mut inventory = inventory();
        inventory.available_artifacts.insert("intermediate".into());
        inventory.available_materials.remove("source_part");
        inventory.available_materials.remove("carrier");
        let bundle =
            compile_dependency_build(&protocol, &Ot2TargetProfile::default(), &inventory).unwrap();
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
            "workflow assemble_intermediate() -> Material<Plasmid>:\n  dependencies = []",
            "workflow assemble_intermediate(\n  final_artifact: Material<Plasmid>,\n) -> Material<Plasmid>:\n  dependencies = [final_artifact]",
        );
        let checked = compile_module(&source).unwrap();
        let protocol = crate::PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap();
        let bundle =
            compile_dependency_build(&protocol, &Ot2TargetProfile::default(), &inventory())
                .unwrap();
        assert_eq!(bundle.manifest.status, DependencyBuildStatus::Partial);
        assert!(
            bundle
                .manifest
                .nodes
                .iter()
                .filter(|node| node.artifact != "final_host")
                .all(|node| node.resolution == ArtifactResolution::Cyclic)
        );

        let mut inventory = inventory();
        inventory
            .available_artifacts
            .insert("final_artifact".into());
        let bundle =
            compile_dependency_build(&protocol, &Ot2TargetProfile::default(), &inventory).unwrap();
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
