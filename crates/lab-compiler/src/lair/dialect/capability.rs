//! First-class semantic capability requirements inside method candidate regions.

// Construction APIs are consumed by the forthcoming method-refinement pass.
#![allow(dead_code)]

use std::cell::Ref;
use std::collections::BTreeSet;

use lab_capability::{
    CapabilityKind, ConstraintRelation, ControlMode, PropertyConstraint, PropertyKind,
    QualificationLevel,
};
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{NOpdsInterface, NResultsInterface};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::verify_err;

use crate::lair::dialect::attributes::string_vec;
use crate::lair::dialect::procedure::is_stable_local_id;
use crate::lair::dialect::scalar::{decode_property_value, encode_property_value};

/// A semantic requirement attached to one exact Procedure node in the same candidate.
#[pliron_op(
    name = "capability.requirement",
    format,
    attributes = (
        requirement_id: StringAttr,
        procedure_node: StringAttr,
        capability_kind: StringAttr,
        minimum_qualification: StringAttr,
        accepted_control_modes: VecAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct RequirementOp;

impl RequirementOp {
    pub(crate) fn new(
        context: &mut Context,
        requirement_id: impl Into<String>,
        procedure_node: impl Into<String>,
        capability_kind: &CapabilityKind,
        minimum_qualification: QualificationLevel,
        accepted_control_modes: impl IntoIterator<Item = ControlMode>,
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        result.set_attr_requirement_id(context, StringAttr::new(requirement_id.into()));
        result.set_attr_procedure_node(context, StringAttr::new(procedure_node.into()));
        result.set_attr_capability_kind(context, StringAttr::new(capability_kind.to_string()));
        result.set_attr_minimum_qualification(
            context,
            StringAttr::new(minimum_qualification.to_string()),
        );
        result.set_attr_accepted_control_modes(
            context,
            string_vec(
                accepted_control_modes
                    .into_iter()
                    .map(|mode| mode.to_string())
                    .collect(),
            ),
        );
        result
    }

    pub(crate) fn requirement_id(&self, context: &Context) -> String {
        self.get_attr_requirement_id(context)
            .expect("verified capability.requirement carries requirement_id")
            .as_str()
            .to_owned()
    }

    pub(crate) fn procedure_node(&self, context: &Context) -> String {
        self.get_attr_procedure_node(context)
            .expect("verified capability.requirement carries procedure_node")
            .as_str()
            .to_owned()
    }

    pub(crate) fn semantic_capability_kind(&self, context: &Context) -> CapabilityKind {
        CapabilityKind::new(
            self.get_attr_capability_kind(context)
                .expect("verified capability.requirement carries capability_kind")
                .as_str(),
        )
        .expect("verified capability.requirement capability kind is an absolute IRI")
    }

    pub(crate) fn semantic_minimum_qualification(&self, context: &Context) -> QualificationLevel {
        QualificationLevel::try_from(
            self.get_attr_minimum_qualification(context)
                .expect("verified capability.requirement carries minimum_qualification")
                .as_str(),
        )
        .expect("verified capability.requirement carries a closed qualification")
    }

    pub(crate) fn semantic_control_modes(&self, context: &Context) -> BTreeSet<ControlMode> {
        self.get_attr_accepted_control_modes(context)
            .expect("verified capability.requirement carries accepted_control_modes")
            .0
            .iter()
            .map(|value| {
                ControlMode::try_from(
                    value
                        .downcast_ref::<StringAttr>()
                        .expect("verified control modes are strings")
                        .as_str(),
                )
                .expect("verified control modes belong to the closed vocabulary")
            })
            .collect()
    }
}

impl Verify for RequirementOp {
    fn verify(&self, context: &Context) -> Result<()> {
        for (name, value) in [
            ("requirement_id", self.get_attr_requirement_id(context)),
            ("procedure_node", self.get_attr_procedure_node(context)),
        ] {
            if value.is_none_or(|value| !is_stable_local_id(value.as_str())) {
                return verify_err!(
                    self.loc(context),
                    "capability.requirement {name} must be non-empty and contain no whitespace"
                );
            }
        }
        if self
            .get_attr_capability_kind(context)
            .is_none_or(|value| CapabilityKind::new(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "capability.requirement capability_kind must be an absolute IRI"
            );
        }
        if self
            .get_attr_minimum_qualification(context)
            .is_none_or(|value| QualificationLevel::try_from(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "capability.requirement minimum_qualification must be a closed SBOLInventory qualification IRI"
            );
        }
        let Some(modes) = self.get_attr_accepted_control_modes(context) else {
            return verify_err!(
                self.loc(context),
                "capability.requirement is missing accepted_control_modes"
            );
        };
        let mut seen = BTreeSet::new();
        for value in &modes.0 {
            let Some(value) = value.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "capability.requirement accepted_control_modes must contain only strings"
                );
            };
            let Ok(mode) = ControlMode::try_from(value.as_str()) else {
                return verify_err!(
                    self.loc(context),
                    "capability.requirement accepted_control_modes contains an unknown IRI"
                );
            };
            if !mode.is_concrete() {
                return verify_err!(
                    self.loc(context),
                    "capability.requirement cannot accept UnspecifiedControl"
                );
            }
            if !seen.insert(mode) {
                return verify_err!(
                    self.loc(context),
                    "capability.requirement accepted_control_modes contains a duplicate"
                );
            }
        }
        if seen.is_empty() {
            return verify_err!(
                self.loc(context),
                "capability.requirement must accept at least one concrete control mode"
            );
        }
        Ok(())
    }
}

