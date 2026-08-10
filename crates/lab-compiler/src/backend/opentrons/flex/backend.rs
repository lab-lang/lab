//! Flex implementation of the compiler backend contracts.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::backend::{Backend, BackendDescriptor, BackendEmitter, BackendTarget};
use crate::{ArtifactBundle, ProtocolLairProgram};

use crate::backend::opentrons::flex::plan::{
    FlexEmissionError, FlexExecutionPlan, FlexPlanningError, emit_program, plan_build,
};
use crate::backend::opentrons::flex::profile::FlexTargetProfile;

/// The Flex backend bound to one bench. Planning reads every deck, labware,
/// and instrument decision from the profile it carries.
#[derive(Clone, Debug, Default)]
pub struct FlexBackend {
    profile: FlexTargetProfile,
}

impl FlexBackend {
    pub fn new(profile: FlexTargetProfile) -> Self {
        Self { profile }
    }

    pub fn profile(&self) -> &FlexTargetProfile {
        &self.profile
    }
}

#[derive(Debug, Error)]
pub enum FlexCompileError {
    #[error(transparent)]
    Planning(#[from] FlexPlanningError),
}

impl Backend<ProtocolLairProgram> for FlexBackend {
    type Program = FlexExecutionPlan;
    type Error = FlexCompileError;

    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            id: "opentrons".into(),
            display_name: "Opentrons JSON protocol".into(),
            manufacturer: Some("Opentrons".into()),
            targets: vec![BackendTarget {
                id: "flex".into(),
                display_name: "Opentrons Flex".into(),
                capabilities: BTreeSet::from([
                    "liquid_transfer".into(),
                    "temperature_control".into(),
                    "thermocycler".into(),
                    "gripper".into(),
                    "json_protocol".into(),
                ]),
            }],
        }
    }

    fn compile(&self, protocol: &ProtocolLairProgram) -> Result<Self::Program, Self::Error> {
        Ok(plan_build(protocol, &self.profile)?)
    }
}

impl BackendEmitter<FlexExecutionPlan> for FlexBackend {
    type Error = FlexEmissionError;

    fn emit(&self, program: &FlexExecutionPlan) -> Result<ArtifactBundle, Self::Error> {
        Ok(emit_program(program)?.artifacts().clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{Backend, BackendEmitter};
    use crate::test_support::golden_gate_protocol;

    use crate::backend::opentrons::flex::backend::*;

    #[test]
    fn compiles_the_example_and_emits_json_protocols() {
        let protocol = golden_gate_protocol();
        let backend = FlexBackend::default();
        let program = backend.compile(&protocol).unwrap();
        assert_eq!(backend.descriptor().id, "opentrons");
        assert_eq!(backend.descriptor().targets[0].id, "flex");
        assert_eq!(program.assemblies.len(), 2);
        // One plasmid feeds two chassis, so four strains come from two
        // assemblies rather than one strain per assembly.
        assert_eq!(program.strains.len(), 4);

        let artifacts = backend.emit(&program).unwrap();
        assert_eq!(artifacts.len(), 6);
        for path in [
            "assembly_protocol.json",
            "transformation_protocol.json",
            "plating_protocol.json",
            "automation_manifest.json",
            "manual_protocol.typ",
            "lab-style.typ",
        ] {
            assert!(artifacts.get(path).is_some(), "missing {path}");
        }

        let assembly: serde_json::Value = serde_json::from_str(
            artifacts
                .get("assembly_protocol.json")
                .unwrap()
                .text_contents()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(assembly["$otSharedSchema"], "#/protocol/schemas/8");
        assert_eq!(assembly["robot"]["model"], "OT-3 Standard");
    }
}
