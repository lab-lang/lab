use std::borrow::Borrow;
use std::fmt::{self, Display};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Reports why a semantic identity could not be represented as an absolute IRI.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("`{value}` is not an absolute IRI")]
pub struct IriError {
    value: String,
}

impl IriError {
    /// The rejected lexical value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// An owned absolute IRI validated without importing an RDF or URL implementation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, JsonSchema)]
#[schemars(transparent)]
pub struct AbsoluteIri(String);

impl AbsoluteIri {
    /// Validates and owns an absolute IRI.
    pub fn new(value: impl Into<String>) -> Result<Self, IriError> {
        let value = value.into();
        if is_absolute_iri(&value) {
            Ok(Self(value))
        } else {
            Err(IriError { value })
        }
    }

    /// Returns the validated lexical IRI.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Releases the owned lexical IRI.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Display for AbsoluteIri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for AbsoluteIri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for AbsoluteIri {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for AbsoluteIri {
    type Error = IriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for AbsoluteIri {
    type Error = IriError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for AbsoluteIri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AbsoluteIri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Recognizes an absolute IRI without pulling an RDF or URL stack into semantic code.
pub fn is_absolute_iri(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        && scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
        && !value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(
                    character,
                    '<' | '>' | '"' | '{' | '}' | '|' | '\\' | '^' | '`'
                )
        })
}

macro_rules! semantic_iri {
    ($(#[$meta:meta])* $name:ident, $description:literal) => {
        $(#[$meta])*
        #[doc = $description]
        #[derive(
            Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(AbsoluteIri);

        impl $name {
            /// Validates and owns the semantic IRI.
            pub fn new(value: impl Into<String>) -> Result<Self, IriError> {
                AbsoluteIri::new(value).map(Self)
            }

            /// Returns the validated lexical IRI.
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// Returns the shared absolute-IRI representation.
            pub fn as_iri(&self) -> &AbsoluteIri {
                &self.0
            }

            /// Releases the shared absolute-IRI representation.
            pub fn into_iri(self) -> AbsoluteIri {
                self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
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
            type Error = IriError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IriError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for AbsoluteIri {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

semantic_iri!(
    CapabilityKind,
    "The absolute IRI naming a semantic capability kind."
);
semantic_iri!(
    PropertyKind,
    "The absolute IRI naming a semantic capability property."
);
semantic_iri!(UnitIri, "The absolute IRI naming a measurement unit.");
semantic_iri!(
    MethodId,
    "The stable absolute IRI identifying a method definition."
);
semantic_iri!(
    OperationId,
    "The stable absolute IRI identifying a semantic procedure operation."
);
semantic_iri!(
    ProcedureContractId,
    "The stable absolute IRI identifying a versioned operational Procedure contract."
);
semantic_iri!(
    ProcedureImplementationId,
    "The stable absolute IRI identifying one adapter implementation of a Procedure contract."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_absolute_iris_without_an_rdf_stack() {
        assert!(is_absolute_iri("https://example.org/design"));
        assert!(is_absolute_iri(
            "urn:uuid:2ed8c319-58b7-46ad-aaf0-95c79be6b107"
        ));
        assert!(!is_absolute_iri("BBa_J23101"));
        assert!(!is_absolute_iri("1https://example.org/design"));
        assert!(!is_absolute_iri("https://example.org/a design"));
        assert!(!is_absolute_iri("https:"));
    }

    #[test]
    fn nominal_iri_types_round_trip_but_do_not_interchange() {
        let kind = CapabilityKind::new("https://sbol.io/ns/capability#LiquidHandling").unwrap();
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"https://sbol.io/ns/capability#LiquidHandling\"");
        assert_eq!(serde_json::from_str::<CapabilityKind>(&json).unwrap(), kind);
        assert!(serde_json::from_str::<PropertyKind>("\"Temperature\"").is_err());
    }
}
