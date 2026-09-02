//! Exact scalar encoding shared by Procedure parameters and Capability constraints.

use lab_capability::{
    AbsoluteIri, ExactDecimal, ExactInteger, PropertyValue, ScalarValue, UnitIri,
};

pub(super) fn encode_property_value(value: &PropertyValue) -> (&'static str, String) {
    match &value.value {
        ScalarValue::Text(value) => ("text", value.clone()),
        ScalarValue::Integer(value) => ("integer", value.to_string()),
        ScalarValue::Real(value) => ("real", value.to_string()),
        ScalarValue::Boolean(value) => ("boolean", value.to_string()),
        ScalarValue::Iri(value) => ("iri", value.to_string()),
    }
}

pub(super) fn decode_property_value(
    value_kind: &str,
    lexical: &str,
    unit: Option<&str>,
) -> Result<PropertyValue, String> {
    let value = match value_kind {
        "text" => ScalarValue::Text(lexical.to_owned()),
        "integer" => {
            ScalarValue::Integer(ExactInteger::parse(lexical).map_err(|error| error.to_string())?)
        }
        "real" => {
            ScalarValue::Real(ExactDecimal::parse(lexical).map_err(|error| error.to_string())?)
        }
        "boolean" => ScalarValue::Boolean(match lexical {
            "true" => true,
            "false" => false,
            _ => return Err("boolean value must be `true` or `false`".to_owned()),
        }),
        "iri" => ScalarValue::Iri(AbsoluteIri::new(lexical).map_err(|error| error.to_string())?),
        other => return Err(format!("unknown scalar value kind `{other}`")),
    };
    let unit = unit
        .map(|unit| UnitIri::new(unit).map_err(|error| error.to_string()))
        .transpose()?;
    PropertyValue::new(value, unit).map_err(|error| error.to_string())
}
