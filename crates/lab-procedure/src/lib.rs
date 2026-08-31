//! Versioned, device-neutral operational Procedure contracts.
//!
//! This crate sits between scientific Method refinement and device implementation. Programs name
//! observable material operations and derive their semantic facility demands without importing RDF,
//! compiler IR, facility identities, adapter profiles, or vendor command models.

mod capability;
mod feature;
mod id;
mod pipetting;
mod program;
mod quantity;
mod thermal;
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
