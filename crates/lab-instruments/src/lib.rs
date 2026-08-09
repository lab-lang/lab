//! Generic interfaces for standalone bench instruments.
//!
//! A workcell build compiles device-neutral run documents — a thermal
//! profile, a plate acquisition — and the runtime drives whichever
//! instrument a station's kind names. The traits here are that seam: each
//! driver crate implements one, and the runtime holds the trait, never a
//! vendor.
//!
//! The traits cover standalone instruments only. An instrument integrated
//! into a liquid handler's own protocol (the Flex's on-deck thermocycler,
//! driven in-protocol through its JSON commands) is that backend's concern
//! and never passes through here.
//!
//! Capability asymmetries are modeled, not hidden. A thermocycler either
//! reports run progress or it does not ([`Thermocycler::progress`] returns
//! `None` honestly); a reader either measures a wavelength or rejects it.
//! The one synchronization primitive every device supports — and the only
//! one a compiled plan may rely on — is awaiting profile completion.

mod plate;
mod thermal;

pub use plate::{
    MeasurementUnit, PlateData, PlateDataError, PlateReader, ReaderCapabilities, WavelengthSupport,
};
pub use thermal::{
    ProfileProgress, RunHandle, SensorReading, ThermalLimits, ThermalProfile, ThermalProfileError,
    ThermalReadings, ThermalStage, ThermalStep, Thermocycler,
};