/// One typed property constraint attached to a requirement by stable identity.
#[pliron_op(
    name = "capability.constraint",
    format,
    attributes = (
        constraint_requirement_id: StringAttr,
        property_kind: StringAttr,
        relation: StringAttr,
        value_kind: StringAttr,
        value: StringAttr,
        unit: StringAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct ConstraintOp;

impl ConstraintOp {
    pub(crate) fn new(
        context: &mut Context,
        requirement_id: impl Into<String>,
        constraint: &PropertyConstraint,
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        let (value_kind, value) = encode_property_value(&constraint.required);
        result.set_attr_constraint_requirement_id(context, StringAttr::new(requirement_id.into()));
        result.set_attr_property_kind(
            context,
            StringAttr::new(constraint.property_kind.to_string()),
        );
        result.set_attr_relation(context, StringAttr::new(constraint.relation.to_string()));
        result.set_attr_value_kind(context, StringAttr::new(value_kind.to_owned()));
        result.set_attr_value(context, StringAttr::new(value));
        if let Some(unit) = &constraint.required.unit {
            result.set_attr_unit(context, StringAttr::new(unit.to_string()));
        }
        result
    }

    pub(crate) fn requirement_id(&self, context: &Context) -> String {
        self.get_attr_constraint_requirement_id(context)
            .expect("verified capability.constraint carries requirement_id")
            .as_str()
            .to_owned()
    }

    pub(crate) fn decode(
        &self,
        context: &Context,
    ) -> std::result::Result<PropertyConstraint, String> {
        let requirement_id = required_attr(
            self.get_attr_constraint_requirement_id(context),
            "constraint_requirement_id",
        )?;
        if !is_stable_local_id(&requirement_id) {
            return Err("requirement_id must be non-empty and contain no whitespace".to_owned());
        }
        let property_kind = PropertyKind::new(required_attr(
            self.get_attr_property_kind(context),
            "property_kind",
        )?)
        .map_err(|error| error.to_string())?;
        let relation = match required_attr(self.get_attr_relation(context), "relation")?.as_str() {
            "exact" => ConstraintRelation::Exact,
            "at_least" => ConstraintRelation::AtLeast,
            "at_most" => ConstraintRelation::AtMost,
            other => return Err(format!("unknown constraint relation `{other}`")),
        };
        let lexical = required_attr(self.get_attr_value(context), "value")?;
        let value_kind = required_attr(self.get_attr_value_kind(context), "value_kind")?;
        let unit = self.get_attr_unit(context);
        let required = decode_property_value(
            &value_kind,
            &lexical,
            unit.as_ref().map(|unit| unit.as_str()),
        )?;
        Ok(PropertyConstraint {
            property_kind,
            relation,
            required,
        })
    }
}

impl Verify for ConstraintOp {
    fn verify(&self, context: &Context) -> Result<()> {
        if let Err(error) = self.decode(context) {
            return verify_err!(self.loc(context), "invalid capability.constraint: {error}");
        }
        Ok(())
    }
}

fn required_attr(
    value: Option<Ref<'_, StringAttr>>,
    name: &str,
) -> std::result::Result<String, String> {
    value
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| format!("missing {name}"))
}
