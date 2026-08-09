//! Dependency-aware Flex package composition.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{ArtifactBundle, ArtifactError, ProtocolLairProgram};

use crate::backend::opentrons::flex::profile::FlexTargetProfile;
use crate::planning::{BuildInventory, DependencyBuildManifest};
use crate::planning::{DependencyGraphError, resolve_dependency_graph};

use crate::backend::opentrons::flex::plan::{plan_selected_build, protocol_build_graph};
use crate::backend::opentrons::flex::{FlexBuildError, FlexBundle};

use crate::backend::opentrons::flex::package::report::{
    pretty_json, render_full_build_instructions, render_report,
};

#[derive(Clone, Debug)]
pub struct FlexDependencyBuildBundle {
    manifest: DependencyBuildManifest,
    artifacts: ArtifactBundle,
}

impl FlexDependencyBuildBundle {
    pub fn manifest(&self) -> &DependencyBuildManifest {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, FlexDependencyBuildError> {
        pretty_json(&self.manifest)
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }
}

#[derive(Debug, Error)]
pub enum FlexDependencyBuildError {
    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),
    #[error("failed to compile generated batch for '{artifact}': {source}")]
    Backend {
        artifact: String,
        #[source]
        source: FlexBuildError,
    },
    #[error("failed to serialize dependency build manifest: {0}")]
    Serialization(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

/// Specialize a source-derived dependency graph into independently executable
/// Flex packages. Graph resolution itself is target-neutral; only the
/// requirements projected into each graph node and the emitted batches are
/// owned by this module.
pub fn compile_dependency_build(
    protocol: &ProtocolLairProgram,
    profile: &FlexTargetProfile,
    inventory: &BuildInventory,
) -> Result<FlexDependencyBuildBundle, FlexDependencyBuildError> {
    let graph =
        protocol_build_graph(protocol).map_err(|source| FlexDependencyBuildError::Backend {
            artifact: "<protocol>".into(),
            source: FlexBuildError::Planning(source),
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
            FlexDependencyBuildError::Backend {
                artifact: label.clone(),
                source: FlexBuildError::Planning(source),
            }
        })?;
        let automation =
            FlexBundle::from_plan(plan).map_err(|source| FlexDependencyBuildError::Backend {
                artifact: label.clone(),
                source: FlexBuildError::Emission(source),
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

    Ok(FlexDependencyBuildBundle {
        manifest,
        artifacts,
    })
}
