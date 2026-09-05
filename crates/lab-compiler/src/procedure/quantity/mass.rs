use lab_capability::{ExactDecimal, PropertyValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::QuantityError;
use super::value::{
    numeric_decimal, numeric_value, parse_decimal, quantity, require_positive, validate_unit,
};
use crate::procedure::vocabulary::GRAM;

/// An exact mass in canonical QUDT grams.
///
/// Grams are the canonical unit because a mass reaches the bench through a balance, and a balance
/// reads grams. Nucleic-acid masses are exact at this scale: five nanograms is `0.000000005`, which
/// an exact decimal represents without rounding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Mass(PropertyValue);

impl Mass {
    pub fn grams(value: ExactDecimal) -> Result<Self, QuantityError> {
        require_positive(&value)?;
        Ok(Self(quantity(value, GRAM)))
    }

    pub fn parse_grams(value: impl AsRef<str>) -> Result<Self, QuantityError> {
        Self::grams(parse_decimal(value)?)
    }

    pub fn value(&self) -> &ExactDecimal {
        numeric_value(&self.0)
    }

    pub fn as_property_value(&self) -> &PropertyValue {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Mass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        validate_unit(&value, GRAM).map_err(serde::de::Error::custom)?;
        let exact = numeric_decimal(&value).map_err(serde::de::Error::custom)?;
        Self::grams(exact).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::Mass;
    use crate::procedure::Volume;

    #[test]
    fn mass_is_positive_exact_and_unit_checked() {
        let mass = Mass::parse_grams("5.0000").unwrap();
        assert_eq!(mass.value().to_string(), "5");
        let json = serde_json::to_string(&mass).unwrap();
        assert!(json.contains("http://qudt.org/vocab/unit/GM"));
        assert_eq!(serde_json::from_str::<Mass>(&json).unwrap(), mass);
        assert!(Mass::parse_grams("0").is_err());
        assert!(Mass::parse_grams("-1").is_err());

        let microlitres = serde_json::to_string(&Volume::parse_microlitres("5").unwrap()).unwrap();
        assert!(serde_json::from_str::<Mass>(&microlitres).is_err());
    }

    #[test]
    fn nucleic_acid_masses_stay_exact() {
        let mass = Mass::parse_grams("0.000000005").unwrap();
        assert_eq!(mass.value().to_string(), "0.000000005");
    }
}
