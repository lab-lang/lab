use std::collections::BTreeSet;

use lab_capability::{
    CapabilityKind, ConstraintRelation, ExactInteger, PropertyConstraint, PropertyKind,
    PropertyValue, ScalarValue,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vocabulary::{
    CONTROLLED_TEMPERATURE_RAMP, HEATED_LID_TEMPERATURE_CONTROL, MAXIMUM_BLOCK_TEMPERATURE,
    MAXIMUM_LID_TEMPERATURE, MAXIMUM_RAMP_RATE, MAXIMUM_SAMPLE_COUNT,
    MAXIMUM_THERMAL_SAMPLE_VOLUME, MINIMUM_BLOCK_TEMPERATURE, MINIMUM_LID_TEMPERATURE,
    MINIMUM_THERMAL_SAMPLE_VOLUME, PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
};
use crate::{
    BindingScope, CapabilityClause, CapabilityFormula, Duration, ProcedureLocalId, Temperature,
    TemperatureRampRate, Volume,
};

/// The material state carried through one canonical thermal program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThermalLoad {
    /// Zero-based input of the enclosing Procedure task that is loaded into the instrument.
    pub input: u32,
    /// Exact enclosing Procedure outputs established after the program completes.
    ///
    /// Multiple typed outputs may describe the same processed physical load. For example, heat
    /// shock establishes both a named strain product and the transformed culture that proceeds to
    /// recovery.
    pub outputs: Vec<ProcedureLocalId>,
    /// Number of independently addressable samples run under the same profile.
    pub sample_count: u32,
    /// Fill volume of each sample.
    pub volume_each: Volume,
}

/// One repeated group of ordered thermal plateaus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThermalStage {
    pub id: ProcedureLocalId,
    /// Total executions of this group; one means execute it once.
    pub repeats: u32,
    pub steps: Vec<ThermalStep>,
}

/// One plateau: reach a block temperature at an optional controlled rate, then hold it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThermalStep {
    pub id: ProcedureLocalId,
    pub temperature: Temperature,
    pub hold: Duration,
    /// An explicit target ramp rate. `None` permits the implementation's default rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_rate: Option<TemperatureRampRate>,
}

/// Version 1 of Lab's canonical, device-neutral thermal-program contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThermalProgramV1 {
    pub load: ThermalLoad,
    /// One lid setpoint applied across the program. `None` means no heated-lid requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lid_temperature: Option<Temperature>,
    pub stages: Vec<ThermalStage>,
    /// An indefinite block hold after all finite stages complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_hold: Option<Temperature>,
}

impl ThermalProgramV1 {
    pub fn validate(self) -> Result<ValidatedThermalProgramV1, ThermalProgramValidationError> {
        if self.load.sample_count == 0 {
            return Err(ThermalProgramValidationError::NoSamples);
        }
        if self.load.outputs.is_empty() {
            return Err(ThermalProgramValidationError::NoOutputs);
        }
        let mut outputs = BTreeSet::new();
        for output in &self.load.outputs {
            if !outputs.insert(output.clone()) {
                return Err(ThermalProgramValidationError::DuplicateOutput {
                    output: output.clone(),
                });
            }
        }
        if self.stages.is_empty() {
            return Err(ThermalProgramValidationError::NoStages);
        }
        let mut stage_ids = BTreeSet::new();
        let mut step_ids = BTreeSet::new();
        for stage in &self.stages {
            if !stage_ids.insert(stage.id.clone()) {
                return Err(ThermalProgramValidationError::DuplicateStage {
                    stage: stage.id.clone(),
                });
            }
            if stage.repeats == 0 {
                return Err(ThermalProgramValidationError::ZeroStageRepeats {
                    stage: stage.id.clone(),
                });
            }
            if stage.steps.is_empty() {
                return Err(ThermalProgramValidationError::EmptyStage {
                    stage: stage.id.clone(),
                });
            }
            for step in &stage.steps {
                if !step_ids.insert(step.id.clone()) {
                    return Err(ThermalProgramValidationError::DuplicateStep {
                        step: step.id.clone(),
                    });
                }
            }
        }
        Ok(ValidatedThermalProgramV1(self))
    }
}

