use lab_capability::{ExactDecimal, PropertyValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::QuantityError;
use super::value::{
    numeric_decimal, numeric_value, parse_decimal, quantity, require_positive, validate_unit,
};
use crate::vocabulary::{DEGREE_CELSIUS, DEGREE_CELSIUS_PER_SECOND};

/// An exact temperature in canonical QUDT degrees Celsius.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Temperature(PropertyValue);

impl Temperature {
    pub fn degrees_celsius(value: ExactDecimal) -> Self {
        Self(quantity(value, DEGREE_CELSIUS))
    }

    pub fn parse_degrees_celsius(
        value: impl AsRef<str>,
    ) -> Result<Self, lab_capability::NumberParseError> {
        ExactDecimal::parse(value).map(Self::degrees_celsius)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Temperature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, DEGREE_CELSIUS).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Ok(Self::degrees_celsius(exact))
    }
}

/// A closed temperature interval required by a Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TemperatureRange {
    pub minimum: Temperature,
    pub maximum: Temperature,
}

impl TemperatureRange {
    pub fn new(minimum: Temperature, maximum: Temperature) -> Result<Self, QuantityError> {
        if minimum.value() > maximum.value() {
            return Err(QuantityError::ReversedTemperatureRange {
                minimum: minimum.value().to_string(),
                maximum: maximum.value().to_string(),
            });
        }
        Ok(Self { minimum, maximum })
    }

    pub fn exact(value: Temperature) -> Self {
        Self {
            minimum: value.clone(),
            maximum: value,
        }
    }
}

impl<'de> Deserialize<'de> for TemperatureRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Representation {
            minimum: Temperature,
            maximum: Temperature,
        }

        let value = Representation::deserialize(deserializer)?;
        Self::new(value.minimum, value.maximum).map_err(serde::de::Error::custom)
    }
}
/// An exact positive thermal ramp rate in canonical QUDT degrees Celsius per second.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct TemperatureRampRate(PropertyValue);

impl TemperatureRampRate {
    pub fn degrees_celsius_per_second(value: ExactDecimal) -> Result<Self, QuantityError> {
        require_positive(&value)?;
        Ok(Self(quantity(value, DEGREE_CELSIUS_PER_SECOND)))
    }

    pub fn parse_degrees_celsius_per_second(value: impl AsRef<str>) -> Result<Self, QuantityError> {
        let value = parse_decimal(value)?;
        Self::degrees_celsius_per_second(value)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for TemperatureRampRate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, DEGREE_CELSIUS_PER_SECOND).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Self::degrees_celsius_per_second(exact).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{Temperature, TemperatureRampRate, TemperatureRange};
    use crate::Duration;

    #[test]
    fn temperature_ranges_validate_order_during_construction_and_deserialization() {
        let cold = Temperature::parse_degrees_celsius("4").unwrap();
        let warm = Temperature::parse_degrees_celsius("8").unwrap();
        assert!(TemperatureRange::new(cold.clone(), warm.clone()).is_ok());
        assert!(TemperatureRange::new(warm.clone(), cold.clone()).is_err());

        let reversed = format!(
            "{{\"minimum\":{},\"maximum\":{}}}",
            serde_json::to_string(&warm).unwrap(),
            serde_json::to_string(&cold).unwrap()
        );
        assert!(serde_json::from_str::<TemperatureRange>(&reversed).is_err());
    }

    #[test]
    fn ramp_rates_are_positive_and_distinct_from_durations() {
        let ramp = TemperatureRampRate::parse_degrees_celsius_per_second("1.5").unwrap();
        assert_eq!(ramp.value().to_string(), "1.5");
        assert!(TemperatureRampRate::parse_degrees_celsius_per_second("0").is_err());
        assert!(serde_json::from_str::<Duration>(&serde_json::to_string(&ramp).unwrap()).is_err());
    }
}
