use std::collections::BTreeSet;

use lab_capability::{CapabilityKind, ConstraintRelation, PropertyConstraint, PropertyKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vocabulary::{
    IN_WELL_MIXING, MAXIMUM_MIX_VOLUME, MAXIMUM_TEMPERATURE, MAXIMUM_TRANSFER_VOLUME,
    METERED_LIQUID_TRANSFER, MINIMUM_TEMPERATURE, MINIMUM_TRANSFER_VOLUME,
    TEMPERATURE_CONTROLLED_STAGING,
};
use crate::{
    BindingScope, CapabilityClause, CapabilityFormula, ProcedureLocalId, TemperatureRange, Volume,
};

/// One material made available to a canonical pipetting program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialInput {
    pub id: ProcedureLocalId,
}

/// One material state produced by a canonical pipetting program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialOutput {
    pub id: ProcedureLocalId,
}

/// The semantic role of one logical vessel before any deck or well allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VesselRole {
    /// A liquid value arriving through the enclosing Procedure task's zero-based input list.
    ProcedureInput {
        input: u32,
    },
    MaterialSource {
        material: ProcedureLocalId,
    },
    Product {
        output: ProcedureLocalId,
    },
    Intermediate,
}

/// A logical vessel with zero-based addressable positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Vessel {
    pub id: ProcedureLocalId,
    pub role: VesselRole,
    pub positions: u32,
}

/// One logical position in a Procedure vessel.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    pub vessel: ProcedureLocalId,
    pub position: u32,
}

/// The strongest fluid-path reuse a realization may perform for one semantic operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FluidPathPolicy {
    /// Each destination must use an isolated fluid path.
    IsolatedDestinations,
    /// Destinations may share one fluid path loaded from the same source, but that path must not
    /// re-enter the source after contacting a destination.
    SharedSourceNoReentry,
}

/// One stable, observable liquid operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PipettingStep {
    Transfer {
        id: ProcedureLocalId,
        source: Location,
        destination: Location,
        volume: Volume,
        fluid_path: FluidPathPolicy,
    },
    Distribute {
        id: ProcedureLocalId,
        source: Location,
        destinations: Vec<Location>,
        volume_each: Volume,
        fluid_path: FluidPathPolicy,
    },
    Mix {
        id: ProcedureLocalId,
        targets: Vec<Location>,
        cycles: u32,
        volume: Volume,
        fluid_path: FluidPathPolicy,
    },
    Barrier {
        id: ProcedureLocalId,
        reason: String,
    },
}

impl PipettingStep {
    pub fn id(&self) -> &ProcedureLocalId {
        match self {
            Self::Transfer { id, .. }
            | Self::Distribute { id, .. }
            | Self::Mix { id, .. }
            | Self::Barrier { id, .. } => id,
        }
    }
}

/// Cross-cutting conditions that every realization of the program must preserve.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PipettingConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_temperature: Option<TemperatureRange>,
}

/// Version 1 of Lab's canonical, device-neutral pipetting contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PipettingProgramV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<MaterialInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<MaterialOutput>,
    pub vessels: Vec<Vessel>,
    pub steps: Vec<PipettingStep>,
    #[serde(default)]
    pub constraints: PipettingConstraints,
}

impl PipettingProgramV1 {
    pub fn new(
        materials: Vec<MaterialInput>,
        outputs: Vec<MaterialOutput>,
        vessels: Vec<Vessel>,
        steps: Vec<PipettingStep>,
        constraints: PipettingConstraints,
    ) -> Self {
        Self {
            materials,
            outputs,
            vessels,
            steps,
            constraints,
        }
    }

