//! Backend-independent vocabulary for reporting lowering constraints.
//!
//! A specialization supplies target, operation, resource, and parameter names;
//! the compiler infrastructure owns only the general shapes of constraint
//! failure.

use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TargetConstraintError {
    #[error(
        "target '{target}' does not support the operation sequence for '{subject}': expected {expected:?}, found {found:?}"
    )]
    UnsupportedOperationSequence {
        target: String,
        subject: String,
        expected: Vec<String>,
        found: Vec<String>,
    },
    #[error(
        "target '{target}' requires parameter '{parameter}' for '{subject}' to be between {minimum} and {maximum}, found {found}"
    )]
    ParameterOutOfRange {
        target: String,
        subject: String,
        parameter: String,
        minimum: u64,
        maximum: u64,
        found: u64,
    },
    #[error("target '{target}' requires uniform values for {parameters:?} across '{subject}'")]
    NonUniformParameters {
        target: String,
        subject: String,
        parameters: Vec<String>,
    },
    #[error(
        "target '{target}' capacity exceeded during '{operation}' for '{subject}' resource '{resource}': required {required} {unit}, capacity {capacity} {unit}"
    )]
    CapacityExceeded {
        target: String,
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
    use super::TargetConstraintError;

    #[test]
    fn capacity_diagnostics_do_not_encode_a_protocol() {
        let error = TargetConstraintError::CapacityExceeded {
            target: "example_target".into(),
            operation: "example_operation".into(),
            subject: "example_subject".into(),
            resource: "reaction_volume".into(),
            required: 24,
            capacity: 20,
            unit: "uL".into(),
        };
        assert_eq!(
            error.to_string(),
            "target 'example_target' capacity exceeded during 'example_operation' for 'example_subject' resource 'reaction_volume': required 24 uL, capacity 20 uL"
        );
    }
}
