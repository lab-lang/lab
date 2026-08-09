//! OT-2 protocol analysis, validation, allocation, and bundle construction.

mod build;
mod bundle;
mod constraints;
mod error;
mod execution;

pub use crate::backend::opentrons::ot2::plan::build::plan_build;
pub(in crate::backend::opentrons::ot2) use crate::backend::opentrons::ot2::plan::build::plan_selected_build;
pub use crate::backend::opentrons::ot2::plan::bundle::{Ot2Bundle, compile_build, emit_program};
pub use crate::backend::opentrons::ot2::plan::error::{
    Ot2BuildError, Ot2EmissionError, Ot2PlanningError,
};
pub use crate::backend::opentrons::ot2::plan::execution::{
    Ot2AssemblyChemistry, Ot2AssemblyPlan, Ot2ExecutionPlan, Ot2PlatingPlan, Ot2StrainChemistry,
    Ot2StrainPlan, Ot2TransformationPlan, Ot2Well,
};

use crate::ProtocolLairProgram;
use crate::planning::BuildGraph;

/// Robot-neutral build-graph projection, carrying this backend's planning
/// error type.
pub(in crate::backend::opentrons::ot2) fn protocol_build_graph(
    protocol: &ProtocolLairProgram,
) -> Result<BuildGraph, Ot2PlanningError> {
    Ok(crate::backend::graph::protocol_build_graph(protocol)?)
}
