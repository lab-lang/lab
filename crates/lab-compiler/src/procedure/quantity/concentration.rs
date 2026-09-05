use lab_capability::{ExactDecimal, PropertyValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::QuantityError;
use super::value::{
    numeric_decimal, numeric_value, parse_decimal, quantity, require_positive, validate_unit,
};
use crate::procedure::vocabulary::GRAM_PER_LITRE;

/// An exact mass concentration in canonical QUDT grams per litre.
///
/// Grams per litre is the unit a medium recipe is written in, and the unit a nucleic-acid
/// concentration converts to exactly: one nanogram per microlitre is `0.001` grams per litre.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct MassConcentration(PropertyValue);

impl MassConcentration {
    pub fn grams_per_litre(value: ExactDecimal) -> Result<Self, QuantityError> {
        require_positive(&value)?;
        Ok(Self(quantity(value, GRAM_PER_LITRE)))
    }

    pub fn parse_grams_per_litre(value: impl AsRef<str>) -> Result<Self, QuantityError> {
        Self::grams_per_litre(parse_decimal(value)?)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for MassConcentration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, GRAM_PER_LITRE).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Self::grams_per_litre(exact).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::MassConcentration;
    use crate::procedure::Mass;

    #[test]
    fn concentration_is_positive_exact_and_unit_checked() {
        let concentration = MassConcentration::parse_grams_per_litre("10.0").unwrap();
        assert_eq!(concentration.value().to_string(), "10");
        let json = serde_json::to_string(&concentration).unwrap();
        assert!(json.contains("http://qudt.org/vocab/unit/GM-PER-L"));
        assert_eq!(
            serde_json::from_str::<MassConcentration>(&json).unwrap(),
            concentration
        );
        assert!(MassConcentration::parse_grams_per_litre("0").is_err());
        assert!(MassConcentration::parse_grams_per_litre("-1").is_err());

        let grams = serde_json::to_string(&Mass::parse_grams("10").unwrap()).unwrap();
        assert!(serde_json::from_str::<MassConcentration>(&grams).is_err());
    }

    #[test]
    fn nucleic_acid_concentrations_stay_exact() {
        // 100 ng/uL is 0.1 g/L.
        let concentration = MassConcentration::parse_grams_per_litre("0.1").unwrap();
        assert_eq!(concentration.value().to_string(), "0.1");
    }
}
