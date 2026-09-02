//! Opentrons Flex lowering for exact facility-allocated Procedure invocations.

mod invocation;
pub mod profile;

/// Stable adapter identity used by explicit Asset bindings, device plans, and adapter diagnostics.
pub(in crate::backend::opentrons::flex) const BACKEND: &str = "opentrons.flex";

pub use crate::backend::opentrons::flex::profile::{FlexAdapterProfile, FlexProfileError};

pub(in crate::backend) use invocation::lower_invocation;
