use lab_capability::{
    CapabilityKind, ConstraintRelation, ExactInteger, PropertyConstraint, PropertyKind,
    PropertyValue, ScalarValue,
};

use super::validation::ValidatedThermalProgramV1;
use crate::procedure::vocabulary::{
    CONTROLLED_TEMPERATURE_RAMP, HEATED_LID_TEMPERATURE_CONTROL, MAXIMUM_BLOCK_TEMPERATURE,
    MAXIMUM_LID_TEMPERATURE, MAXIMUM_RAMP_RATE, MAXIMUM_SAMPLE_COUNT,
    MAXIMUM_THERMAL_SAMPLE_VOLUME, MINIMUM_BLOCK_TEMPERATURE, MINIMUM_LID_TEMPERATURE,
    MINIMUM_THERMAL_SAMPLE_VOLUME, PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
};
use crate::procedure::{BindingScope, CapabilityClause, CapabilityFormula, ProcedureLocalId};

impl ValidatedThermalProgramV1 {
    /// Derive exact facility demands from the thermal controls present in this program.
    pub fn capability_formula(&self) -> CapabilityFormula {
        let temperatures = self
            .as_program()
            .stages
            .iter()
            .flat_map(|stage| stage.steps.iter().map(|step| &step.temperature))
            .chain(self.as_program().final_hold.iter())
            .collect::<Vec<_>>();
        let minimum = temperatures
            .iter()
            .min_by_key(|temperature| temperature.value())
            .expect("validated thermal programs have at least one step");
        let maximum = temperatures
            .iter()
            .max_by_key(|temperature| temperature.value())
            .expect("validated thermal programs have at least one step");
        let mut all_of = vec![CapabilityClause {
            role: local("block-temperature"),
            capability_kind: capability(PROGRAMMED_BLOCK_TEMPERATURE_CONTROL),
            constraints: vec![
                constraint(
                    MINIMUM_BLOCK_TEMPERATURE,
                    ConstraintRelation::AtMost,
                    minimum.as_property_value(),
                ),
                constraint(
                    MAXIMUM_BLOCK_TEMPERATURE,
                    ConstraintRelation::AtLeast,
                    maximum.as_property_value(),
                ),
                constraint(
                    MAXIMUM_SAMPLE_COUNT,
                    ConstraintRelation::AtLeast,
                    &integer(self.as_program().load.sample_count),
                ),
                constraint(
                    MINIMUM_THERMAL_SAMPLE_VOLUME,
                    ConstraintRelation::AtMost,
                    self.as_program().load.volume_each.as_property_value(),
                ),
                constraint(
                    MAXIMUM_THERMAL_SAMPLE_VOLUME,
                    ConstraintRelation::AtLeast,
                    self.as_program().load.volume_each.as_property_value(),
                ),
            ],
        }];
        if let Some(lid) = &self.as_program().lid_temperature {
            all_of.push(CapabilityClause {
                role: local("heated-lid"),
                capability_kind: capability(HEATED_LID_TEMPERATURE_CONTROL),
                constraints: vec![
                    constraint(
                        MINIMUM_LID_TEMPERATURE,
                        ConstraintRelation::AtMost,
                        lid.as_property_value(),
                    ),
                    constraint(
                        MAXIMUM_LID_TEMPERATURE,
                        ConstraintRelation::AtLeast,
                        lid.as_property_value(),
                    ),
                ],
            });
        }
        if let Some(maximum) = self
            .as_program()
            .stages
            .iter()
            .flat_map(|stage| stage.steps.iter())
            .filter_map(|step| step.ramp_rate.as_ref())
            .max_by_key(|rate| rate.value())
        {
            all_of.push(CapabilityClause {
                role: local("controlled-ramp"),
                capability_kind: capability(CONTROLLED_TEMPERATURE_RAMP),
                constraints: vec![constraint(
                    MAXIMUM_RAMP_RATE,
                    ConstraintRelation::AtLeast,
                    maximum.as_property_value(),
                )],
            });
        }
        CapabilityFormula {
            binding_scope: BindingScope::AtomicAssetAssembly,
            all_of,
        }
    }
}

fn local(value: &str) -> ProcedureLocalId {
    ProcedureLocalId::new(value).expect("built-in role is a valid Procedure-local identity")
}

fn capability(value: &str) -> CapabilityKind {
    CapabilityKind::new(value).expect("built-in capability is an absolute IRI")
}

fn property(value: &str) -> PropertyKind {
    PropertyKind::new(value).expect("built-in property is an absolute IRI")
}

fn constraint(
    kind: &str,
    relation: ConstraintRelation,
    required: &PropertyValue,
) -> PropertyConstraint {
    PropertyConstraint {
        property_kind: property(kind),
        relation,
        required: required.clone(),
    }
}

fn integer(value: u32) -> PropertyValue {
    PropertyValue::new(
        ScalarValue::Integer(
            ExactInteger::parse(value.to_string()).expect("u32 has a valid exact representation"),
        ),
        None,
    )
    .expect("unitless integers are valid property values")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedure::thermal::program::test_program;

    #[test]
    fn derives_atomic_block_lid_and_ramp_requirements() {
        let validated = test_program().validate().unwrap();
        let formula = validated.capability_formula();
        assert_eq!(formula.binding_scope, BindingScope::AtomicAssetAssembly);
        assert_eq!(formula.all_of.len(), 3);
        assert_eq!(
            formula
                .all_of
                .iter()
                .map(|clause| clause.capability_kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
                HEATED_LID_TEMPERATURE_CONTROL,
                CONTROLLED_TEMPERATURE_RAMP,
            ]
        );
        let block = &formula.all_of[0];
        assert_eq!(block.constraints.len(), 5);
        assert!(block.constraints.iter().any(|constraint| {
            constraint.property_kind.as_str() == MINIMUM_BLOCK_TEMPERATURE
                && constraint.required.value
                    == ScalarValue::Real(lab_capability::ExactDecimal::parse("4").unwrap())
        }));
    }

    #[test]
    fn optional_controls_only_derive_when_the_program_requests_them() {
        let mut block_only = test_program();
        block_only.lid_temperature = None;
        block_only.stages[0].steps[0].ramp_rate = None;
        let formula = block_only.validate().unwrap().capability_formula();
        assert_eq!(formula.all_of.len(), 1);
        assert_eq!(
            formula.all_of[0].capability_kind.as_str(),
            PROGRAMMED_BLOCK_TEMPERATURE_CONTROL
        );
    }
}
