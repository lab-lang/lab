//! Errors from parsing and validating a Flex target profile.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlexProfileError {
    #[error("failed to parse Flex target profile: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("target profile declares backend '{found}', but this backend is '{expected}'")]
    WrongBackend {
        expected: &'static str,
        found: String,
    },
    #[error(
        "the {instrument} instrument names pipette '{model}', which is not a Flex pipette; Flex pipettes are p50_single_flex, p50_multi_flex, p1000_single_flex, p1000_multi_flex, and p1000_96"
    )]
    UnknownPipette {
        instrument: &'static str,
        model: String,
    },
    #[error(
        "the {instrument} instrument names mount '{mount}', but a Flex pipette mounts 'left' or 'right'"
    )]
    UnknownMount {
        instrument: &'static str,
        mount: String,
    },
    #[error("both instruments claim the '{mount}' mount, and a mount holds one pipette")]
    SharedMount { mount: String },
    #[error("the {module} declares model '{found}', but this backend drives the '{expected}'")]
    WrongModuleModel {
        module: &'static str,
        expected: &'static str,
        found: String,
    },
    #[error(
        "the trash bin names area '{found}', which is not a movable-trash area; trash bins install in columns 1 and 3 as movableTrash<row><column>"
    )]
    UnknownTrashArea { found: String },
    #[error("the temperature module names slot '{slot}', but its caddy installs in column 1 or 3")]
    TemperatureModuleColumn { slot: String },
    #[error("{context} names deck slot '{slot}', which a Flex does not address")]
    UnknownSlot { context: String, slot: String },
    #[error("deck slot '{slot}' is claimed by both {first} and {second} during {stage}")]
    SlotConflict {
        stage: &'static str,
        slot: String,
        first: String,
        second: String,
    },
    #[error("{context} must declare at least one deck slot")]
    NoSlots { context: String },
}
