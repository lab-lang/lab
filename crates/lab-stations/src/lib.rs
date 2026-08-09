//! Lab's instrument-capability implementations over vendor driver crates.
//!
//! Each station type here wraps one vendor session and implements the
//! matching [`lab_instruments`] capability trait, translating between
//! Lab's neutral vocabulary and the vendor's own. The vendor crates know
//! nothing of Lab — they speak their instruments' native types — so this
//! crate is the whole seam: if a vendor someday publishes a good Rust
//! crate of their own, its adapter lands here and ours retires.

mod byonoy;
mod odtc;

pub use byonoy::{ByonoyStation, ByonoyStationError};
pub use odtc::{OdtcStation, OdtcStationError, odtc_thermal_limits};
