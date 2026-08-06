//! OT-2 implementation of the compiler backend contracts.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::backend::{Backend, BackendDescriptor, BackendEmitter, BackendTarget};
use crate::{ArtifactBundle, ProtocolLairProgram};

use super::plan::{Ot2EmissionError, Ot2ExecutionPlan, Ot2PlanningError, emit_program, plan_build};
use super::profile::Ot2TargetProfile;

/// The OT-2 backend bound to one bench. Planning reads every deck, labware, and
/// instrument decision from the profile it carries.
#[derive(Clone, Debug, Default)]
pub struct Ot2Backend {
    profile: Ot2TargetProfile,
}

impl Ot2Backend {
    pub fn new(profile: Ot2TargetProfile) -> Self {
        Self { profile }
    }

    pub fn profile(&self) -> &Ot2TargetProfile {
        &self.profile
    }
}

#[derive(Debug, Error)]
pub enum Ot2CompileError {
    #[error(transparent)]
    Planning(#[from] Ot2PlanningError),
}

impl Backend<ProtocolLairProgram> for Ot2Backend {
    type Program = Ot2ExecutionPlan;
    type Error = Ot2CompileError;

    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            id: "opentrons".into(),
            display_name: "Opentrons Python Protocol API".into(),
            manufacturer: Some("Opentrons".into()),
            targets: vec![BackendTarget {
                id: "ot2".into(),
                display_name: "Opentrons OT-2".into(),
                capabilities: BTreeSet::from([
                    "liquid_transfer".into(),
                    "temperature_control".into(),
                    "python_protocol_api".into(),
                ]),
            }],
        }
    }

    fn compile(&self, protocol: &ProtocolLairProgram) -> Result<Self::Program, Self::Error> {
        Ok(plan_build(protocol, &self.profile)?)
    }
}

impl BackendEmitter<Ot2ExecutionPlan> for Ot2Backend {
    type Error = Ot2EmissionError;

    fn emit(&self, program: &Ot2ExecutionPlan) -> Result<ArtifactBundle, Self::Error> {
        Ok(emit_program(program)?.artifacts().clone())
    }
}

#[cfg(test)]
mod tests {
    use lab_language::compile_module;

    use crate::PortableLairProgram;
    use crate::backend::{Backend, BackendEmitter};

    use super::*;

    #[test]
    fn compiles_only_verified_lair_through_the_backend_contract() {
        let module = compile_module(include_str!(
            "../../../../../examples/opentrons-build/reporter-library.lab"
        ))
        .unwrap();

        let lair = PortableLairProgram::lower(&module).unwrap();
        assert!(lair.ir().contains("design.plasmid"));
        assert!(lair.ir().contains("design.strain"));
        assert!(lair.ir().contains("workflow.realize"));
        assert!(lair.ir().contains("workflow.transform"));
        let protocol = lair.select_protocol().unwrap();
        assert!(protocol.ir().contains("protocol.assemble"));
        assert!(!protocol.ir().contains("workflow."));
        let backend = Ot2Backend::default();
        let program = backend.compile(&protocol).unwrap();
        assert_eq!(backend.descriptor().id, "opentrons");
        assert_eq!(program.assemblies.len(), 2);
        assert_eq!(program.strains.len(), 2);

        let artifacts = backend.emit(&program).unwrap();
        assert_eq!(artifacts.len(), 5);
        assert!(artifacts.get("assembly_protocol.py").is_some());
    }
}
