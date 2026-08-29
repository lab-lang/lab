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

/// Stable adapter identity used by explicit Asset bindings, device plans, and adapter diagnostics.
pub(in crate::backend::opentrons::ot2) const BACKEND: &str = "opentrons.ot2";

pub use crate::backend::opentrons::ot2::backend::{Ot2Backend, Ot2CompileError};
pub use crate::backend::opentrons::ot2::package::{
    DependencyBuildBundle, DependencyBuildError, compile_dependency_build,
};
pub use crate::backend::opentrons::ot2::plan::{
    Ot2AssemblyChemistry, Ot2AssemblyPlan, Ot2BuildError, Ot2Bundle, Ot2EmissionError,
    Ot2ExecutionPlan, Ot2PlanningError, Ot2PlatingPlan, Ot2StrainChemistry, Ot2StrainPlan,
    Ot2TransformationPlan, compile_build, emit_program, plan_build,
};
pub use crate::backend::opentrons::ot2::profile::{Ot2AdapterProfile, Ot2ProfileError};
