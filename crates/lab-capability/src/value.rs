use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::{AbsoluteIri, UnitIri};

/// A malformed integer or decimal lexical value.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("`{value}` is not a finite base-10 {kind}")]
pub struct NumberParseError {
    value: String,
    kind: &'static str,
}

/// An arbitrary-precision signed integer stored in canonical decimal notation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactInteger(String);

impl ExactInteger {
    /// Parses and canonicalizes a signed base-10 integer without a size limit.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, NumberParseError> {
        let original = value.as_ref();
        let (negative, digits) = sign_and_body(original);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(NumberParseError {
                value: original.to_owned(),
                kind: "integer",
            });
        }
        let digits = digits.trim_start_matches('0');
        if digits.is_empty() {
            return Ok(Self("0".to_owned()));
        }
        Ok(Self(if negative {
            format!("-{digits}")
        } else {
            digits.to_owned()
        }))
    }

    /// Returns the canonical lexical value.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn sign_and_digits(&self) -> (bool, &str) {
        self.0
            .strip_prefix('-')
            .map_or((false, self.0.as_str()), |digits| (true, digits))
    }
}

impl Display for ExactInteger {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ExactInteger {
    type Err = NumberParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for ExactInteger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ExactInteger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for ExactInteger {
    fn schema_name() -> Cow<'static, str> {
        "ExactInteger".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::ExactInteger").into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^[+-]?[0-9]+$"
        })
    }
}

impl PartialOrd for ExactInteger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExactInteger {
    fn cmp(&self, other: &Self) -> Ordering {
        let (left_negative, left) = self.sign_and_digits();
        let (right_negative, right) = other.sign_and_digits();
        match (left_negative, right_negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => compare_unsigned_integers(left, right),
            (true, true) => compare_unsigned_integers(left, right).reverse(),
        }
    }
}

/// An arbitrary-precision finite decimal stored as a canonical coefficient and scale.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactDecimal {
    negative: bool,
    digits: String,
    scale: usize,
}

impl ExactDecimal {
    /// Parses a signed base-10 decimal without exponent notation or a size limit.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, NumberParseError> {
        let original = value.as_ref();
        let (negative, body) = sign_and_body(original);
        let mut pieces = body.split('.');
        let integer = pieces.next().unwrap_or_default();
        let fractional = pieces.next();
        if pieces.next().is_some()
            || (integer.is_empty() && fractional.is_none_or(str::is_empty))
            || !integer.bytes().all(|byte| byte.is_ascii_digit())
            || fractional.is_some_and(|digits| !digits.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(NumberParseError {
                value: original.to_owned(),
                kind: "decimal",
            });
        }
        let integer = if integer.is_empty() { "0" } else { integer };
        let integer = integer.trim_start_matches('0');
        let integer = if integer.is_empty() { "0" } else { integer };
        let fractional = fractional.unwrap_or_default().trim_end_matches('0');
        let scale = fractional.len();
        let combined = format!("{integer}{fractional}");
        let digits = combined.trim_start_matches('0');
        if digits.is_empty() {
            return Ok(Self {
                negative: false,
                digits: "0".to_owned(),
                scale: 0,
            });
        }
        Ok(Self {
            negative,
            digits: digits.to_owned(),
            scale,
        })
    }

    /// Constructs the exact decimal represented by an integer.
    pub fn from_integer(value: &ExactInteger) -> Self {
        let (negative, digits) = value.sign_and_digits();
        Self {
            negative,
            digits: digits.to_owned(),
            scale: 0,
        }
    }

    /// Whether this value is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.digits == "0"
    }

    fn unsigned_integer_digits(&self) -> &str {
        if self.digits.len() > self.scale {
            &self.digits[..self.digits.len() - self.scale]
        } else {
            "0"
        }
    }

    fn fractional_digit(&self, index: usize) -> u8 {
        if index >= self.scale {
            return b'0';
        }
        if self.digits.len() > self.scale {
            self.digits.as_bytes()[self.digits.len() - self.scale + index]
        } else {
            let leading_zeroes = self.scale - self.digits.len();
            if index < leading_zeroes {
                b'0'
            } else {
                self.digits.as_bytes()[index - leading_zeroes]
            }
        }
    }

    fn cmp_magnitude(&self, other: &Self) -> Ordering {
        let integer = compare_unsigned_integers(
            self.unsigned_integer_digits(),
            other.unsigned_integer_digits(),
        );
        if integer != Ordering::Equal {
            return integer;
        }
        (0..self.scale.max(other.scale))
            .map(|index| {
                self.fractional_digit(index)
                    .cmp(&other.fractional_digit(index))
            })
            .find(|ordering| *ordering != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    }
}

impl Display for ExactDecimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negative {
            formatter.write_str("-")?;
        }
        if self.scale == 0 {
            return formatter.write_str(&self.digits);
        }
        if self.digits.len() <= self.scale {
            formatter.write_str("0.")?;
            for _ in 0..self.scale - self.digits.len() {
                formatter.write_str("0")?;
            }
            formatter.write_str(&self.digits)
        } else {
            let split = self.digits.len() - self.scale;
            write!(
                formatter,
                "{}.{}",
                &self.digits[..split],
                &self.digits[split..]
            )
        }
    }
}

impl FromStr for ExactDecimal {
    type Err = NumberParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for ExactDecimal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ExactDecimal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for ExactDecimal {
    fn schema_name() -> Cow<'static, str> {
        "ExactDecimal".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::ExactDecimal").into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^[+-]?(([0-9]+(\.[0-9]*)?)|(\.[0-9]+))$"
        })
    }
}

