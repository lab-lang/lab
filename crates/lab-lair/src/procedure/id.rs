use std::borrow::Borrow;
use std::fmt::{self, Display};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A malformed stable identity local to one canonical Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("`{value}` must be non-empty and contain no whitespace or control characters")]
pub struct ProcedureLocalIdError {
    value: String,
}

/// A stable identity local to one canonical Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, JsonSchema)]
#[schemars(transparent)]
pub struct ProcedureLocalId(String);

impl ProcedureLocalId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProcedureLocalIdError> {
        let value = value.into();
        if value.is_empty()
            || value
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            Err(ProcedureLocalIdError { value })
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ProcedureLocalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for ProcedureLocalId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ProcedureLocalId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for ProcedureLocalId {
    type Error = ProcedureLocalIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for ProcedureLocalId {
    type Error = ProcedureLocalIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for ProcedureLocalId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ProcedureLocalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ids_validate_at_construction_and_deserialization() {
        assert_eq!(
            ProcedureLocalId::new("reaction/1").unwrap().as_str(),
            "reaction/1"
        );
        assert!(ProcedureLocalId::new("reaction 1").is_err());
        assert!(serde_json::from_str::<ProcedureLocalId>("\"bad\\nname\"").is_err());
    }
}
