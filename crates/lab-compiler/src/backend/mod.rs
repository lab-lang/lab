//! Compiler backend contracts, the planning every backend shares, and the
//! concrete execution targets.
//!
//! Three layers live here. The contracts — [`Backend`], [`BackendEmitter`],
//! [`BackendDescriptor`], [`TargetConstraintError`] — say what a backend is.
//! The modules beside them are planning that holds for any liquid handler and
//! names no robot: provenance analysis over Protocol LAIR, projection into the
//! target-independent build graph, SBS plate geometry and well allocation, the
//! labware groupings every bench profile declares, and the rendering of a
//! dependency-driven build. Backend identity enters that planning only as a
//! parameter, so a capacity error names the machine that planned the build.
//!
//! Below both sits one module per vendor family, [`opentrons`], holding one
//! module per machine. A target implementation owns the selection from
//! verified LAIR into a target IR, target validation and resource planning,
//! and concrete emitters. The language frontend and generic output renderers
//! deliberately do not depend on any target module.

mod constraints;
mod descriptor;
mod error;
mod graph;
pub mod hamilton;
pub mod opentrons;
mod package;
mod profile;
mod resources;
mod trace;
mod traits;

pub use constraints::TargetConstraintError;
pub use descriptor::{BackendDescriptor, BackendTarget};
pub use traits::{Backend, BackendEmitter};
