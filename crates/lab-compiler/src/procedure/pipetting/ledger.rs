use std::collections::{BTreeMap, BTreeSet};

use lab_capability::ExactDecimal;

use super::error::{PipettingProgramValidationError, VolumeConflict};
use super::operation::{AspirationStrategy, PipettingStep};
use super::program::PipettingProgramV1;
use super::vessel::{Location, Vessel, VesselRole};
use crate::procedure::ProcedureLocalId;

/// Exact liquid bookkeeping derived from ordered canonical steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidLedger {
    final_volumes: BTreeMap<Location, Option<ExactDecimal>>,
    withdrawn: BTreeMap<Location, ExactDecimal>,
    required_initial: BTreeMap<Location, ExactDecimal>,
    credited: BTreeSet<Location>,
}

impl LiquidLedger {
    /// Known final volume in microlitres, or `None` when the input fill was intentionally open.
    pub fn final_volume(&self, location: &Location) -> Option<&ExactDecimal> {
        self.final_volumes.get(location).and_then(Option::as_ref)
    }

    /// Total volume withdrawn from one logical location in microlitres.
    pub fn withdrawn(&self, location: &Location) -> Option<&ExactDecimal> {
        self.withdrawn.get(location)
    }

    /// Smallest starting volume that satisfies every precondition this program places on one
    /// position, in microlitres.
    ///
    /// This is what a source whose fill the Method leaves open must actually be loaded with. It is
    /// derived by replaying the ordered steps rather than reasoning about how often a source is
    /// remixed: a mix partway through has to fit whatever is left at that point, so a large mix
    /// late in a long series demands more than the total draw does. `None` for a position the
    /// program dispenses into, whose contents come from the program rather than from a load.
    pub fn required_initial_volume(&self, location: &Location) -> Option<&ExactDecimal> {
        if self.credited.contains(location) {
            return None;
        }
        self.required_initial.get(location)
    }
}

pub(super) fn build_liquid_ledger(
    program: &PipettingProgramV1,
) -> Result<LiquidLedger, PipettingProgramValidationError> {
    validate_source_valuation(program)?;
    let zero = ExactDecimal::parse("0").expect("zero is a valid exact decimal");
    let mut capacities = BTreeMap::new();
    let mut dead_volumes = BTreeMap::new();
    let mut final_volumes = BTreeMap::new();
    for vessel in &program.vessels {
        if let Some(capacity) = &vessel.working_capacity_each {
            capacities.insert(vessel.id.clone(), capacity.value().clone());
        }
        if let Some(dead) = &vessel.dead_volume_each {
            dead_volumes.insert(vessel.id.clone(), dead.value().clone());
        }
        // A vessel the program itself fills starts empty. Leaving it unknown would exempt it from
        // every later check, including checks on liquid it has since received.
        let initial = vessel
            .initial_volume_each
            .as_ref()
            .map(|volume| volume.value().clone())
            .or_else(|| ledger_can_value(vessel).then(|| zero.clone()));
        for position in 0..vessel.positions {
            final_volumes.insert(
                Location {
                    vessel: vessel.id.clone(),
                    position,
                },
                initial.clone(),
            );
        }
    }
    let mut withdrawn = BTreeMap::<Location, ExactDecimal>::new();
    let mut required_initial = BTreeMap::<Location, ExactDecimal>::new();
    let mut credited = BTreeSet::<Location>::new();
    for step in &program.steps {
        match step {
            PipettingStep::Transfer {
                id,
                source,
                destination,
                volume,
                ..
            } => move_liquid(
                id,
                source,
                std::slice::from_ref(destination),
                volume.value(),
                &mut final_volumes,
                &mut withdrawn,
                &mut required_initial,
                &mut credited,
                &capacities,
                &dead_volumes,
            )?,
            PipettingStep::Distribute {
                id,
                source,
                destinations,
                volume_each,
                ..
            } => move_liquid(
                id,
                source,
                destinations,
                volume_each.value(),
                &mut final_volumes,
                &mut withdrawn,
                &mut required_initial,
                &mut credited,
                &capacities,
                &dead_volumes,
            )?,
            PipettingStep::Mix {
                id,
                targets,
                volume,
                ..
            } => {
                for target in targets {
                    let consumed = withdrawn
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| zero.clone());
                    let needed = consumed.added_to(volume.value());
                    let entry = required_initial
                        .entry(target.clone())
                        .or_insert_with(|| zero.clone());
                    if *entry < needed {
                        *entry = needed;
                    }
                    if let Some(Some(available)) = final_volumes.get(target)
                        && available < volume.value()
                    {
                        return Err(PipettingProgramValidationError::InsufficientMixVolume {
                            step: id.clone(),
                            vessel: target.vessel.clone(),
                            position: target.position,
                            required: volume.value().to_string(),
                            available: available.to_string(),
                        });
                    }
                }
            }
            PipettingStep::Barrier { .. } => {}
        }
    }
    Ok(LiquidLedger {
        final_volumes,
        withdrawn,
        required_initial,
        credited,
    })
}

