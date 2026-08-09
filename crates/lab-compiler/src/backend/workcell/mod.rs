//! Workcell target: one liquid handler, the instruments beside it, and a
//! human carrying labware between them.
//!
//! This module is the containment boundary for every multi-station
//! decision: which after-run work moves to an instrument station, where
//! handoffs appear, and the coordination plan that orders a wave. Machine
//! planning itself stays in each station's own backend — the workcell
//! composes planners, it does not replace them.

mod package;
mod profile;

/// This backend's identity. A target profile declares it, and no other
/// spelling of it exists.
pub(in crate::backend::workcell) const BACKEND: &str = "workcell";

pub use package::{WorkcellBuildError, WorkcellDependencyBuildBundle, compile_dependency_build};
pub use profile::{StationDecl, StationKind, WorkcellProfile, WorkcellProfileError};