/// A thermal program whose structure, quantities, and identities satisfy the V1 contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedThermalProgramV1(ThermalProgramV1);

impl ValidatedThermalProgramV1 {
    pub fn as_program(&self) -> &ThermalProgramV1 {
        &self.0
    }

    /// Derive exact facility demands from the thermal controls present in this program.
    pub fn capability_formula(&self) -> CapabilityFormula {
        let temperatures = self
            .0
            .stages
            .iter()
            .flat_map(|stage| stage.steps.iter().map(|step| &step.temperature))
            .chain(self.0.final_hold.iter())
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
                    &integer(self.0.load.sample_count),
                ),
                constraint(
                    MINIMUM_THERMAL_SAMPLE_VOLUME,
                    ConstraintRelation::AtMost,
                    self.0.load.volume_each.as_property_value(),
                ),
                constraint(
                    MAXIMUM_THERMAL_SAMPLE_VOLUME,
                    ConstraintRelation::AtLeast,
                    self.0.load.volume_each.as_property_value(),
                ),
            ],
        }];
        if let Some(lid) = &self.0.lid_temperature {
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
            .0
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

impl AsRef<ThermalProgramV1> for ValidatedThermalProgramV1 {
    fn as_ref(&self) -> &ThermalProgramV1 {
        self.as_program()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ThermalProgramValidationError {
    #[error("thermal program has no samples")]
    NoSamples,
    #[error("thermal program has no outputs")]
    NoOutputs,
    #[error("thermal program repeats output `{output}`")]
    DuplicateOutput { output: ProcedureLocalId },
    #[error("thermal program has no stages")]
    NoStages,
    #[error("thermal program repeats stage `{stage}`")]
    DuplicateStage { stage: ProcedureLocalId },
    #[error("thermal stage `{stage}` has no steps")]
    EmptyStage { stage: ProcedureLocalId },
    #[error("thermal stage `{stage}` has zero repeats")]
    ZeroStageRepeats { stage: ProcedureLocalId },
    #[error("thermal program repeats step `{step}`")]
    DuplicateStep { step: ProcedureLocalId },
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

    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn temperature(value: &str) -> Temperature {
        Temperature::parse_degrees_celsius(value).unwrap()
    }

    fn duration(value: &str) -> Duration {
        Duration::parse_seconds(value).unwrap()
    }

    fn program() -> ThermalProgramV1 {
        ThermalProgramV1 {
            load: ThermalLoad {
                input: 0,
                outputs: vec![id("product")],
                sample_count: 8,
                volume_each: Volume::parse_microlitres("20").unwrap(),
            },
            lid_temperature: Some(temperature("105")),
            stages: vec![ThermalStage {
                id: id("cycle"),
                repeats: 30,
                steps: vec![
                    ThermalStep {
                        id: id("denature"),
                        temperature: temperature("95"),
                        hold: duration("15"),
                        ramp_rate: Some(
                            TemperatureRampRate::parse_degrees_celsius_per_second("2.5").unwrap(),
                        ),
                    },
                    ThermalStep {
                        id: id("anneal"),
                        temperature: temperature("60"),
                        hold: duration("30"),
                        ramp_rate: None,
                    },
                ],
            }],
            final_hold: Some(temperature("4")),
        }
    }

    #[test]
    fn derives_atomic_block_lid_and_ramp_requirements() {
        let validated = program().validate().unwrap();
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
    fn structure_and_identity_validation_fail_closed() {
        let mut invalid = program();
        invalid.load.sample_count = 0;
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::NoSamples)
        ));

        let mut invalid = program();
        invalid.load.outputs.clear();
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::NoOutputs)
        ));

        let mut invalid = program();
        invalid.load.outputs.push(id("product"));
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::DuplicateOutput { .. })
        ));

        let mut invalid = program();
        invalid.stages[0].repeats = 0;
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::ZeroStageRepeats { .. })
        ));

        let mut invalid = program();
        let duplicate = invalid.stages[0].steps[0].clone();
        invalid.stages[0].steps.push(duplicate);
        assert!(matches!(
            invalid.validate(),
            Err(ThermalProgramValidationError::DuplicateStep { .. })
        ));
    }

    #[test]
    fn optional_controls_only_derive_when_the_program_requests_them() {
        let mut block_only = program();
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