    pub fn validate(self) -> Result<ValidatedPipettingProgramV1, PipettingProgramValidationError> {
        if self.vessels.is_empty() {
            return Err(PipettingProgramValidationError::NoVessels);
        }
        if self.steps.is_empty() {
            return Err(PipettingProgramValidationError::NoSteps);
        }

        let mut material_ids = BTreeSet::new();
        for material in &self.materials {
            if !material_ids.insert(material.id.clone()) {
                return Err(PipettingProgramValidationError::DuplicateMaterial {
                    material: material.id.clone(),
                });
            }
        }
        let mut output_ids = BTreeSet::new();
        for output in &self.outputs {
            if !output_ids.insert(output.id.clone()) {
                return Err(PipettingProgramValidationError::DuplicateOutput {
                    output: output.id.clone(),
                });
            }
        }
        let mut vessel_ids = BTreeSet::new();
        for vessel in &self.vessels {
            if !vessel_ids.insert(vessel.id.clone()) {
                return Err(PipettingProgramValidationError::DuplicateVessel {
                    vessel: vessel.id.clone(),
                });
            }
            if vessel.positions == 0 {
                return Err(PipettingProgramValidationError::EmptyVessel {
                    vessel: vessel.id.clone(),
                });
            }
            if let VesselRole::MaterialSource { material } = &vessel.role
                && !material_ids.contains(material)
            {
                return Err(PipettingProgramValidationError::UnknownMaterial {
                    vessel: vessel.id.clone(),
                    material: material.clone(),
                });
            }
            if let VesselRole::Product { output } = &vessel.role
                && !output_ids.contains(output)
            {
                return Err(PipettingProgramValidationError::UnknownOutput {
                    vessel: vessel.id.clone(),
                    output: output.clone(),
                });
            }
        }

        let vessels = self
            .vessels
            .iter()
            .map(|vessel| (&vessel.id, vessel.positions))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut step_ids = BTreeSet::new();
        for step in &self.steps {
            if !step_ids.insert(step.id().clone()) {
                return Err(PipettingProgramValidationError::DuplicateStep {
                    step: step.id().clone(),
                });
            }
            match step {
                PipettingStep::Transfer {
                    id,
                    source,
                    destination,
                    ..
                } => {
                    validate_location(id, source, &vessels)?;
                    validate_location(id, destination, &vessels)?;
                    if source == destination {
                        return Err(PipettingProgramValidationError::SelfTransfer {
                            step: id.clone(),
                        });
                    }
                }
                PipettingStep::Distribute {
                    id,
                    source,
                    destinations,
                    ..
                } => {
                    validate_location(id, source, &vessels)?;
                    if destinations.is_empty() {
                        return Err(PipettingProgramValidationError::EmptyTargets {
                            step: id.clone(),
                        });
                    }
                    require_unique_targets(id, destinations)?;
                    for destination in destinations {
                        validate_location(id, destination, &vessels)?;
                        if source == destination {
                            return Err(PipettingProgramValidationError::SelfTransfer {
                                step: id.clone(),
                            });
                        }
                    }
                }
                PipettingStep::Mix {
                    id,
                    targets,
                    cycles,
                    ..
                } => {
                    if targets.is_empty() {
                        return Err(PipettingProgramValidationError::EmptyTargets {
                            step: id.clone(),
                        });
                    }
                    require_unique_targets(id, targets)?;
                    if *cycles == 0 {
                        return Err(PipettingProgramValidationError::ZeroMixCycles {
                            step: id.clone(),
                        });
                    }
                    for target in targets {
                        validate_location(id, target, &vessels)?;
                    }
                }
                PipettingStep::Barrier { id, reason } => {
                    if reason.trim().is_empty() {
                        return Err(PipettingProgramValidationError::EmptyBarrierReason {
                            step: id.clone(),
                        });
                    }
                }
            }
        }
        if !self
            .steps
            .iter()
            .any(|step| !matches!(step, PipettingStep::Barrier { .. }))
        {
            return Err(PipettingProgramValidationError::NoLiquidOperations);
        }
        Ok(ValidatedPipettingProgramV1(self))
    }
}

/// A pipetting program whose contract, graph, references, and operation bounds are valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPipettingProgramV1(PipettingProgramV1);

impl ValidatedPipettingProgramV1 {
    pub fn as_program(&self) -> &PipettingProgramV1 {
        &self.0
    }

