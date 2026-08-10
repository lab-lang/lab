//! Lab's instrument capabilities: the traits the runtime holds, the
//! neutral vocabulary compiled documents speak, and the station types
//! that implement the traits over vendor driver crates.
//!
//! This crate is Lab-internal. Vendor driver crates know nothing of it —
//! they speak their instruments' native types — and each station type
//! here wraps one vendor session and translates between the vendor's
//! vocabulary and the neutral one. The stations are the whole seam: if a
//! vendor someday publishes a good Rust crate of their own, its adapter
//! lands here and ours retires. The compiler uses only the data model (a
//! `lab.thermocycle-run.v0` document embeds a [`ThermalProfile`]); only
//! the runtime uses the traits and stations. Nothing here may depend on
//! the rest of the toolchain.
//!
//! Each trait is one *capability*, not one device category. A device
//! implements whichever capabilities its hardware offers, and a machine
//! with several — a liquid handler carrying an on-deck cycler — composes
//! several. The split between what gets a trait and what does not is the
//! shape of the work, not the kind of machine: **command-shaped** work
//! (run this thermal profile, read this plate) is small closed data that
//! fits a document plus a parametric call, and lives here;
//! **program-shaped** work (liquid choreography) is compiled per backend
//! into vendor programs, and its interface is the run-document format
//! plus a replaying executor, never a hardware API.
//!
//! Capability asymmetries are modeled, not hidden. A thermocycler either
//! reports run progress or it does not ([`Thermocycler::progress`] returns
//! `None` honestly); a reader either measures a wavelength or rejects it.
//! The one synchronization primitive every device supports — and the only
//! one a compiled plan may rely on — is awaiting profile completion.

mod plate;
mod thermal;

pub use plate::{
    ByonoyStation, ByonoyStationError, MeasurementUnit, PlateData, PlateDataError, PlateReader,
    ReaderCapabilities, WavelengthSupport,
};
pub use thermal::{
    OdtcStation, OdtcStationError, ProfileProgress, RunHandle, SensorReading, ThermalLimits,
    ThermalProfile, ThermalProfileError, ThermalReadings, ThermalStage, ThermalStep, Thermocycler,
    odtc_thermal_limits,
};
