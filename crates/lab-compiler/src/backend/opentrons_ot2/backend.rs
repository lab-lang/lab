//! OT-2 implementation of the compiler backend contracts.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::backend::{Backend, BackendDescriptor, BackendEmitter, BackendTarget};
use crate::{ArtifactBundle, ProtocolLairProgram};

use super::plan::{Ot2EmissionError, Ot2ExecutionPlan, Ot2PlanningError, emit_program, plan_build};

#[derive(Clone, Copy, Debug, Default)]
pub struct Ot2Backend;

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
        Ok(plan_build(protocol)?)
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
        assert!(lair.ir().contains("workflow.realize"));
        assert!(lair.ir().contains("workflow.transform"));
        let protocol = lair.select_protocol().unwrap();
        assert!(protocol.ir().contains("protocol.assemble"));
        assert!(!protocol.ir().contains("workflow."));
        let program = Ot2Backend.compile(&protocol).unwrap();
        assert_eq!(Ot2Backend.descriptor().id, "opentrons");
        assert_eq!(program.constructs.len(), 2);

        let artifacts = Ot2Backend.emit(&program).unwrap();
        assert_eq!(artifacts.len(), 5);
        assert!(artifacts.get("assembly_protocol.py").is_some());
    }
}
