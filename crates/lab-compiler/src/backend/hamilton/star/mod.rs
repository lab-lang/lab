//! Hamilton STAR/STARlet specialization for plasmid build workflows.
//!
//! This module is the containment boundary for every STAR-specific decision:
//! accepted Protocol operations, the vendored carrier catalog and deck
//! coordinates, channel batching and liquid-height derivation, firmware
//! frame emission through `hamilton-star`, and the packaged human
//! instructions. The emitted `lab.star-run.v0` documents are the review
//! boundary: `lab run` replays their frames verbatim, adding only command
//! ids.

mod backend;
pub mod catalog;
mod emit;
mod package;
pub mod plan;
pub mod profile;

/// This backend's identity. A target profile declares it, planning stamps it
/// into every execution plan and target-constraint error, and no other
/// spelling of it exists.
pub(in crate::backend::hamilton::star) const BACKEND: &str = "hamilton.star";

pub use crate::backend::hamilton::star::backend::{StarBackend, StarCompileError};
pub use crate::backend::hamilton::star::emit::{RunStep, StarBundle, compile_build, emit_program};
pub use crate::backend::hamilton::star::package::{
    StarDependencyBuildBundle, StarDependencyBuildError, compile_dependency_build,
};
pub use crate::backend::hamilton::star::plan::{
    ManualStep, StarBuildError, StarEmissionError, StarExecutionPlan, StarPlanningError, plan_build,
};
pub use crate::backend::hamilton::star::profile::{StarProfileError, StarTargetProfile};
