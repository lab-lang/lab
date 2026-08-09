//! STAR protocol analysis, validation, allocation, and run lowering.

mod build;
mod choreograph;
mod constraints;
mod error;
mod execution;
mod liquids;

use crate::ProtocolLairProgram;
pub use crate::backend::hamilton::star::plan::build::plan_build;
pub(in crate::backend) use crate::backend::hamilton::star::plan::build::plan_selected_build;
pub use crate::backend::hamilton::star::plan::error::{
    StarBuildError, StarEmissionError, StarPlanningError,
};
pub use crate::backend::hamilton::star::plan::execution::{
    ChannelLiquid, ManualStep, SourceFill, StarAssemblyChemistry, StarAssemblyPlan,
    StarExecutionPlan, StarOperation, StarPlatingPlan, StarRunPlan, StarStrainChemistry,
    StarStrainPlan, StarTransformationPlan, StarWell, ThermalRequirement, TipClass,
    TipPickupPosition,
};
use crate::planning::BuildGraph;

/// Robot-neutral build-graph projection, carrying this backend's planning
/// error type.
pub(in crate::backend::hamilton::star) fn protocol_build_graph(
    protocol: &ProtocolLairProgram,
) -> Result<BuildGraph, StarPlanningError> {
    Ok(crate::backend::graph::protocol_build_graph(protocol)?)
}
