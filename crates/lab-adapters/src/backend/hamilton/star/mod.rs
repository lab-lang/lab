//! Hamilton STAR/STARlet lowering for exact facility-allocated Procedure invocations.
//!
//! The emitted `lab.star-run.v0` document is the review boundary: `lab run` replays its frames verbatim, adding only command ids.

pub mod catalog;
mod emit;
mod invocation;
mod plan;
pub mod profile;

/// Stable adapter identity used by explicit Asset bindings, device plans, and adapter diagnostics.
pub(in crate::backend::hamilton::star) const BACKEND: &str = "hamilton.star";

pub use crate::backend::hamilton::star::profile::{StarAdapterProfile, StarProfileError};

pub(in crate::backend) use invocation::lower_invocation;
