//! Errors from parsing and validating OT-2 adapter configuration.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Ot2ProfileError {
    #[error("failed to parse OT-2 adapter profile: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("{context} names deck slot '{slot}', which an OT-2 does not address")]
    UnknownSlot { context: String, slot: String },
    #[error(
        "{context} claims deck slot '{slot}', which the installed thermocycler already occupies"
    )]
    ThermocyclerSlot { context: String, slot: String },
    #[error("deck slot '{slot}' is claimed by both {first} and {second} during {stage}")]
    SlotConflict {
        stage: &'static str,
        slot: String,
        first: String,
        second: String,
    },
    #[error("{context} must declare at least one deck slot")]
    NoSlots { context: String },
    #[error("OT-2 technique calibration '{parameter}' {message}")]
    InvalidTechnique {
        parameter: &'static str,
        message: &'static str,
    },
}
