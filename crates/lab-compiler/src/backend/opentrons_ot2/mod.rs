//! OpenTrons OT-2 specialization for plasmid build workflows.
//!
//! This module is the containment boundary for every OT-2-specific decision:
//! accepted Protocol operations, deck and well
//! allocation, Opentrons API/labware choices, Python emission, and packaged
//! human instructions.

mod backend;
mod emit;
mod package;
mod plan;
mod profile;

pub use backend::{Ot2Backend, Ot2CompileError};
pub use package::{DependencyBuildBundle, DependencyBuildError, compile_dependency_build};
pub use plan::{
    Ot2AssemblyChemistry, Ot2AssemblyPlan, Ot2BuildError, Ot2Bundle, Ot2EmissionError,
    Ot2ExecutionPlan, Ot2PlanningError, Ot2PlatingPlan, Ot2StrainChemistry, Ot2StrainPlan,
    Ot2TransformationPlan, compile_build, emit_program, plan_build,
};
pub use profile::{Ot2ProfileError, Ot2TargetProfile};
