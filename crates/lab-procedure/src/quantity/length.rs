use lab_capability::{ExactDecimal, PropertyValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::value::{numeric_decimal, numeric_value, quantity, validate_unit};
use crate::vocabulary::MILLIMETRE;

/// An exact signed length in canonical QUDT millimetres.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Length(PropertyValue);

impl Length {
    pub fn millimetres(value: ExactDecimal) -> Self {
        Self(quantity(value, MILLIMETRE))
    }

    pub fn parse_millimetres(
        value: impl AsRef<str>,
    ) -> Result<Self, lab_capability::NumberParseError> {
        ExactDecimal::parse(value).map(Self::millimetres)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, MILLIMETRE).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Ok(Self::millimetres(exact))
    }
}

#[cfg(test)]
mod tests {
    use super::Length;

    #[test]
    fn length_accepts_signed_vessel_relative_offsets() {
        let length = Length::parse_millimetres("-8.0").unwrap();
        assert_eq!(length.value().to_string(), "-8");
        let json = serde_json::to_string(&length).unwrap();
        assert!(json.contains("http://qudt.org/vocab/unit/MilliM"));
        assert_eq!(serde_json::from_str::<Length>(&json).unwrap(), length);
    }
}
