use serde::Serialize;

use crate::{ArtifactBundle, ProtocolLairProgram};

use crate::backend::opentrons_ot2::emit::{
    render_assembly_protocol, render_manual_protocol, render_plating_protocol,
    render_transformation_protocol,
};

use crate::backend::opentrons_ot2::profile::Ot2TargetProfile;

use super::{Ot2BuildError, Ot2EmissionError, Ot2ExecutionPlan, plan_build};

/// Planned OT-2 program together with its emitted artifact package.
#[derive(Clone, Debug)]
pub struct Ot2Bundle {
    manifest: Ot2ExecutionPlan,
    artifacts: ArtifactBundle,
}

impl Ot2Bundle {
    pub(in crate::backend::opentrons_ot2) fn from_plan(
        manifest: Ot2ExecutionPlan,
    ) -> Result<Self, Ot2EmissionError> {
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
                "assembly_protocol.py",
                "text/x-python",
                render_assembly_protocol(&manifest)?,
            )?;
        }
        if !manifest.strains.is_empty() {
            artifacts.insert_text(
                "transformation_protocol.py",
                "text/x-python",
                render_transformation_protocol(&manifest)?,
            )?;
            artifacts.insert_text(
                "plating_protocol.py",
                "text/x-python",
                render_plating_protocol(&manifest)?,
            )?;
        }
        Ok(Self {
            manifest,
            artifacts,
        })
    }

    pub fn manifest(&self) -> &Ot2ExecutionPlan {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, Ot2EmissionError> {
        Ok(self.artifact_text("automation_manifest.json").to_owned())
    }

    pub fn manual_protocol(&self) -> &str {
        self.artifact_text("manual_protocol.md")
    }

    /// The stage protocols a batch reaches. A batch that realizes no artifact
    /// of the corresponding kind does not emit one.
    pub fn assembly_protocol(&self) -> Option<&str> {
        self.optional_artifact_text("assembly_protocol.py")
    }

    pub fn transformation_protocol(&self) -> Option<&str> {
        self.optional_artifact_text("transformation_protocol.py")
    }

    pub fn plating_protocol(&self) -> Option<&str> {
        self.optional_artifact_text("plating_protocol.py")
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }

    fn artifact_text(&self, path: &str) -> &str {
        self.optional_artifact_text(path)
            .expect("OT-2 bundle contains every unconditional artifact")
    }

    fn optional_artifact_text(&self, path: &str) -> Option<&str> {
        self.artifacts
            .get(path)?
            .text_contents()
            .expect("OT-2 source artifacts are UTF-8")
            .into()
    }
}

pub fn compile_build(
    protocol: &ProtocolLairProgram,
    profile: &Ot2TargetProfile,
) -> Result<Ot2Bundle, Ot2BuildError> {
    Ok(Ot2Bundle::from_plan(plan_build(protocol, profile)?)?)
}

pub fn emit_program(program: &Ot2ExecutionPlan) -> Result<Ot2Bundle, Ot2EmissionError> {
    Ot2Bundle::from_plan(program.clone())
}

fn pretty_json(value: &impl Serialize) -> Result<String, Ot2EmissionError> {
    serde_json::to_string_pretty(value)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| Ot2EmissionError::Serialization(error.to_string()))
}