#[allow(clippy::too_many_arguments)]
fn move_liquid(
    step: &ProcedureLocalId,
    source: &Location,
    destinations: &[Location],
    volume_each: &ExactDecimal,
    volumes: &mut BTreeMap<Location, Option<ExactDecimal>>,
    withdrawn: &mut BTreeMap<Location, ExactDecimal>,
    required_initial: &mut BTreeMap<Location, ExactDecimal>,
    credited: &mut BTreeSet<Location>,
    capacities: &BTreeMap<ProcedureLocalId, ExactDecimal>,
    dead_volumes: &BTreeMap<ProcedureLocalId, ExactDecimal>,
) -> Result<(), PipettingProgramValidationError> {
    let total = volume_each.multiplied_by_u32(
        u32::try_from(destinations.len()).expect("validated positions fit in u32"),
    );
    if let Some(Some(available)) = volumes.get(source) {
        // Dead volume is liquid the tip cannot reach, so it is not available however much of it
        // the vessel holds.
        let dead = dead_volumes.get(&source.vessel);
        let reachable =
            dead.map_or_else(|| available.clone(), |dead| available.subtracted_by(dead));
        if reachable < total {
            return Err(match dead {
                Some(dead) => {
                    PipettingProgramValidationError::BelowDeadVolume(Box::new(VolumeConflict {
                        step: step.clone(),
                        vessel: source.vessel.clone(),
                        position: source.position,
                        moved: total.to_string(),
                        present: available.to_string(),
                        limit: dead.to_string(),
                    }))
                }
                None => PipettingProgramValidationError::InsufficientVolume {
                    step: step.clone(),
                    vessel: source.vessel.clone(),
                    position: source.position,
                    required: total.to_string(),
                    available: available.to_string(),
                },
            });
        }
        volumes.insert(source.clone(), Some(available.subtracted_by(&total)));
    }
    // Everything drawn so far plus this draw is a lower bound on the starting fill, and a dead
    // volume is liquid that has to remain on top of it.
    let consumed = withdrawn
        .get(source)
        .cloned()
        .unwrap_or_else(|| ExactDecimal::parse("0").expect("zero is a valid exact decimal"));
    let mut needed = consumed.added_to(&total);
    if let Some(dead) = dead_volumes.get(&source.vessel) {
        needed = needed.added_to(dead);
    }
    let entry = required_initial
        .entry(source.clone())
        .or_insert_with(|| ExactDecimal::parse("0").expect("zero is a valid exact decimal"));
    if *entry < needed {
        *entry = needed;
    }
    withdrawn
        .entry(source.clone())
        .and_modify(|current| *current = current.added_to(&total))
        .or_insert(total);
    for destination in destinations {
        credited.insert(destination.clone());
        if let Some(Some(current)) = volumes.get(destination) {
            let filled = current.added_to(volume_each);
            if let Some(capacity) = capacities.get(&destination.vessel)
                && &filled > capacity
            {
                return Err(PipettingProgramValidationError::ExceedsWorkingCapacity(
                    Box::new(VolumeConflict {
                        step: step.clone(),
                        vessel: destination.vessel.clone(),
                        position: destination.position,
                        moved: volume_each.to_string(),
                        present: current.to_string(),
                        limit: capacity.to_string(),
                    }),
                ));
            }
            volumes.insert(destination.clone(), Some(filled));
        }
    }
    Ok(())
}

