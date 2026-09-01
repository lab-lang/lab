//! Versioned, device-neutral operational Procedure contracts.
//!
//! Procedure programs are the task-interior portion of LAIR: they name observable material
//! operations and derive semantic facility demands. They remain device-neutral and do not contain
//! facility identities, adapter profiles, or vendor command models.

pub(crate) mod analysis;
pub mod capability;
pub mod feature;
pub mod id;
pub(crate) mod ir;
pub(crate) mod normalization;
pub mod pipetting;
pub mod program;
pub mod quantity;
pub mod thermal;
pub mod vocabulary;

pub use capability::{BindingScope, CapabilityClause, CapabilityFormula};
pub use feature::{ProgramFeature, pipetting_features, thermal_features};
pub use id::{ProcedureLocalId, ProcedureLocalIdError};
pub use pipetting::{
    AspirationStrategy, DispenseStrategy, FluidPathPolicy, LiquidLedger, Location, MaterialInput,
    MaterialOutput, MixTechnique, PipettingConstraints, PipettingProgramV1,
    PipettingProgramValidationError, PipettingStep, TransferTechnique, ValidatedPipettingProgramV1,
    Vessel, VesselRole, VolumeConflict, staged_temperature_envelope,
};
pub use program::{ProcedureProgram, ProcedureProgramValidationError, ValidatedProcedureProgram};
pub use quantity::{
    Duration, Length, QuantityError, Temperature, TemperatureRampRate, TemperatureRange, Volume,
};
pub use thermal::{
    ThermalLoad, ThermalProgramV1, ThermalProgramValidationError, ThermalStage, ThermalStep,
    ValidatedThermalProgramV1,
};
