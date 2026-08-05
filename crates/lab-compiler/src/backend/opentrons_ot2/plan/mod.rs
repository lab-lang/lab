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
pub use execution::{Ot2ConstructPlan, Ot2ExecutionPlan, Ot2PlatingPlan, Ot2TransformationPlan};
pub(in crate::backend::opentrons_ot2) use graph::protocol_build_graph;

const API_LEVEL: &str = "2.21";
const REACTION_VOLUME_UL: u16 = 20;
const SUPPORTED_STEPS: [&str; 5] = ["assemble", "transform", "recover", "dilute", "plate"];
const TARGET: &str = "opentrons_ot2";
