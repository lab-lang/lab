use thiserror::Error;

/// An invalid exact physical quantity in a canonical Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum QuantityError {
    #[error("`{value}` is not a finite base-10 decimal quantity")]
    InvalidNumber { value: String },
    #[error("quantity unit `{found}` must be `{expected}`")]
    WrongUnit { expected: String, found: String },
    #[error("quantity value must be numeric")]
    NonNumeric,
    #[error("quantity must be greater than zero, found `{value}`")]
    NonPositive { value: String },
    #[error("quantity must not be negative, found `{value}`")]
    Negative { value: String },
    #[error("temperature range minimum `{minimum}` exceeds maximum `{maximum}`")]
    ReversedTemperatureRange { minimum: String, maximum: String },
}
