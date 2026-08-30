//! Versioned, device-neutral operational Procedure contracts.
//!
//! This crate sits between scientific Method refinement and device implementation. Programs name
//! observable material operations and derive their semantic facility demands without importing RDF,
//! compiler IR, facility identities, adapter profiles, or vendor command models.

mod capability;
mod id;
mod pipetting;
mod program;
mod quantity;
mod thermal;
pub mod vocabulary;

pub use capability::{BindingScope, CapabilityClause, CapabilityFormula};
pub use id::{ProcedureLocalId, ProcedureLocalIdError};
pub use pipetting::{
    FluidPathPolicy, Location, MaterialInput, MaterialOutput, PipettingConstraints,
    PipettingProgramV1, PipettingProgramValidationError, PipettingStep,
    ValidatedPipettingProgramV1, Vessel, VesselRole,
};
pub use program::{ProcedureProgram, ProcedureProgramValidationError, ValidatedProcedureProgram};
pub use quantity::{
    Duration, QuantityError, Temperature, TemperatureRampRate, TemperatureRange, Volume,
};
pub use thermal::{
    ThermalLoad, ThermalProgramV1, ThermalProgramValidationError, ThermalStage, ThermalStep,
    ValidatedThermalProgramV1,
};
