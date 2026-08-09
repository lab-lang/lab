use serde::Serialize;

use crate::{ArtifactBundle, ProtocolLairProgram};

use crate::backend::opentrons::flex::emit::{
    render_assembly_protocol, render_manual_protocol, render_plating_protocol,
    render_transformation_protocol,
};

use crate::backend::opentrons::flex::profile::FlexTargetProfile;

use crate::backend::opentrons::flex::plan::{
    FlexBuildError, FlexEmissionError, FlexExecutionPlan, plan_build,
};

/// Planned Flex program together with its emitted artifact package.
#[derive(Clone, Debug)]
pub struct FlexBundle {
    manifest: FlexExecutionPlan,
    artifacts: ArtifactBundle,
}

impl FlexBundle {
    pub(in crate::backend::opentrons::flex) fn from_plan(
        manifest: FlexExecutionPlan,
    ) -> Result<Self, FlexEmissionError> {
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
        // A batch emits a robot protocol only for the stages its artifacts
        // actually reach. A batch that assembles plasmids and transforms none
        // has nothing to plate, and a protocol over an empty plan would fail on
        // the robot rather than at compile time.
        if !manifest.assemblies.is_empty() {
            artifacts.insert_text(
                "assembly_protocol.json",
                "application/json",
                render_assembly_protocol(&manifest)?,
            )?;
        }
        if !manifest.strains.is_empty() {
            artifacts.insert_text(
                "transformation_protocol.json",
                "application/json",
                render_transformation_protocol(&manifest)?,
            )?;
            artifacts.insert_text(
                "plating_protocol.json",
                "application/json",
                render_plating_protocol(&manifest)?,
            )?;
        }
        Ok(Self {
            manifest,
            artifacts,
        })
    }

    pub fn manifest(&self) -> &FlexExecutionPlan {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, FlexEmissionError> {
        Ok(self.artifact_text("automation_manifest.json").to_owned())
    }

    pub fn manual_protocol(&self) -> &str {
        self.artifact_text("manual_protocol.md")
    }

    /// The stage protocols a batch reaches. A batch that realizes no artifact
    /// of the corresponding kind does not emit one.
    pub fn assembly_protocol(&self) -> Option<&str> {
        self.optional_artifact_text("assembly_protocol.json")
    }

    pub fn transformation_protocol(&self) -> Option<&str> {
        self.optional_artifact_text("transformation_protocol.json")
    }

    pub fn plating_protocol(&self) -> Option<&str> {
        self.optional_artifact_text("plating_protocol.json")
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }

    fn artifact_text(&self, path: &str) -> &str {
        self.optional_artifact_text(path)
            .expect("Flex bundle contains every unconditional artifact")
    }

    fn optional_artifact_text(&self, path: &str) -> Option<&str> {
        self.artifacts
            .get(path)?
            .text_contents()
            .expect("Flex source artifacts are UTF-8")
            .into()
    }
}

pub fn compile_build(
    protocol: &ProtocolLairProgram,
    profile: &FlexTargetProfile,
) -> Result<FlexBundle, FlexBuildError> {
    Ok(FlexBundle::from_plan(plan_build(protocol, profile)?)?)
}

pub fn emit_program(program: &FlexExecutionPlan) -> Result<FlexBundle, FlexEmissionError> {
    FlexBundle::from_plan(program.clone())
}

fn pretty_json(value: &impl Serialize) -> Result<String, FlexEmissionError> {
    serde_json::to_string_pretty(value)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| FlexEmissionError::Serialization(error.to_string()))
}
