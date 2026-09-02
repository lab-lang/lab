//! Portable Method definitions, their LAIR operations, and refinement passes.
//!
//! A method refines one semantic Intent operation into a facility-independent Procedure graph.
//! Definitions contain no facility, Asset, CapabilityOffering, MaterialLot, adapter, or schedule
//! identity. LAIR validates them before constructing candidate regions.

mod catalog;
mod definition;
mod id;
pub(crate) mod ir;
pub(crate) mod refinement;
mod registry;
mod standard;

pub use catalog::{METHOD_CATALOG_SCHEMA_VERSION, MethodCatalogDocument, MethodCatalogError};
pub use definition::{
    CapabilityConstraintDefinition, CapabilityRequirementDefinition, MaterialInputDefinition,
    MaterialSourceExpression, MethodDefinition, MethodInput, MethodOutput, MethodParameter,
    MethodSignature, ParameterType, PortType, ProcedureParameterDefinition,
    ProcedureTaskDefinition, ProcedureValue, ProcedureValueExpression, ScalarType,
    ScalarValueExpression, TaskOutput, ValueReference,
};
pub use id::{IntentOperationId, LocalId, LocalIdError};
pub use registry::{MethodDefinitionError, MethodRegistry, MethodRegistryError};
pub use standard::{standard_method_definitions, standard_method_registry};
