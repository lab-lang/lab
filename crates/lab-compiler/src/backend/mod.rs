//! Compiler backend contracts and concrete execution-target implementations.
//!
//! Target implementations own the lowering from checked Lab modules into a
//! target IR, target validation and resource planning, and concrete emitters.
//! The language frontend and generic output renderers deliberately do not
//! depend on any target module.

mod constraints;
mod descriptor;
pub mod opentrons_ot2;
mod traits;

pub use constraints::TargetConstraintError;
pub use descriptor::{BackendDescriptor, BackendTarget};
pub use traits::{Backend, BackendEmitter};