    /// Derive exact facility demands from the operations present in this program.
    pub fn capability_formula(&self) -> CapabilityFormula {
        let mut transfer_minimum: Option<&Volume> = None;
        let mut transfer_maximum: Option<&Volume> = None;
        let mut mix_maximum: Option<&Volume> = None;
        for step in &self.0.steps {
            match step {
                PipettingStep::Transfer { volume, .. }
                | PipettingStep::Distribute {
                    volume_each: volume,
                    ..
                } => {
                    transfer_minimum = minimum_volume(transfer_minimum, volume);
                    transfer_maximum = maximum_volume(transfer_maximum, volume);
                }
                PipettingStep::Mix { volume, .. } => {
                    mix_maximum = maximum_volume(mix_maximum, volume);
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
        if let Some(temperature) = &self.0.constraints.source_temperature {
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
        CapabilityFormula {
            binding_scope: BindingScope::AtomicAssetAssembly,
            all_of,
        }
    }
}

impl AsRef<PipettingProgramV1> for ValidatedPipettingProgramV1 {
    fn as_ref(&self) -> &PipettingProgramV1 {
        self.as_program()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PipettingProgramValidationError {
    #[error("pipetting program contains no logical vessels")]
    NoVessels,
    #[error("pipetting program contains no operations")]
    NoSteps,
    #[error("pipetting program contains no liquid operations")]
    NoLiquidOperations,
    #[error("pipetting program repeats material `{material}`")]
    DuplicateMaterial { material: ProcedureLocalId },
    #[error("pipetting program repeats output `{output}`")]
    DuplicateOutput { output: ProcedureLocalId },
    #[error("pipetting program repeats vessel `{vessel}`")]
    DuplicateVessel { vessel: ProcedureLocalId },
    #[error("pipetting vessel `{vessel}` has no addressable positions")]
    EmptyVessel { vessel: ProcedureLocalId },
    #[error("pipetting vessel `{vessel}` refers to unknown material `{material}`")]
    UnknownMaterial {
        vessel: ProcedureLocalId,
        material: ProcedureLocalId,
    },
    #[error("pipetting vessel `{vessel}` refers to unknown output `{output}`")]
    UnknownOutput {
        vessel: ProcedureLocalId,
        output: ProcedureLocalId,
    },
    #[error("pipetting program repeats step `{step}`")]
    DuplicateStep { step: ProcedureLocalId },
    #[error("pipetting step `{step}` refers to unknown vessel `{vessel}`")]
    UnknownVessel {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
    },
    #[error(
        "pipetting step `{step}` refers to position {position} outside vessel `{vessel}` with {positions} positions"
    )]
    PositionOutOfRange {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
        positions: u32,
    },
    #[error("pipetting step `{step}` has no targets")]
    EmptyTargets { step: ProcedureLocalId },
    #[error("pipetting step `{step}` repeats target `{vessel}` position {position}")]
    DuplicateTarget {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
    },
    #[error("pipetting step `{step}` transfers a location into itself")]
    SelfTransfer { step: ProcedureLocalId },
    #[error("pipetting step `{step}` has zero mix cycles")]
    ZeroMixCycles { step: ProcedureLocalId },
    #[error("pipetting barrier `{step}` has no reason")]
    EmptyBarrierReason { step: ProcedureLocalId },
}

fn validate_location(
    step: &ProcedureLocalId,
    location: &Location,
    vessels: &std::collections::BTreeMap<&ProcedureLocalId, u32>,
) -> Result<(), PipettingProgramValidationError> {
    let Some(positions) = vessels.get(&location.vessel) else {
        return Err(PipettingProgramValidationError::UnknownVessel {
            step: step.clone(),
            vessel: location.vessel.clone(),
        });
    };
    if location.position >= *positions {
        return Err(PipettingProgramValidationError::PositionOutOfRange {
            step: step.clone(),
            vessel: location.vessel.clone(),
            position: location.position,
            positions: *positions,
        });
    }
    Ok(())
}

fn require_unique_targets(
    step: &ProcedureLocalId,
    targets: &[Location],
) -> Result<(), PipettingProgramValidationError> {
    let mut unique = BTreeSet::new();
    for target in targets {
        if !unique.insert(target.clone()) {
            return Err(PipettingProgramValidationError::DuplicateTarget {
                step: step.clone(),
                vessel: target.vessel.clone(),
                position: target.position,
            });
        }
    }
    Ok(())
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
    use super::*;
    use crate::{Temperature, vocabulary};

    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn location(vessel: &str, position: u32) -> Location {
        Location {
            vessel: id(vessel),
            position,
        }
    }

    fn example() -> PipettingProgramV1 {
        PipettingProgramV1::new(
            vec![MaterialInput { id: id("water") }],
            vec![MaterialOutput { id: id("reaction") }],
            vec![
                Vessel {
                    id: id("water-source"),
                    role: VesselRole::MaterialSource {
                        material: id("water"),
                    },
                    positions: 1,
                },
                Vessel {
                    id: id("reactions"),
                    role: VesselRole::Product {
                        output: id("reaction"),
                    },
                    positions: 2,
                },
            ],
            vec![
                PipettingStep::Distribute {
                    id: id("add-water"),
                    source: location("water-source", 0),
                    destinations: vec![location("reactions", 0), location("reactions", 1)],
                    volume_each: Volume::parse_microlitres("0.5").unwrap(),
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                },
                PipettingStep::Transfer {
                    id: id("add-buffer"),
                    source: location("water-source", 0),
                    destination: location("reactions", 0),
                    volume: Volume::parse_microlitres("2").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                },
                PipettingStep::Mix {
                    id: id("mix-reactions"),
                    targets: vec![location("reactions", 0), location("reactions", 1)],
                    cycles: 3,
                    volume: Volume::parse_microlitres("15").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                },
            ],
            PipettingConstraints {
                source_temperature: Some(TemperatureRange::exact(
                    Temperature::parse_degrees_celsius("4").unwrap(),
                )),
            },
        )
    }

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
    fn validation_rejects_dangling_and_out_of_range_locations() {
        let mut program = example();
        let PipettingStep::Distribute { destinations, .. } = &mut program.steps[0] else {
            unreachable!()
        };
        destinations[0] = location("missing", 0);
        assert!(matches!(
            program.validate(),
            Err(PipettingProgramValidationError::UnknownVessel { .. })
        ));

        let mut program = example();
        let PipettingStep::Distribute { destinations, .. } = &mut program.steps[0] else {
            unreachable!()
        };
        destinations[0] = location("reactions", 2);
        assert!(matches!(
            program.validate(),
            Err(PipettingProgramValidationError::PositionOutOfRange { .. })
        ));
    }

    #[test]
    fn validation_rejects_duplicate_ids_and_empty_operations() {
        let mut program = example();
        program.vessels.push(program.vessels[0].clone());
        assert!(matches!(
            program.validate(),
            Err(PipettingProgramValidationError::DuplicateVessel { .. })
        ));

        let mut program = example();
        program.steps.clear();
        assert_eq!(
            program.validate().unwrap_err(),
            PipettingProgramValidationError::NoSteps
        );
    }
}