/// Whether the ledger can follow this vessel's volume from the program alone.
///
/// A stated fill is one way. The other is a vessel the program itself fills from empty, whose
/// volume is therefore whatever the steps put into it.
fn ledger_can_value(vessel: &Vessel) -> bool {
    vessel.initial_volume_each.is_some()
        || matches!(
            vessel.role,
            VesselRole::Product { .. }
                | VesselRole::MaterialProduct { .. }
                | VesselRole::Intermediate
        )
}

/// Proves every aspiration draws from a position whose volume the compiler can follow.
///
/// A material source may leave its fill open, because the adapter computes a load that covers the
/// planned withdrawals. Anything else arrived from an upstream task with a known volume, and
/// leaving it unstated would exempt it from every volume check rather than merely leaving one
/// number blank.
fn validate_source_valuation(
    program: &PipettingProgramV1,
) -> Result<(), PipettingProgramValidationError> {
    let vessels = program
        .vessels
        .iter()
        .map(|vessel| (&vessel.id, vessel))
        .collect::<BTreeMap<_, _>>();
    for step in &program.steps {
        let (id, source, tracked) = match step {
            PipettingStep::Transfer {
                id,
                source,
                technique,
                ..
            } => (
                id,
                source,
                matches!(
                    technique.aspiration,
                    AspirationStrategy::TrackedLiquidSurface
                ),
            ),
            PipettingStep::Distribute {
                id,
                source,
                technique,
                ..
            } => (
                id,
                source,
                matches!(
                    technique.aspiration,
                    AspirationStrategy::TrackedLiquidSurface
                ),
            ),
            PipettingStep::Mix {
                id,
                targets,
                technique,
                ..
            } => {
                let tracked = matches!(
                    technique.aspiration,
                    AspirationStrategy::TrackedLiquidSurface
                );
                for target in targets {
                    let Some(vessel) = vessels.get(&target.vessel) else {
                        continue;
                    };
                    if ledger_can_value(vessel) {
                        continue;
                    }
                    if tracked {
                        return Err(PipettingProgramValidationError::UntrackableSource {
                            step: id.clone(),
                            vessel: target.vessel.clone(),
                        });
                    }
                    if !matches!(vessel.role, VesselRole::MaterialSource { .. }) {
                        return Err(PipettingProgramValidationError::UnvaluedSourceAspiration {
                            step: id.clone(),
                            vessel: target.vessel.clone(),
                        });
                    }
                }
                continue;
            }
            PipettingStep::Barrier { .. } => continue,
        };
        let Some(vessel) = vessels.get(&source.vessel) else {
            continue;
        };
        if ledger_can_value(vessel) {
            continue;
        }
        if tracked {
            return Err(PipettingProgramValidationError::UntrackableSource {
                step: id.clone(),
                vessel: source.vessel.clone(),
            });
        }
        if !matches!(vessel.role, VesselRole::MaterialSource { .. }) {
            return Err(PipettingProgramValidationError::UnvaluedSourceAspiration {
                step: id.clone(),
                vessel: source.vessel.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::error::PipettingProgramValidationError;
    use super::super::operation::{
        AspirationStrategy, FluidPathPolicy, MixTechnique, PipettingStep, TransferTechnique,
    };
    use super::super::program::{
        MaterialInput, MaterialOutput, PipettingConstraints, PipettingProgramV1,
        test_support::{example, id, location},
    };
    use super::super::vessel::{Vessel, VesselRole};
    use crate::procedure::Volume;

    /// Builds a one-source, one-destination program with the given limits.
    fn limited(
        source_fill: Option<&str>,
        dead: Option<&str>,
        capacity: Option<&str>,
        transfer_ul: &str,
        aspiration: AspirationStrategy,
    ) -> PipettingProgramV1 {
        PipettingProgramV1::new(
            Vec::new(),
            vec![MaterialOutput { id: id("product") }],
            vec![
                Vessel {
                    id: id("source"),
                    role: VesselRole::Intermediate,
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: dead.map(|value| Volume::parse_microlitres(value).unwrap()),
                    initial_volume_each: source_fill
                        .map(|value| Volume::parse_microlitres(value).unwrap()),
                    temperature: None,
                },
                Vessel {
                    id: id("destination"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 1,
                    working_capacity_each: capacity
                        .map(|value| Volume::parse_microlitres(value).unwrap()),
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
            ],
            vec![PipettingStep::Transfer {
                id: id("transfer"),
                source: location("source", 0),
                destination: location("destination", 0),
                volume: Volume::parse_microlitres(transfer_ul).unwrap(),
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: TransferTechnique {
                    aspiration,
                    ..TransferTechnique::default()
                },
            }],
            PipettingConstraints::default(),
        )
    }

    #[test]
    fn required_initial_volume_accounts_for_a_mix_partway_through() {
        // Four 2 uL draws with a 5 uL mix before each. The tube is down to 2 uL before the last
        // mix, so it has to start with 5 + 3 x 2 = 11 uL, not the 8 uL the draws total.
        let mut steps = Vec::new();
        for index in 0..4u32 {
            steps.push(PipettingStep::Mix {
                id: id(&format!("mix-{index}")),
                targets: vec![location("source", 0)],
                cycles: 1,
                volume: Volume::parse_microlitres("5").unwrap(),
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: MixTechnique::default(),
            });
            steps.push(PipettingStep::Transfer {
                id: id(&format!("draw-{index}")),
                source: location("source", 0),
                destination: location("plate", index),
                volume: Volume::parse_microlitres("2").unwrap(),
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: TransferTechnique::default(),
            });
        }
        let program = PipettingProgramV1::new(
            vec![MaterialInput { id: id("dna") }],
            vec![MaterialOutput { id: id("product") }],
            vec![
                Vessel {
                    id: id("source"),
                    role: VesselRole::MaterialSource {
                        material: id("dna"),
                    },
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: id("plate"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 4,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
            ],
            steps,
            PipettingConstraints::default(),
        )
        .validate()
        .unwrap();

        let ledger = program.liquid_ledger();
        let source = location("source", 0);
        assert_eq!(ledger.withdrawn(&source).unwrap().to_string(), "8");
        assert_eq!(
            ledger.required_initial_volume(&source).unwrap().to_string(),
            "11",
            "a mix late in the series needs more than the total draw"
        );
        // A position the program fills is not something an operator loads.
        assert!(
            ledger
                .required_initial_volume(&location("plate", 0))
                .is_none()
        );
    }

    #[test]
    fn required_initial_volume_reserves_the_dead_volume() {
        let program = limited(None, Some("30"), None, "70", AspirationStrategy::Liquid);
        let mut program = program;
        program.materials.push(MaterialInput { id: id("water") });
        program.vessels[0].role = VesselRole::MaterialSource {
            material: id("water"),
        };
        let validated = program.validate().unwrap();
        assert_eq!(
            validated
                .liquid_ledger()
                .required_initial_volume(&location("source", 0))
                .unwrap()
                .to_string(),
            "100",
            "70 uL drawn on top of 30 uL that cannot be reached"
        );
    }

    #[test]
    fn a_source_cannot_be_drawn_into_its_dead_volume() {
        // 100 uL present, 30 uL of it unreachable, so 80 uL is not available even though the
        // vessel holds more than that.
        let error = limited(
            Some("100"),
            Some("30"),
            None,
            "80",
            AspirationStrategy::Liquid,
        )
        .validate()
        .unwrap_err();
        assert!(
            matches!(
                error,
                PipettingProgramValidationError::BelowDeadVolume(ref conflict)
                    if conflict.moved == "80" && conflict.limit == "30"
            ),
            "{error}"
        );

        limited(
            Some("100"),
            Some("30"),
            None,
            "70",
            AspirationStrategy::Liquid,
        )
        .validate()
        .expect("drawing down to the dead volume is allowed");
    }

    #[test]
    fn a_destination_cannot_be_filled_past_its_working_capacity() {
        let error = limited(
            Some("500"),
            None,
            Some("200"),
            "300",
            AspirationStrategy::Liquid,
        )
        .validate()
        .unwrap_err();
        assert!(
            matches!(
                error,
                PipettingProgramValidationError::ExceedsWorkingCapacity(ref conflict)
                    if conflict.limit == "200" && conflict.moved == "300"
            ),
            "{error}"
        );
    }

    #[test]
    fn validation_builds_an_exact_liquid_ledger_and_rejects_underflow() {
        let validated = example().validate().unwrap();
        assert_eq!(
            validated
                .liquid_ledger()
                .withdrawn(&location("water-source", 0))
                .unwrap()
                .to_string(),
            "3"
        );
        assert_eq!(
            validated
                .liquid_ledger()
                .final_volume(&location("reactions", 0))
                .unwrap()
                .to_string(),
            "2.5"
        );
        assert_eq!(
            validated
                .liquid_ledger()
                .final_volume(&location("reactions", 1))
                .unwrap()
                .to_string(),
            "0.5"
        );

        let mut insufficient = example();
        insufficient.vessels[0].initial_volume_each =
            Some(Volume::parse_microlitres("2.5").unwrap());
        assert!(matches!(
            insufficient.validate(),
            Err(PipettingProgramValidationError::InsufficientVolume { .. })
        ));
    }

    #[test]
    fn an_unvalued_source_cannot_be_aspirated_unless_an_adapter_loads_it() {
        // An unstated fill used to exempt a vessel from every volume check rather than leaving one
        // number blank, so this is rejected outright.
        let mut arrives_filled = limited(None, None, None, "10", AspirationStrategy::Liquid);
        arrives_filled.vessels[0].role = VesselRole::ProcedureInput { input: 0 };
        let error = arrives_filled.validate().unwrap_err();
        assert!(
            matches!(
                error,
                PipettingProgramValidationError::UnvaluedSourceAspiration { ref vessel, .. }
                    if vessel.as_str() == "source"
            ),
            "a value arriving from an upstream task has a knowable volume: {error}"
        );

        // A material source is the exception: the adapter computes a load covering the plan.
        let mut program = limited(None, None, None, "10", AspirationStrategy::Liquid);
        program.materials.push(MaterialInput { id: id("water") });
        program.vessels[0].role = VesselRole::MaterialSource {
            material: id("water"),
        };
        program
            .validate()
            .expect("a material source may leave its fill to the adapter");
    }

    #[test]
    fn a_mix_cannot_draw_from_a_source_the_plan_cannot_follow() {
        // A mix draws and returns liquid in place, so its target needs a volume the ledger can
        // follow just as an aspiration source does.
        let program = PipettingProgramV1::new(
            Vec::new(),
            vec![MaterialOutput { id: id("product") }],
            vec![
                Vessel {
                    id: id("source"),
                    role: VesselRole::ProcedureInput { input: 0 },
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: id("plate"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
            ],
            vec![PipettingStep::Mix {
                id: id("mix-source"),
                targets: vec![location("source", 0)],
                cycles: 2,
                volume: Volume::parse_microlitres("5").unwrap(),
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: MixTechnique::default(),
            }],
            PipettingConstraints::default(),
        );

        let error = program.validate().unwrap_err();
        assert!(
            matches!(
                error,
                PipettingProgramValidationError::UnvaluedSourceAspiration { ref vessel, .. }
                    if vessel.as_str() == "source"
            ),
            "{error}"
        );
    }

    #[test]
    fn a_tracked_surface_requires_a_source_the_plan_can_follow() {
        let mut program = limited(
            None,
            None,
            None,
            "10",
            AspirationStrategy::TrackedLiquidSurface,
        );
        program.materials.push(MaterialInput { id: id("water") });
        program.vessels[0].role = VesselRole::MaterialSource {
            material: id("water"),
        };
        let error = program.clone().validate().unwrap_err();
        assert!(
            matches!(
                error,
                PipettingProgramValidationError::UntrackableSource { ref vessel, .. }
                    if vessel.as_str() == "source"
            ),
            "following a falling surface needs a stated starting volume: {error}"
        );

        program.vessels[0].initial_volume_each = Some(Volume::parse_microlitres("500").unwrap());
        program
            .validate()
            .expect("a stated fill makes the surface followable");
    }
}
