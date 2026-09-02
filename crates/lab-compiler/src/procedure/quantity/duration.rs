use lab_capability::{ExactDecimal, PropertyValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::QuantityError;
use super::value::{
    numeric_decimal, numeric_value, parse_decimal, quantity, require_non_negative, validate_unit,
};
use crate::procedure::vocabulary::SECOND;

/// An exact non-negative duration in canonical QUDT seconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Duration(PropertyValue);

impl Duration {
    pub fn seconds(value: ExactDecimal) -> Result<Self, QuantityError> {
        require_non_negative(&value)?;
        Ok(Self(quantity(value, SECOND)))
    }

    pub fn parse_seconds(value: impl AsRef<str>) -> Result<Self, QuantityError> {
        let value = parse_decimal(value)?;
        Self::seconds(value)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, SECOND).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Self::seconds(exact).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::Duration;

    #[test]
    fn durations_are_non_negative() {
        let zero = Duration::parse_seconds("0").unwrap();
        assert_eq!(zero.value().to_string(), "0");
        assert!(Duration::parse_seconds("-0.1").is_err());
    }
}
