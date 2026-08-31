use std::fmt::{self, Display};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The ordered SBOLInventory qualification ladder used by semantic requirements.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum QualificationLevel {
    #[serde(rename = "https://sbol.io/ns/facility#Discovered")]
    Discovered,
    #[serde(rename = "https://sbol.io/ns/facility#Described")]
    Described,
    #[serde(rename = "https://sbol.io/ns/facility#Plannable")]
    Plannable,
    #[serde(rename = "https://sbol.io/ns/facility#Simulatable")]
    Simulatable,
    #[serde(rename = "https://sbol.io/ns/facility#Executable")]
    Executable,
    #[serde(rename = "https://sbol.io/ns/facility#Qualified")]
    Qualified,
}

impl QualificationLevel {
    /// The normative SBOLInventory IRI serialized at external boundaries.
    pub const fn iri(self) -> &'static str {
        match self {
            Self::Discovered => "https://sbol.io/ns/facility#Discovered",
            Self::Described => "https://sbol.io/ns/facility#Described",
            Self::Plannable => "https://sbol.io/ns/facility#Plannable",
            Self::Simulatable => "https://sbol.io/ns/facility#Simulatable",
            Self::Executable => "https://sbol.io/ns/facility#Executable",
            Self::Qualified => "https://sbol.io/ns/facility#Qualified",
        }
    }

    /// Whether an observed qualification satisfies this minimum level.
    pub const fn is_satisfied_by(self, observed: Self) -> bool {
        observed as u8 >= self as u8
    }
}

impl Display for QualificationLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.iri())
    }
}

impl TryFrom<&str> for QualificationLevel {
    type Error = QualificationLevelError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "https://sbol.io/ns/facility#Discovered" => Ok(Self::Discovered),
            "https://sbol.io/ns/facility#Described" => Ok(Self::Described),
            "https://sbol.io/ns/facility#Plannable" => Ok(Self::Plannable),
            "https://sbol.io/ns/facility#Simulatable" => Ok(Self::Simulatable),
            "https://sbol.io/ns/facility#Executable" => Ok(Self::Executable),
            "https://sbol.io/ns/facility#Qualified" => Ok(Self::Qualified),
            _ => Err(QualificationLevelError {
                value: value.to_owned(),
            }),
        }
    }
}

/// An IRI outside the closed qualification ladder.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("`{value}` is not an SBOLInventory qualification level")]
pub struct QualificationLevelError {
    value: String,
}

/// The closed SBOLInventory control-mode vocabulary.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum ControlMode {
    #[serde(rename = "https://sbol.io/ns/facility#UnspecifiedControl")]
    Unspecified,
    #[serde(rename = "https://sbol.io/ns/facility#ManualControl")]
    Manual,
    #[serde(rename = "https://sbol.io/ns/facility#ReviewedFileControl")]
    ReviewedFile,
    #[serde(rename = "https://sbol.io/ns/facility#VendorSessionControl")]
    VendorSession,
    #[serde(rename = "https://sbol.io/ns/facility#ApiControl")]
    Api,
    #[serde(rename = "https://sbol.io/ns/facility#SiLA2Control")]
    Sila2,
    #[serde(rename = "https://sbol.io/ns/facility#OpcUaControl")]
    OpcUa,
}

impl ControlMode {
    /// Every operational mode; descriptive `UnspecifiedControl` is deliberately excluded.
    pub const CONCRETE: [Self; 6] = [
        Self::Manual,
        Self::ReviewedFile,
        Self::VendorSession,
        Self::Api,
        Self::Sila2,
        Self::OpcUa,
    ];

    /// The normative SBOLInventory IRI serialized at external boundaries.
    pub const fn iri(self) -> &'static str {
        match self {
            Self::Unspecified => "https://sbol.io/ns/facility#UnspecifiedControl",
            Self::Manual => "https://sbol.io/ns/facility#ManualControl",
            Self::ReviewedFile => "https://sbol.io/ns/facility#ReviewedFileControl",
            Self::VendorSession => "https://sbol.io/ns/facility#VendorSessionControl",
            Self::Api => "https://sbol.io/ns/facility#ApiControl",
            Self::Sila2 => "https://sbol.io/ns/facility#SiLA2Control",
            Self::OpcUa => "https://sbol.io/ns/facility#OpcUaControl",
        }
    }

    /// Whether the mode names an operational control path.
    pub const fn is_concrete(self) -> bool {
        !matches!(self, Self::Unspecified)
    }
}

impl Display for ControlMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.iri())
    }
}

impl TryFrom<&str> for ControlMode {
    type Error = ControlModeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "https://sbol.io/ns/facility#UnspecifiedControl" => Ok(Self::Unspecified),
            "https://sbol.io/ns/facility#ManualControl" => Ok(Self::Manual),
            "https://sbol.io/ns/facility#ReviewedFileControl" => Ok(Self::ReviewedFile),
            "https://sbol.io/ns/facility#VendorSessionControl" => Ok(Self::VendorSession),
            "https://sbol.io/ns/facility#ApiControl" => Ok(Self::Api),
            "https://sbol.io/ns/facility#SiLA2Control" => Ok(Self::Sila2),
            "https://sbol.io/ns/facility#OpcUaControl" => Ok(Self::OpcUa),
            _ => Err(ControlModeError {
                value: value.to_owned(),
            }),
        }
    }
}

/// An IRI outside the closed control-mode vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("`{value}` is not an SBOLInventory control mode")]
pub struct ControlModeError {
    value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualification_order_is_the_normative_ladder() {
        assert!(QualificationLevel::Plannable.is_satisfied_by(QualificationLevel::Executable));
        assert!(!QualificationLevel::Executable.is_satisfied_by(QualificationLevel::Plannable));
        assert_eq!(
            serde_json::to_string(&QualificationLevel::Qualified).unwrap(),
            "\"https://sbol.io/ns/facility#Qualified\""
        );
    }

    #[test]
    fn control_modes_fail_closed_and_separate_unspecified() {
        assert!(!ControlMode::Unspecified.is_concrete());
        assert!(ControlMode::CONCRETE.iter().all(|mode| mode.is_concrete()));
        assert!(ControlMode::try_from("https://example.org/ApiControl").is_err());
    }
}
