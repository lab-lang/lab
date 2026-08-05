//! OpenTrons OT-2 specialization for plasmid build workflows.
//!
//! This module is the containment boundary for every OT-2-specific decision:
//! accepted biological operation sequences, target IR, deck and well
//! allocation, Opentrons API/labware choices, Python emission, and packaged
//! human instructions.

mod emit;
mod ir;
mod lower;
mod package;
mod plan;

pub use ir::{
    Ot2BuildArtifact, Ot2BuildIr, Ot2BuildIrError, Ot2BuildRecipe, Ot2ConstructPlan,
    Ot2ExecutionPlan, Ot2PlatingPlan, Ot2TransformationPlan,
};
pub use lower::{Ot2LoweringError, lower_build};
pub use package::{DependencyBuildBundle, DependencyBuildError, compile_dependency_build};
pub use plan::{
    Ot2BuildError, Ot2Bundle, Ot2EmissionError, Ot2PlanningError, compile_build, emit_program,
    plan_build,
};

use lab_language::CheckedModule;
use std::collections::BTreeSet;
use thiserror::Error;

use super::{Backend, BackendDescriptor, BackendEmitter, BackendTarget, TargetConstraintError};
use crate::ArtifactBundle;

#[derive(Clone, Copy, Debug, Default)]
pub struct Ot2Backend;

#[derive(Debug, Error)]
pub enum Ot2CompileError {
    #[error(transparent)]
    Lowering(#[from] Ot2LoweringError),
    #[error(transparent)]
    Planning(#[from] Ot2PlanningError),
}

impl Backend<CheckedModule> for Ot2Backend {
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

    fn compile(&self, module: &CheckedModule) -> Result<Self::Program, Self::Error> {
        Ok(plan_build(&lower_build(module)?)?)
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

    use super::*;
    use crate::Compiler;

    #[test]
    fn compiles_checked_modules_through_the_backend_contract() {
        let module = compile_module(include_str!(
            "../../../../../examples/opentrons-build/reporter-library.lab"
        ))
        .unwrap();

        let compilation = Compiler.compile_backend(&module, &Ot2Backend).unwrap();
        assert_eq!(compilation.descriptor().id, "opentrons");
        assert_eq!(compilation.program().constructs.len(), 2);

        let artifacts = Ot2Backend.emit(compilation.program()).unwrap();
        assert_eq!(artifacts.len(), 5);
        assert!(artifacts.get("assembly_protocol.py").is_some());
    }
}
