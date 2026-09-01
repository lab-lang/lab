use std::cmp::Ordering;

use lab_capability::{ExactDecimal, PropertyValue, ScalarValue, UnitIri};

use super::error::QuantityError;

pub(super) fn quantity(value: ExactDecimal, unit: &str) -> PropertyValue {
    PropertyValue::new(
        ScalarValue::Real(value),
        Some(UnitIri::new(unit).expect("built-in QUDT unit is an absolute IRI")),
    )
    .expect("numeric values accept units")
}

pub(super) fn validate_unit(value: &PropertyValue, expected: &str) -> Result<(), QuantityError> {
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

pub(super) fn numeric_value(value: &PropertyValue) -> &ExactDecimal {
    match &value.value {
        ScalarValue::Real(value) => value,
        _ => unreachable!("typed Procedure quantity stores an exact decimal"),
    }
}

pub(super) fn numeric_decimal(value: &PropertyValue) -> Result<ExactDecimal, QuantityError> {
    match &value.value {
        ScalarValue::Real(value) => Ok(value.clone()),
        ScalarValue::Integer(value) => Ok(ExactDecimal::from_integer(value)),
        _ => Err(QuantityError::NonNumeric),
    }
}

pub(super) fn require_positive(value: &ExactDecimal) -> Result<(), QuantityError> {
    let zero = ExactDecimal::parse("0").expect("zero is a valid decimal");
    if value.cmp(&zero) != Ordering::Greater {
        return Err(QuantityError::NonPositive {
            value: value.to_string(),
        });
    }
    Ok(())
}

pub(super) fn require_non_negative(value: &ExactDecimal) -> Result<(), QuantityError> {
    let zero = ExactDecimal::parse("0").expect("zero is a valid decimal");
    if value.cmp(&zero) == Ordering::Less {
        return Err(QuantityError::Negative {
            value: value.to_string(),
        });
    }
    Ok(())
}

pub(super) fn parse_decimal(value: impl AsRef<str>) -> Result<ExactDecimal, QuantityError> {
    ExactDecimal::parse(value.as_ref()).map_err(|_| QuantityError::InvalidNumber {
        value: value.as_ref().to_owned(),
    })
}
