//! Portable, RDF-free method definitions shared by Lab frontends and the compiler.
//!
//! A method refines one semantic Intent operation into a facility-independent Procedure graph.
//! Definitions contain no facility, Asset, CapabilityOffering, MaterialLot, adapter, or schedule
//! identity. The compiler validates them before constructing LAIR candidate regions.

mod definition;
mod id;
mod registry;

pub use definition::{
    CapabilityConstraintDefinition, CapabilityRequirementDefinition, ConstraintValue,
    MethodDefinition, MethodInput, MethodOutput, MethodParameter, MethodSignature, PortType,
    ProcedureTaskDefinition, ScalarType, TaskOutput, ValueReference,
};
pub use id::{IntentOperationId, LocalId, LocalIdError};
pub use registry::{MethodDefinitionError, MethodRegistry, MethodRegistryError};
