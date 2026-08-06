//! OT-2 protocol analysis, validation, allocation, and bundle construction.

mod build;
mod bundle;
mod constraints;
mod error;
mod execution;
mod graph;
mod resources;
mod trace;

pub use build::plan_build;
pub(in crate::backend::opentrons_ot2) use build::plan_selected_build;
pub use bundle::{Ot2Bundle, compile_build, emit_program};
pub use error::{Ot2BuildError, Ot2EmissionError, Ot2PlanningError};
pub use execution::{
    Ot2AssemblyChemistry, Ot2AssemblyPlan, Ot2ExecutionPlan, Ot2PlatingPlan, Ot2StrainChemistry,
    Ot2StrainPlan, Ot2TransformationPlan, Ot2Well,
};
pub(in crate::backend::opentrons_ot2) use graph::protocol_build_graph;

/// Laboratory steps each artifact kind contributes to a build graph node.
const ASSEMBLY_STEPS: [&str; 1] = ["assemble"];
const STRAIN_STEPS: [&str; 4] = ["transform", "recover", "dilute", "plate"];
const TARGET: &str = "opentrons_ot2";
