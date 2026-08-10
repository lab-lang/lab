//! Dependency-aware STAR package composition: the same wave semantics the
//! Opentrons backends use, emitting one run package per dependency wave.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{ArtifactBundle, ArtifactError, ProtocolLairProgram};

use crate::backend::document::DocMeta;
use crate::backend::hamilton::star::emit::StarBundle;
use crate::backend::hamilton::star::plan::{
    StarBuildError, plan_selected_build, protocol_build_graph,
};
use crate::backend::hamilton::star::profile::StarTargetProfile;
use crate::backend::package::{render_full_build_instructions, render_report};
use crate::backend::typst;
use crate::planning::{BuildInventory, DependencyBuildManifest};
use crate::planning::{DependencyGraphError, resolve_dependency_graph};

#[derive(Clone, Debug)]
pub struct StarDependencyBuildBundle {
    manifest: DependencyBuildManifest,
    artifacts: ArtifactBundle,
}

impl StarDependencyBuildBundle {
    pub fn manifest(&self) -> &DependencyBuildManifest {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, StarDependencyBuildError> {
        serde_json::to_string_pretty(&self.manifest)
            .map(|mut text| {
                text.push('\n');
                text
            })
            .map_err(|error| StarDependencyBuildError::Serialization(error.to_string()))
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }
}

#[derive(Debug, Error)]
pub enum StarDependencyBuildError {
    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),
    #[error("failed to compile generated batch for '{artifact}': {source}")]
    Backend {
        artifact: String,
        #[source]
        source: StarBuildError,
    },
    #[error("failed to serialize dependency build manifest: {0}")]
    Serialization(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

/// Specialize a source-derived dependency graph into independently
/// executable STAR packages. Graph resolution itself is target-neutral;
/// only the requirements projected into each node and the emitted batches
/// are owned by this module.
pub fn compile_dependency_build(
    protocol: &ProtocolLairProgram,
    profile: &StarTargetProfile,
    inventory: &BuildInventory,
) -> Result<StarDependencyBuildBundle, StarDependencyBuildError> {
    let graph =
        protocol_build_graph(protocol).map_err(|source| StarDependencyBuildError::Backend {
            artifact: "<protocol>".into(),
            source: StarBuildError::Planning(source),
        })?;
    let manifest = resolve_dependency_graph(&graph, inventory)?;
    // Artifacts generated in the same iteration have no ordering constraint
    // between them, so they share one machine session: one deck, one plate,
    // one pass per stage. Dependencies still force a wave boundary, and the
    // operator must finish a wave before starting the next.
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
        serde_json::to_string_pretty(&manifest)
            .map(|mut text| {
                text.push('\n');
                text
            })
            .map_err(|error| StarDependencyBuildError::Serialization(error.to_string()))?,
    )?;
    artifacts.insert_text(
        "dependency_report.typ",
        "text/x-typst",
        typst::render(&render_report(
            DocMeta::new(
                "Dependency report",
                "Artifact graph, wave schedule, and blockers",
                &profile.target.name,
                "Hamilton STAR",
            ),
            &manifest,
        )),
    )?;
    artifacts.insert_text(typst::STYLE_PATH, "text/x-typst", typst::STYLE)?;
    let mut instruction_batches = Vec::new();
    let mut bench = Vec::new();
    for (index, (iteration, selected)) in waves.into_iter().enumerate() {
        let label = selected.iter().cloned().collect::<Vec<_>>().join(", ");
        let plan = plan_selected_build(protocol, profile, Some(&selected)).map_err(|source| {
            StarDependencyBuildError::Backend {
                artifact: label.clone(),
                source: StarBuildError::Planning(source),
            }
        })?;
        let automation =
            StarBundle::from_plan(plan).map_err(|source| StarDependencyBuildError::Backend {
                artifact: label.clone(),
                source: StarBuildError::Emission(source),
            })?;
        let directory = format!("wave-{:03}", index + 1);
        if bench.is_empty() {
            bench = automation.bench_blocks();
        }
        instruction_batches.push((
            index + 1,
            iteration,
            label,
            directory.clone(),
            automation.run_blocks(),
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
        "manual_protocol.typ",
        "text/x-typst",
        typst::render(&render_full_build_instructions(
            DocMeta::new(
                "Automated plasmid build",
                "Operator instructions for the full dependency-driven build",
                &profile.target.name,
                "Hamilton STAR",
            ),
            &manifest,
            bench,
            &instruction_batches,
            Vec::new(),
        )),
    )?;

    Ok(StarDependencyBuildBundle {
        manifest,
        artifacts,
    })
}