impl PartialOrd for ExactDecimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExactDecimal {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.cmp_magnitude(other),
            (true, true) => self.cmp_magnitude(other).reverse(),
        }
    }
}

/// A typed scalar accepted by capability properties and constraints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ScalarValue {
    Text(String),
    Integer(ExactInteger),
    Real(ExactDecimal),
    Boolean(bool),
    Iri(AbsoluteIri),
}

impl ScalarValue {
    /// Compares numeric integer and real variants without floating-point conversion.
    pub fn compare_numeric(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => Some(left.cmp(right)),
            (Self::Integer(left), Self::Real(right)) => {
                Some(ExactDecimal::from_integer(left).cmp(right))
            }
            (Self::Real(left), Self::Integer(right)) => {
                Some(left.cmp(&ExactDecimal::from_integer(right)))
            }
            (Self::Real(left), Self::Real(right)) => Some(left.cmp(right)),
            _ => None,
        }
    }

    /// Semantic equality, treating numerically equal integer and real values as equal.
    pub fn semantically_equals(&self, other: &Self) -> bool {
        self.compare_numeric(other).is_some_and(Ordering::is_eq) || self == other
    }

    /// Whether a measurement unit is meaningful for this scalar.
    pub const fn supports_unit(&self) -> bool {
        matches!(self, Self::Integer(_) | Self::Real(_))
    }
}

/// A scalar capability value with an optional exact unit IRI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PropertyValue {
    pub value: ScalarValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitIri>,
}

impl PropertyValue {
    /// Constructs a valid property value. Only numeric values may carry units.
    pub fn new(value: ScalarValue, unit: Option<UnitIri>) -> Result<Self, PropertyValueError> {
        if unit.is_some() && !value.supports_unit() {
            return Err(PropertyValueError::UnitOnNonNumericValue);
        }
        Ok(Self { value, unit })
    }

    /// Constructs a unitless value.
    pub fn unitless(value: ScalarValue) -> Self {
        Self { value, unit: None }
    }

    /// Semantic equality including exact unit identity.
    pub fn semantically_equals(&self, other: &Self) -> bool {
        self.unit == other.unit && self.value.semantically_equals(&other.value)
    }
}

impl<'de> Deserialize<'de> for PropertyValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Representation {
            value: ScalarValue,
            #[serde(default)]
            unit: Option<UnitIri>,
        }

        let value = Representation::deserialize(deserializer)?;
        Self::new(value.value, value.unit).map_err(serde::de::Error::custom)
    }
}

/// A property value violates the semantic scalar/unit model.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PropertyValueError {
    #[error("only numeric capability values may carry a unit")]
    UnitOnNonNumericValue,
}

fn sign_and_body(value: &str) -> (bool, &str) {
    if let Some(body) = value.strip_prefix('-') {
        (true, body)
    } else if let Some(body) = value.strip_prefix('+') {
        (false, body)
    } else {
        (false, value)
    }
}

fn compare_unsigned_integers(left: &str, right: &str) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_canonical_and_arbitrarily_large() {
        let value = ExactInteger::parse("+000123456789012345678901234567890").unwrap();
        assert_eq!(value.as_str(), "123456789012345678901234567890");
        assert_eq!(ExactInteger::parse("-000").unwrap().as_str(), "0");
        assert!(ExactInteger::parse("1.0").is_err());
        assert!(ExactInteger::parse("").is_err());
        assert!(serde_json::from_str::<ExactInteger>("\"1.0\"").is_err());
    }

    #[test]
    fn decimals_canonicalize_and_compare_exactly() {
        let cases = [
            ("+001.2300", "1.23"),
            (".001200", "0.0012"),
            ("1.", "1"),
            ("-0.000", "0"),
            ("100.0200", "100.02"),
        ];
        for (source, expected) in cases {
            assert_eq!(ExactDecimal::parse(source).unwrap().to_string(), expected);
        }
        assert!(
            ExactDecimal::parse("0.1000000000000000000000000001").unwrap()
                > ExactDecimal::parse("0.1").unwrap()
        );
        assert!(ExactDecimal::parse("-1000.2").unwrap() < ExactDecimal::parse("-999.9").unwrap());
        assert!(ExactDecimal::parse("1e3").is_err());
        assert_eq!(
            ExactDecimal::parse("0.0012")
                .unwrap()
                .cmp(&ExactDecimal::parse("0.00120").unwrap()),
            Ordering::Equal
        );
    }

    #[test]
    fn serde_uses_canonical_strings_instead_of_binary_floats() {
        let value = ExactDecimal::parse("01.2500").unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"1.25\"");
        assert_eq!(
            serde_json::from_str::<ExactDecimal>("\"1.250\"").unwrap(),
            value
        );
        assert!(serde_json::from_str::<ExactDecimal>("1.25").is_err());
    }

    #[test]
    fn numeric_kinds_compare_semantically() {
        let integer = ScalarValue::Integer(ExactInteger::parse("4").unwrap());
        let real = ScalarValue::Real(ExactDecimal::parse("4.000").unwrap());
        assert!(integer.semantically_equals(&real));
    }

    #[test]
    fn only_numeric_values_accept_units_even_through_serde() {
        let unit = UnitIri::new("http://qudt.org/vocab/unit/DEG_C").unwrap();
        assert!(PropertyValue::new(ScalarValue::Text("cold".to_owned()), Some(unit)).is_err());
        let invalid = r#"{
            "value": {"type": "text", "value": "cold"},
            "unit": "http://qudt.org/vocab/unit/DEG_C"
        }"#;
        assert!(serde_json::from_str::<PropertyValue>(invalid).is_err());
    }
}
