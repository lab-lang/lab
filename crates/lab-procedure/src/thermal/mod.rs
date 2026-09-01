//! Canonical, device-neutral thermal-program contracts and their derived semantics.

mod capabilities;
mod features;
mod program;
mod validation;

pub use program::{ThermalLoad, ThermalProgramV1, ThermalStage, ThermalStep};
pub use validation::{ThermalProgramValidationError, ValidatedThermalProgramV1};

pub(crate) use features::required_features;
