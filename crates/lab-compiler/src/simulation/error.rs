use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SimulationError {
    #[error("execution operation '{operation}' requires unavailable value '{value}'")]
    UnavailableValue { operation: String, value: String },
}
