//! Opentrons Flex specialization for plasmid build workflows.
//!
//! This module is the containment boundary for every Flex-specific decision:
//! accepted Protocol operations, deck and well allocation, Flex labware and
//! instrument choices, JSON protocol emission through `lab-opentrons-protocol`,
//! and packaged human instructions.

mod backend;
mod emit;
mod package;
mod plan;
mod profile;

/// This backend's identity. A target profile declares it, planning stamps it
/// into every execution plan and target-constraint error, and no other
/// spelling of it exists.
pub(in crate::backend::opentrons::flex) const BACKEND: &str = "opentrons.flex";

pub use crate::backend::opentrons::flex::backend::{FlexBackend, FlexCompileError};
pub use crate::backend::opentrons::flex::package::{
    FlexDependencyBuildBundle, FlexDependencyBuildError, compile_dependency_build,
};
pub use crate::backend::opentrons::flex::plan::{
    FlexAssemblyChemistry, FlexAssemblyPlan, FlexBuildError, FlexBundle, FlexEmissionError,
    FlexExecutionPlan, FlexPlanningError, FlexPlatingPlan, FlexStrainChemistry, FlexStrainPlan,
    FlexTransformationPlan, compile_build, emit_program, plan_build,
};
pub use crate::backend::opentrons::flex::profile::{FlexProfileError, FlexTargetProfile};
