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

/// This backend's identity. A target profile declares it, planning stamps it
/// into every execution plan and target-constraint error, and no other
/// spelling of it exists.
pub(in crate::backend::opentrons_ot2) const BACKEND: &str = "opentrons.ot2";

pub use backend::{Ot2Backend, Ot2CompileError};
pub use package::{DependencyBuildBundle, DependencyBuildError, compile_dependency_build};
pub use plan::{
    Ot2AssemblyChemistry, Ot2AssemblyPlan, Ot2BuildError, Ot2Bundle, Ot2EmissionError,
    Ot2ExecutionPlan, Ot2PlanningError, Ot2PlatingPlan, Ot2StrainChemistry, Ot2StrainPlan,
    Ot2TransformationPlan, compile_build, emit_program, plan_build,
};
pub use profile::{Ot2ProfileError, Ot2TargetProfile};
