//! Backend-independent vocabulary for reporting lowering constraints.
//!
//! An adapter specialization supplies implementation, operation, resource, and parameter names; the compiler infrastructure owns only the general shapes of constraint failure.

use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum AdapterConstraintError {
    #[error(
        "adapter '{adapter}' does not support the operation sequence for '{subject}': expected {expected:?}, found {found:?}"
    )]
    UnsupportedOperationSequence {
        adapter: String,
        subject: String,
        expected: Vec<String>,
        found: Vec<String>,
    },
    #[error(
        "adapter '{adapter}' requires parameter '{parameter}' for '{subject}' to be between {minimum} and {maximum}, found {found}"
    )]
    ParameterOutOfRange {
        adapter: String,
        subject: String,
        parameter: String,
        minimum: u64,
        maximum: u64,
        found: u64,
    },
    #[error("adapter '{adapter}' requires uniform values for {parameters:?} across '{subject}'")]
    NonUniformParameters {
        adapter: String,
        subject: String,
        parameters: Vec<String>,
    },
    #[error(
        "adapter '{adapter}' capacity exceeded during '{operation}' for '{subject}' resource '{resource}': required {required} {unit}, capacity {capacity} {unit}"
    )]
    CapacityExceeded {
        adapter: String,
        operation: String,
        subject: String,
        resource: String,
        required: u64,
        capacity: u64,
        unit: String,
    },
}

#[cfg(test)]
mod tests {
    use crate::backend::constraints::AdapterConstraintError;

    #[test]
    fn capacity_diagnostics_do_not_encode_a_protocol() {
        let error = AdapterConstraintError::CapacityExceeded {
            adapter: "example_adapter".into(),
            operation: "example_operation".into(),
            subject: "example_subject".into(),
            resource: "reaction_volume".into(),
            required: 24,
            capacity: 20,
            unit: "uL".into(),
        };
        assert_eq!(
            error.to_string(),
            "adapter 'example_adapter' capacity exceeded during 'example_operation' for 'example_subject' resource 'reaction_volume': required 24 uL, capacity 20 uL"
        );
    }
}
