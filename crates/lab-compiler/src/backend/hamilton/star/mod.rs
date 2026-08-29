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
mod invocation;
mod package;
pub mod plan;
pub mod profile;

/// Stable adapter identity used by explicit Asset bindings, device plans, and adapter diagnostics.
pub(in crate::backend::hamilton::star) const BACKEND: &str = "hamilton.star";

pub use crate::backend::hamilton::star::backend::{StarBackend, StarCompileError};
pub use crate::backend::hamilton::star::emit::{RunStep, StarBundle, compile_build, emit_program};
pub use crate::backend::hamilton::star::package::{
    StarDependencyBuildBundle, StarDependencyBuildError, compile_dependency_build,
};
pub use crate::backend::hamilton::star::plan::{
    ManualStep, StarBuildError, StarEmissionError, StarExecutionPlan, StarPlanningError, plan_build,
};
pub use crate::backend::hamilton::star::profile::{StarAdapterProfile, StarProfileError};

pub(in crate::backend) use invocation::lower_invocation;
