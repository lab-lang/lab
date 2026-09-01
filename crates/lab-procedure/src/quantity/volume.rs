use lab_capability::{ExactDecimal, PropertyValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::QuantityError;
use super::value::{numeric_decimal, numeric_value, quantity, require_positive, validate_unit};
use crate::vocabulary::MICROLITRE;

/// An exact volume in canonical QUDT microlitres.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Volume(PropertyValue);

impl Volume {
    pub fn microlitres(value: ExactDecimal) -> Result<Self, QuantityError> {
        require_positive(&value)?;
        Ok(Self(quantity(value, MICROLITRE)))
    }

    pub fn parse_microlitres(value: impl AsRef<str>) -> Result<Self, QuantityError> {
        let value =
            ExactDecimal::parse(value.as_ref()).map_err(|_| QuantityError::InvalidNumber {
                value: value.as_ref().to_owned(),
            })?;
        Self::microlitres(value)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Volume {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, MICROLITRE).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Self::microlitres(exact).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::Volume;
    use crate::Temperature;

    #[test]
    fn volume_is_positive_exact_and_unit_checked() {
        let volume = Volume::parse_microlitres("0.5000").unwrap();
        assert_eq!(volume.value().to_string(), "0.5");
        let json = serde_json::to_string(&volume).unwrap();
        assert!(json.contains("http://qudt.org/vocab/unit/MicroL"));
        assert_eq!(serde_json::from_str::<Volume>(&json).unwrap(), volume);
        assert!(Volume::parse_microlitres("0").is_err());
        assert!(Volume::parse_microlitres("-1").is_err());

        let celsius =
            serde_json::to_string(&Temperature::parse_degrees_celsius("4").unwrap()).unwrap();
        assert!(serde_json::from_str::<Volume>(&celsius).is_err());
    }
}
