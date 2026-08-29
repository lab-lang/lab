//! Opentrons OT-2 lowering for exact facility-allocated Procedure invocations.

mod invocation;
mod profile;

/// Stable adapter identity used by explicit Asset bindings, device plans, and adapter diagnostics.
pub(in crate::backend::opentrons::ot2) const BACKEND: &str = "opentrons.ot2";

pub use crate::backend::opentrons::ot2::profile::{Ot2AdapterProfile, Ot2ProfileError};
pub(in crate::backend) use invocation::lower_invocation;
