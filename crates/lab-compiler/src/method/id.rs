use std::borrow::Borrow;
use std::fmt::{self, Display};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A malformed stable identity used within a method definition.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("`{value}` must be non-empty and contain no whitespace or control characters")]
pub struct LocalIdError {
    value: String,
}

macro_rules! local_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, JsonSchema)]
        #[schemars(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, LocalIdError> {
                let value = value.into();
                if value.is_empty()
                    || value
                        .chars()
                        .any(|character| character.is_whitespace() || character.is_control())
                {
                    Err(LocalIdError { value })
                } else {
                    Ok(Self(value))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl TryFrom<String> for $name {
            type Error = LocalIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = LocalIdError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

local_id!(
    /// A stable identity local to one method definition, such as a task or port name.
    LocalId
);

local_id!(
    /// The exact frontend Intent operation refined by a method.
    IntentOperationId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_validate_during_construction_and_deserialization() {
        let operation = IntentOperationId::new("std.lab.plasmid.recover").unwrap();
        assert_eq!(operation.as_str(), "std.lab.plasmid.recover");
        assert!(LocalId::new("candidate task").is_err());
        assert!(serde_json::from_str::<LocalId>("\"bad\\nname\"").is_err());
    }
}
