//! Artifact emission: the per-run `lab.star-run.v0` documents, the
//! automation manifest, and the operator's manual protocol, bundled the
//! same way for a direct build and for each dependency wave.

mod manual;
mod runs;

use serde::Serialize;

use crate::{ArtifactBundle, ProtocolLairProgram};

use crate::backend::hamilton::star::emit::manual::render_manual_protocol;
use crate::backend::hamilton::star::emit::runs::render_run;
use crate::backend::hamilton::star::plan::{
    StarBuildError, StarEmissionError, StarExecutionPlan, plan_build,
};
use crate::backend::hamilton::star::profile::StarTargetProfile;

pub use crate::backend::hamilton::star::emit::runs::RunStep;

/// Planned STAR program together with its emitted artifact package.
#[derive(Clone, Debug)]
pub struct StarBundle {
    manifest: StarExecutionPlan,
    artifacts: ArtifactBundle,
}

impl StarBundle {
    pub(in crate::backend::hamilton::star) fn from_plan(
        manifest: StarExecutionPlan,
    ) -> Result<Self, StarEmissionError> {
        let mut artifacts = ArtifactBundle::new();
        artifacts.insert_text(
            "automation_manifest.json",
            "application/json",
            pretty_json(&manifest)?,
        )?;
        artifacts.insert_text(
            "manual_protocol.md",
            "text/markdown",
            render_manual_protocol(&manifest),
        )?;
        // A batch emits a run document only for the stages its artifacts
        // actually reach; a frame sequence over an empty plan would fail on
        // the machine rather than at compile time.
        for run in &manifest.runs {
            artifacts.insert_text(
                format!("{}.star.json", run.id),
                "application/json",
                render_run(&manifest, run)?,
            )?;
        }
        Ok(Self {
            manifest,
            artifacts,
        })
    }

    pub fn manifest(&self) -> &StarExecutionPlan {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, StarEmissionError> {
        Ok(self.artifact_text("automation_manifest.json").to_owned())
    }

    pub fn manual_protocol(&self) -> &str {
        self.artifact_text("manual_protocol.md")
    }

    /// The run document for a run id the batch reached, e.g.
    /// `assembly_run`.
    pub fn run_document(&self, run_id: &str) -> Option<&str> {
        self.optional_artifact_text(&format!("{run_id}.star.json"))
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }

    fn artifact_text(&self, path: &str) -> &str {
        self.optional_artifact_text(path)
            .expect("a STAR bundle contains every unconditional artifact")
    }

    fn optional_artifact_text(&self, path: &str) -> Option<&str> {
        self.artifacts
            .get(path)?
            .text_contents()
            .expect("STAR artifacts are UTF-8")
            .into()
    }
}

pub fn compile_build(
    protocol: &ProtocolLairProgram,
    profile: &StarTargetProfile,
) -> Result<StarBundle, StarBuildError> {
    Ok(StarBundle::from_plan(plan_build(protocol, profile)?)?)
}

pub fn emit_program(program: &StarExecutionPlan) -> Result<StarBundle, StarEmissionError> {
    StarBundle::from_plan(program.clone())
}

pub(in crate::backend::hamilton::star) fn pretty_json(
    value: &impl Serialize,
) -> Result<String, StarEmissionError> {
    serde_json::to_string_pretty(value)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| StarEmissionError::Serialization(error.to_string()))
}
