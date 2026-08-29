use std::cmp::Ordering;
use std::fmt::{self, Display};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{PropertyKind, PropertyValue};

/// The relation between a required property value and an observed offering value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintRelation {
    Exact,
    AtLeast,
    AtMost,
}

impl Display for ConstraintRelation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Exact => "exact",
            Self::AtLeast => "at_least",
            Self::AtMost => "at_most",
        })
    }
}

/// One typed constraint on a capability offering property.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyConstraint {
    pub property_kind: PropertyKind,
    pub relation: ConstraintRelation,
    pub required: PropertyValue,
}

impl PropertyConstraint {
    /// Evaluates the constraint without numeric precision loss.
    ///
    /// A unit mismatch is an ordinary non-match. Ordered relations over non-numeric values are a
    /// malformed requirement and return an error so a planner cannot silently discard them.
    pub fn is_satisfied_by(
        &self,
        observed: &PropertyValue,
    ) -> Result<bool, ConstraintEvaluationError> {
        if self.required.unit != observed.unit {
            return Ok(false);
        }
        match self.relation {
            ConstraintRelation::Exact => Ok(self.required.semantically_equals(observed)),
            ConstraintRelation::AtLeast | ConstraintRelation::AtMost => {
                let ordering = self
                    .required
                    .value
                    .compare_numeric(&observed.value)
                    .ok_or_else(|| ConstraintEvaluationError::Incomparable {
                        property_kind: self.property_kind.clone(),
                        relation: self.relation,
                    })?;
                Ok(match self.relation {
                    ConstraintRelation::Exact => unreachable!("handled above"),
                    ConstraintRelation::AtLeast => {
                        matches!(ordering, Ordering::Less | Ordering::Equal)
                    }
                    ConstraintRelation::AtMost => {
                        matches!(ordering, Ordering::Greater | Ordering::Equal)
                    }
                })
            }
        }
    }
}

/// A constraint requests an ordering that its scalar types do not define.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ConstraintEvaluationError {
    #[error("property `{property_kind}` uses `{relation}` with non-numeric values")]
    Incomparable {
        property_kind: PropertyKind,
        relation: ConstraintRelation,
    },
}

#[cfg(test)]
mod tests {
    use crate::{ExactDecimal, ExactInteger, ScalarValue, UnitIri};

    use super::*;

    fn temperature(value: ScalarValue) -> PropertyValue {
        PropertyValue::new(
            value,
            Some(UnitIri::new("http://qudt.org/vocab/unit/DEG_C").unwrap()),
        )
        .unwrap()
    }

    fn constraint(relation: ConstraintRelation, value: ScalarValue) -> PropertyConstraint {
        PropertyConstraint {
            property_kind: PropertyKind::new("https://sbol.io/ns/capability#Temperature").unwrap(),
            relation,
            required: temperature(value),
        }
    }

    #[test]
    fn exact_numeric_constraints_cross_integer_and_real_lexical_kinds() {
        let requirement = constraint(
            ConstraintRelation::Exact,
            ScalarValue::Integer(ExactInteger::parse("4").unwrap()),
        );
        let observed = temperature(ScalarValue::Real(ExactDecimal::parse("4.000").unwrap()));
        assert!(requirement.is_satisfied_by(&observed).unwrap());
    }

    #[test]
    fn ordered_constraints_define_the_requirement_direction() {
        let minimum = constraint(
            ConstraintRelation::AtLeast,
            ScalarValue::Real(ExactDecimal::parse("4.5").unwrap()),
        );
        let maximum = constraint(
            ConstraintRelation::AtMost,
            ScalarValue::Real(ExactDecimal::parse("10").unwrap()),
        );
        let observed = temperature(ScalarValue::Real(ExactDecimal::parse("7.25").unwrap()));
        assert!(minimum.is_satisfied_by(&observed).unwrap());
        assert!(maximum.is_satisfied_by(&observed).unwrap());
    }

    #[test]
    fn unit_mismatch_is_a_non_match_and_text_ordering_is_an_error() {
        let minimum = constraint(
            ConstraintRelation::AtLeast,
            ScalarValue::Integer(ExactInteger::parse("4").unwrap()),
        );
        let seconds = PropertyValue::new(
            ScalarValue::Integer(ExactInteger::parse("5").unwrap()),
            Some(UnitIri::new("http://qudt.org/vocab/unit/SEC").unwrap()),
        )
        .unwrap();
        assert!(!minimum.is_satisfied_by(&seconds).unwrap());

        let text = PropertyConstraint {
            property_kind: PropertyKind::new("https://example.org/property#Mode").unwrap(),
            relation: ConstraintRelation::AtLeast,
            required: PropertyValue::unitless(ScalarValue::Text("a".to_owned())),
        };
        assert!(
            text.is_satisfied_by(&PropertyValue::unitless(ScalarValue::Text("b".to_owned())))
                .is_err()
        );
    }
}
