//! RDF-free semantic capability contracts shared across the Lab ecosystem.
//!
//! Facility RDF, mutable compiler IR, adapter implementation details, and runtime state have
//! different owners. This crate contains only the small value vocabulary that crosses those
//! boundaries: semantic IRIs, qualification and control requirements, exact scalar values, and
//! property constraints.

mod constraint;
mod iri;
mod qualification;
mod value;

pub use constraint::{ConstraintEvaluationError, ConstraintRelation, PropertyConstraint};
pub use iri::{
    AbsoluteIri, CapabilityKind, IriError, MethodId, OperationId, ProcedureContractId,
    ProcedureImplementationId, PropertyKind, UnitIri, is_absolute_iri,
};
pub use qualification::{
    ControlMode, ControlModeError, QualificationLevel, QualificationLevelError,
};
pub use value::{
    ExactDecimal, ExactInteger, NumberParseError, PropertyValue, PropertyValueError, ScalarValue,
};
