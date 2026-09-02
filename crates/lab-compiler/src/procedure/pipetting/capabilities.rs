use lab_capability::{CapabilityKind, ConstraintRelation, PropertyConstraint, PropertyKind};

use super::operation::{AspirationStrategy, DispenseStrategy, PipettingStep};
use super::validation::ValidatedPipettingProgramV1;
use super::vessel::Vessel;
use crate::procedure::vocabulary::{
    AIR_GAP_HANDLING, IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, MAXIMUM_AIR_GAP_VOLUME,
    MAXIMUM_MIX_VOLUME, MAXIMUM_TEMPERATURE, MAXIMUM_TRANSFER_VOLUME, METERED_LIQUID_TRANSFER,
    MINIMUM_TEMPERATURE, MINIMUM_TRANSFER_VOLUME, POST_DISPENSE_BLOWOUT,
    TEMPERATURE_CONTROLLED_STAGING, TOUCH_TIP, VESSEL_RELATIVE_LIQUID_ACCESS,
};
use crate::procedure::{
    BindingScope, CapabilityClause, CapabilityFormula, ProcedureLocalId, TemperatureRange, Volume,
};

impl ValidatedPipettingProgramV1 {
    /// Derive exact facility demands from the operations present in this program.
    pub fn capability_formula(&self) -> CapabilityFormula {
        let mut transfer_minimum: Option<&Volume> = None;
        let mut transfer_maximum: Option<&Volume> = None;
        let mut mix_maximum: Option<&Volume> = None;
        let mut maximum_air_gap: Option<&Volume> = None;
        let mut tracked_aspiration = false;
        let mut vessel_relative_access = false;
        let mut blow_out = false;
        let mut touch_tip = false;
        for step in &self.as_program().steps {
            match step {
                PipettingStep::Transfer {
                    volume, technique, ..
                }
                | PipettingStep::Distribute {
                    volume_each: volume,
                    technique,
                    ..
                } => {
                    transfer_minimum = minimum_volume(transfer_minimum, volume);
                    transfer_maximum = maximum_volume(transfer_maximum, volume);
                    maximum_air_gap = technique
                        .air_gap
                        .as_ref()
                        .map_or(maximum_air_gap, |volume| {
                            maximum_volume(maximum_air_gap, volume)
                        });
                    collect_technique(
                        &technique.aspiration,
                        &technique.dispense,
                        technique.blow_out,
                        technique.touch_tip,
                        &mut tracked_aspiration,
                        &mut vessel_relative_access,
                        &mut blow_out,
                        &mut touch_tip,
                    );
                }
                PipettingStep::Mix {
                    volume, technique, ..
                } => {
                    mix_maximum = maximum_volume(mix_maximum, volume);
                    collect_technique(
                        &technique.aspiration,
                        &technique.dispense,
                        technique.blow_out,
                        technique.touch_tip,
                        &mut tracked_aspiration,
                        &mut vessel_relative_access,
                        &mut blow_out,
                        &mut touch_tip,
                    );
                }
                PipettingStep::Barrier { .. } => {}
            }
        }

        let mut all_of = Vec::new();
        if let (Some(minimum), Some(maximum)) = (transfer_minimum, transfer_maximum) {
            all_of.push(CapabilityClause {
                role: local("transfer"),
                capability_kind: capability(METERED_LIQUID_TRANSFER),
                constraints: vec![
                    constraint(MINIMUM_TRANSFER_VOLUME, ConstraintRelation::AtMost, minimum),
                    constraint(
                        MAXIMUM_TRANSFER_VOLUME,
                        ConstraintRelation::AtLeast,
                        maximum,
                    ),
                ],
            });
        }
        if let Some(maximum) = mix_maximum {
            all_of.push(CapabilityClause {
                role: local("mix"),
                capability_kind: capability(IN_WELL_MIXING),
                constraints: vec![constraint(
                    MAXIMUM_MIX_VOLUME,
                    ConstraintRelation::AtLeast,
                    maximum,
                )],
            });
        }
        if let Some(temperature) = staged_temperature_envelope(&self.as_program().vessels) {
            all_of.push(CapabilityClause {
                role: local("source-temperature"),
                capability_kind: capability(TEMPERATURE_CONTROLLED_STAGING),
                constraints: vec![
                    PropertyConstraint {
                        property_kind: property(MINIMUM_TEMPERATURE),
                        relation: ConstraintRelation::AtMost,
                        required: temperature.minimum.as_property_value().clone(),
                    },
                    PropertyConstraint {
                        property_kind: property(MAXIMUM_TEMPERATURE),
                        relation: ConstraintRelation::AtLeast,
                        required: temperature.maximum.as_property_value().clone(),
                    },
                ],
            });
        }
        if tracked_aspiration {
            all_of.push(feature_clause(
                "tracked-aspiration",
                LIQUID_LEVEL_AWARE_ASPIRATION,
            ));
        }
        if vessel_relative_access {
            all_of.push(feature_clause(
                "vessel-relative-access",
                VESSEL_RELATIVE_LIQUID_ACCESS,
            ));
        }
        if let Some(maximum) = maximum_air_gap {
            all_of.push(CapabilityClause {
                role: local("air-gap"),
                capability_kind: capability(AIR_GAP_HANDLING),
                constraints: vec![constraint(
                    MAXIMUM_AIR_GAP_VOLUME,
                    ConstraintRelation::AtLeast,
                    maximum,
                )],
            });
        }
        if blow_out {
            all_of.push(feature_clause("blowout", POST_DISPENSE_BLOWOUT));
        }
        if touch_tip {
            all_of.push(feature_clause("touch-tip", TOUCH_TIP));
        }
        CapabilityFormula {
            binding_scope: BindingScope::AtomicAssetAssembly,
            all_of,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_technique(
    aspiration: &AspirationStrategy,
    dispense: &DispenseStrategy,
    step_blow_out: bool,
    step_touch_tip: bool,
    tracked_aspiration: &mut bool,
    vessel_relative_access: &mut bool,
    blow_out: &mut bool,
    touch_tip: &mut bool,
) {
    *tracked_aspiration |= matches!(aspiration, AspirationStrategy::TrackedLiquidSurface);
    *vessel_relative_access |= matches!(aspiration, AspirationStrategy::VesselBottom { .. })
        || !matches!(dispense, DispenseStrategy::Liquid);
    *blow_out |= step_blow_out;
    *touch_tip |= step_touch_tip;
}

/// The envelope a staging device must cover to satisfy every temperature-constrained vessel.
///
/// A device that stages several constrained vessels must reach the coldest stated minimum and the
/// warmest stated maximum, so the clause widens to the union rather than picking one vessel.
pub fn staged_temperature_envelope(vessels: &[Vessel]) -> Option<TemperatureRange> {
    let mut envelope: Option<TemperatureRange> = None;
    for vessel in vessels {
        let Some(temperature) = &vessel.temperature else {
            continue;
        };
        envelope = Some(match envelope {
            None => temperature.clone(),
            Some(current) => TemperatureRange {
                minimum: if temperature.minimum.value() < current.minimum.value() {
                    temperature.minimum.clone()
                } else {
                    current.minimum
                },
                maximum: if temperature.maximum.value() > current.maximum.value() {
                    temperature.maximum.clone()
                } else {
                    current.maximum
                },
            },
        });
    }
    envelope
}

fn feature_clause(role: &str, kind: &str) -> CapabilityClause {
    CapabilityClause {
        role: local(role),
        capability_kind: capability(kind),
        constraints: Vec::new(),
    }
}

fn minimum_volume<'a>(current: Option<&'a Volume>, candidate: &'a Volume) -> Option<&'a Volume> {
    Some(match current {
        Some(current) if current.value() <= candidate.value() => current,
        _ => candidate,
    })
}

fn maximum_volume<'a>(current: Option<&'a Volume>, candidate: &'a Volume) -> Option<&'a Volume> {
    Some(match current {
        Some(current) if current.value() >= candidate.value() => current,
        _ => candidate,
    })
}

fn constraint(kind: &str, relation: ConstraintRelation, volume: &Volume) -> PropertyConstraint {
    PropertyConstraint {
        property_kind: property(kind),
        relation,
        required: volume.as_property_value().clone(),
    }
}

fn local(value: &str) -> ProcedureLocalId {
    ProcedureLocalId::new(value).expect("built-in role is a valid local identity")
}

fn capability(value: &str) -> CapabilityKind {
    CapabilityKind::new(value).expect("built-in capability kind is an absolute IRI")
}

fn property(value: &str) -> PropertyKind {
    PropertyKind::new(value).expect("built-in property kind is an absolute IRI")
}

#[cfg(test)]
mod tests {
    use super::super::operation::{
        AspirationStrategy, DispenseStrategy, PipettingStep, TransferTechnique,
    };
    use super::super::program::{PipettingProgramV1, test_support::example};
    use crate::procedure::{BindingScope, Length, Volume, vocabulary};

    #[test]
    fn exact_program_round_trips_and_derives_narrow_capabilities() {
        let program = example().validate().unwrap();
        let json = serde_json::to_string_pretty(program.as_program()).unwrap();
        let round_trip = serde_json::from_str::<PipettingProgramV1>(&json)
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(round_trip, program);

        let formula = program.capability_formula();
        assert_eq!(formula.binding_scope, BindingScope::AtomicAssetAssembly);
        assert_eq!(formula.all_of.len(), 3);
        assert_eq!(
            formula
                .all_of
                .iter()
                .map(|clause| clause.capability_kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                vocabulary::METERED_LIQUID_TRANSFER,
                vocabulary::IN_WELL_MIXING,
                vocabulary::TEMPERATURE_CONTROLLED_STAGING,
            ]
        );
        assert!(
            formula
                .all_of
                .iter()
                .all(|clause| clause.capability_kind.as_str()
                    != "https://sbol.io/ns/capability#LiquidHandling")
        );
        let transfer = &formula.all_of[0];
        assert_eq!(transfer.constraints.len(), 2);
        assert_eq!(
            transfer.constraints[0].required.value,
            lab_capability::ScalarValue::Real(lab_capability::ExactDecimal::parse("0.5").unwrap())
        );
        assert_eq!(
            transfer.constraints[1].required.value,
            lab_capability::ScalarValue::Real(lab_capability::ExactDecimal::parse("2").unwrap())
        );
    }

    #[test]
    fn techniques_derive_exact_additional_capabilities() {
        let mut program = example();
        // Tracking a falling surface means the compiler must know where the surface starts.
        program.vessels[0].initial_volume_each = Some(Volume::parse_microlitres("500").unwrap());
        let PipettingStep::Distribute { technique, .. } = &mut program.steps[0] else {
            unreachable!()
        };
        *technique = TransferTechnique {
            aspiration: AspirationStrategy::TrackedLiquidSurface,
            dispense: DispenseStrategy::AboveLiquid,
            air_gap: Some(Volume::parse_microlitres("10").unwrap()),
            blow_out: true,
            touch_tip: true,
        };
        let PipettingStep::Mix { technique, .. } = &mut program.steps[2] else {
            unreachable!()
        };
        technique.dispense = DispenseStrategy::VesselBottom {
            offset: Length::parse_millimetres("8").unwrap(),
        };

        let formula = program.validate().unwrap().capability_formula();
        assert_eq!(
            formula
                .all_of
                .iter()
                .map(|clause| clause.capability_kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                vocabulary::METERED_LIQUID_TRANSFER,
                vocabulary::IN_WELL_MIXING,
                vocabulary::TEMPERATURE_CONTROLLED_STAGING,
                vocabulary::LIQUID_LEVEL_AWARE_ASPIRATION,
                vocabulary::VESSEL_RELATIVE_LIQUID_ACCESS,
                vocabulary::AIR_GAP_HANDLING,
                vocabulary::POST_DISPENSE_BLOWOUT,
                vocabulary::TOUCH_TIP,
            ]
        );
        let air_gap = &formula.all_of[5];
        assert_eq!(air_gap.constraints.len(), 1);
        assert_eq!(
            air_gap.constraints[0].property_kind.as_str(),
            vocabulary::MAXIMUM_AIR_GAP_VOLUME
        );
    }
}
