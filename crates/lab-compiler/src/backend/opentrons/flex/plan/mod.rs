//! Flex protocol analysis, validation, allocation, and bundle construction.

mod build;
mod bundle;
mod constraints;
mod error;
mod execution;

pub use crate::backend::opentrons::flex::plan::build::plan_build;
pub(in crate::backend::opentrons::flex) use crate::backend::opentrons::flex::plan::build::plan_selected_build;
pub use crate::backend::opentrons::flex::plan::bundle::{FlexBundle, compile_build, emit_program};
pub use crate::backend::opentrons::flex::plan::error::{
    FlexBuildError, FlexEmissionError, FlexPlanningError,
};
pub use crate::backend::opentrons::flex::plan::execution::{
    FlexAssemblyChemistry, FlexAssemblyPlan, FlexExecutionPlan, FlexPlatingPlan,
    FlexStrainChemistry, FlexStrainPlan, FlexTransformationPlan, FlexWell,
};

use crate::ProtocolLairProgram;
use crate::planning::BuildGraph;

/// Robot-neutral build-graph projection, carrying this backend's planning
/// error type.
pub(in crate::backend::opentrons::flex) fn protocol_build_graph(
    protocol: &ProtocolLairProgram,
) -> Result<BuildGraph, FlexPlanningError> {
    Ok(crate::backend::graph::protocol_build_graph(protocol)?)
}
