use std::cmp::Ordering;

use lab_capability::{ExactDecimal, PropertyValue, ScalarValue, UnitIri};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::vocabulary::{DEGREE_CELSIUS, DEGREE_CELSIUS_PER_SECOND, MICROLITRE, SECOND};

/// An invalid exact physical quantity in a canonical Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum QuantityError {
    #[error("`{value}` is not a finite base-10 decimal quantity")]
    InvalidNumber { value: String },
    #[error("quantity unit `{found}` must be `{expected}`")]
    WrongUnit { expected: String, found: String },
    #[error("quantity value must be numeric")]
    NonNumeric,
    #[error("quantity must be greater than zero, found `{value}`")]
    NonPositive { value: String },
    #[error("quantity must not be negative, found `{value}`")]
    Negative { value: String },
    #[error("temperature range minimum `{minimum}` exceeds maximum `{maximum}`")]
    ReversedTemperatureRange { minimum: String, maximum: String },
}

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

fn quantity(value: ExactDecimal, unit: &str) -> PropertyValue {
    PropertyValue::new(
        ScalarValue::Real(value),
        Some(UnitIri::new(unit).expect("built-in QUDT unit is an absolute IRI")),
    )
    .expect("numeric values accept units")
}

fn validate_unit(value: &PropertyValue, expected: &str) -> Result<(), QuantityError> {
    let found = value
        .unit
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    if found != expected {
        return Err(QuantityError::WrongUnit {
            expected: expected.to_owned(),
            found,
        });
    }
    Ok(())
}

fn numeric_value(value: &PropertyValue) -> &ExactDecimal {
    match &value.value {
        ScalarValue::Real(value) => value,
        _ => unreachable!("typed Procedure quantity stores an exact decimal"),
    }
}

fn numeric_decimal(value: &PropertyValue) -> Result<ExactDecimal, QuantityError> {
    match &value.value {
        ScalarValue::Real(value) => Ok(value.clone()),
        ScalarValue::Integer(value) => Ok(ExactDecimal::from_integer(value)),
        _ => Err(QuantityError::NonNumeric),
    }
}

fn require_positive(value: &ExactDecimal) -> Result<(), QuantityError> {
    let zero = ExactDecimal::parse("0").expect("zero is a valid decimal");
    if value.cmp(&zero) != Ordering::Greater {
        return Err(QuantityError::NonPositive {
            value: value.to_string(),
        });
    }
    Ok(())
}

fn require_non_negative(value: &ExactDecimal) -> Result<(), QuantityError> {
    let zero = ExactDecimal::parse("0").expect("zero is a valid decimal");
    if value.cmp(&zero) == Ordering::Less {
        return Err(QuantityError::Negative {
            value: value.to_string(),
        });
    }
    Ok(())
}

fn parse_decimal(value: impl AsRef<str>) -> Result<ExactDecimal, QuantityError> {
    ExactDecimal::parse(value.as_ref()).map_err(|_| QuantityError::InvalidNumber {
        value: value.as_ref().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn durations_and_ramp_rates_have_distinct_bounds_and_units() {
        let zero = Duration::parse_seconds("0").unwrap();
        assert_eq!(zero.value().to_string(), "0");
        assert!(Duration::parse_seconds("-0.1").is_err());

        let ramp = TemperatureRampRate::parse_degrees_celsius_per_second("1.5").unwrap();
        assert_eq!(ramp.value().to_string(), "1.5");
        assert!(TemperatureRampRate::parse_degrees_celsius_per_second("0").is_err());
        assert!(serde_json::from_str::<Duration>(&serde_json::to_string(&ramp).unwrap()).is_err());
    }
}
