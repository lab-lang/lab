use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{
    CapabilityKind, ConstraintRelation, ExactDecimal, PropertyConstraint, PropertyKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vocabulary::{
    AIR_GAP_HANDLING, IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, MAXIMUM_AIR_GAP_VOLUME,
    MAXIMUM_MIX_VOLUME, MAXIMUM_TEMPERATURE, MAXIMUM_TRANSFER_VOLUME, METERED_LIQUID_TRANSFER,
    MINIMUM_TEMPERATURE, MINIMUM_TRANSFER_VOLUME, POST_DISPENSE_BLOWOUT,
    TEMPERATURE_CONTROLLED_STAGING, TOUCH_TIP, VESSEL_RELATIVE_LIQUID_ACCESS,
};
use crate::{
    BindingScope, CapabilityClause, CapabilityFormula, Length, ProcedureLocalId, TemperatureRange,
    Volume,
};

/// One material made available to a canonical pipetting program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialInput {
    pub id: ProcedureLocalId,
}

/// One material state produced by a canonical pipetting program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialOutput {
    pub id: ProcedureLocalId,
}

/// The semantic role of one logical vessel before any deck or well allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
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
    /// A physical vessel arriving through a task input and leaving as a new material state.
    InputOutput {
        input: u32,
        output: ProcedureLocalId,
    },
    /// A material substrate, such as selective agar, that becomes the task's product.
    MaterialProduct {
        material: ProcedureLocalId,
        output: ProcedureLocalId,
    },
    Intermediate,
}

/// A logical vessel with zero-based addressable positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vessel {
    pub id: ProcedureLocalId,
    pub role: VesselRole,
    pub positions: u32,
    /// Exact liquid volume initially present in every position when it is known to the Method.
    /// Material sources may omit this value so the adapter can calculate a sufficient source load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_volume_each: Option<Volume>,
    /// Largest liquid volume one position of this vessel can hold.
    ///
    /// Stated when the Method knows the vessel it requires. A dispense that would exceed it is a
    /// spill, which is cheaper to catch here than on the deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_capacity_each: Option<Volume>,
    /// Volume in one position the program must not draw below.
    ///
    /// This is the Method's own floor, such as leaving residual above a pellet, not the labware's
    /// unaspirable residual. An adapter knows the tube it will use and enforces its own dead volume
    /// on top of this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_volume_each: Option<Volume>,
    /// Temperature this vessel's contents must be held at while the program runs.
    ///
    /// This is per vessel rather than per program because one program routinely stages materials
    /// with different requirements: chemically competent cells must stay near 0 C while the
    /// recovery medium they are later given must not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<TemperatureRange>,
}

/// One logical position in a Procedure vessel.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub vessel: ProcedureLocalId,
    pub position: u32,
}

/// The strongest fluid-path reuse a realization may perform for one semantic operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum FluidPathPolicy {
    /// Each destination must use an isolated fluid path.
    IsolatedDestinations,
    /// Destinations may share one fluid path loaded from the same source, but that path must not
    /// re-enter the source after contacting a destination.
    SharedSourceNoReentry,
}

/// Where a realization must aspirate relative to the vessel and liquid state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum AspirationStrategy {
    /// Ordinary submerged aspiration; the implementation chooses a qualified default position.
    #[default]
    Liquid,
    /// Recompute the position from the planned current volume and validated vessel geometry.
    TrackedLiquidSurface,
    /// Aspirate at an exact offset above the vessel bottom.
    VesselBottom { offset: Length },
}

/// Where a realization must dispense relative to the vessel, liquid, or receiving material.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum DispenseStrategy {
    /// Ordinary in-liquid dispense; the implementation chooses a qualified default position.
    #[default]
    Liquid,
    /// Dispense above the receiving liquid without contacting it.
    AboveLiquid,
    /// Dispense at an exact offset above the vessel bottom.
    VesselBottom { offset: Length },
    /// Dispense at an exact signed offset from the vessel top.
    VesselTop { offset: Length },
    /// Dispense onto a non-liquid material surface such as selective agar.
    MaterialSurface,
}

/// Portable technique constraints on a transfer or distribution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferTechnique {
    #[serde(default)]
    pub aspiration: AspirationStrategy,
    #[serde(default)]
    pub dispense: DispenseStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub air_gap: Option<Volume>,
    #[serde(default)]
    pub blow_out: bool,
    #[serde(default)]
    pub touch_tip: bool,
}

/// Portable liquid-access constraints on an in-place mix.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MixTechnique {
    #[serde(default)]
    pub aspiration: AspirationStrategy,
    #[serde(default)]
    pub dispense: DispenseStrategy,
    #[serde(default)]
    pub blow_out: bool,
    #[serde(default)]
    pub touch_tip: bool,
}

/// One stable, observable liquid operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum PipettingStep {
    Transfer {
        id: ProcedureLocalId,
        source: Location,
        destination: Location,
        volume: Volume,
        fluid_path: FluidPathPolicy,
        /// Steps carrying the same group identity must use one continuous fluid path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fluid_path_group: Option<ProcedureLocalId>,
        #[serde(default)]
        technique: TransferTechnique,
    },
    Distribute {
        id: ProcedureLocalId,
        source: Location,
        destinations: Vec<Location>,
        volume_each: Volume,
        fluid_path: FluidPathPolicy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fluid_path_group: Option<ProcedureLocalId>,
        #[serde(default)]
        technique: TransferTechnique,
    },
    Mix {
        id: ProcedureLocalId,
        targets: Vec<Location>,
        cycles: u32,
        volume: Volume,
        fluid_path: FluidPathPolicy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fluid_path_group: Option<ProcedureLocalId>,
        #[serde(default)]
        technique: MixTechnique,
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
#[serde(deny_unknown_fields)]
pub struct PipettingConstraints {}

/// Version 1 of Lab's canonical, device-neutral pipetting contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
            match &vessel.role {
                VesselRole::MaterialSource { material }
                | VesselRole::MaterialProduct { material, .. }
                    if !material_ids.contains(material) =>
                {
                    return Err(PipettingProgramValidationError::UnknownMaterial {
                        vessel: vessel.id.clone(),
                        material: material.clone(),
                    });
                }
                VesselRole::Product { output }
                | VesselRole::InputOutput { output, .. }
                | VesselRole::MaterialProduct { output, .. }
                    if !output_ids.contains(output) =>
                {
                    return Err(PipettingProgramValidationError::UnknownOutput {
                        vessel: vessel.id.clone(),
                        output: output.clone(),
                    });
                }
                _ => {}
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
        let ledger = build_liquid_ledger(&self)?;
        Ok(ValidatedPipettingProgramV1 {
            program: self,
            ledger,
        })
    }
}

/// A pipetting program whose contract, graph, references, and operation bounds are valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPipettingProgramV1 {
    program: PipettingProgramV1,
    ledger: LiquidLedger,
}

impl ValidatedPipettingProgramV1 {
    pub fn as_program(&self) -> &PipettingProgramV1 {
        &self.program
    }

    /// The deterministic liquid effects proven while validating this program.
    pub fn liquid_ledger(&self) -> &LiquidLedger {
        &self.ledger
    }

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
        for step in &self.program.steps {
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
        if let Some(temperature) = staged_temperature_envelope(&self.program.vessels) {
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

impl AsRef<PipettingProgramV1> for ValidatedPipettingProgramV1 {
    fn as_ref(&self) -> &PipettingProgramV1 {
        self.as_program()
    }
}

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

/// One position, the volume a step moves, what it already holds, and the bound that is crossed.
///
/// Boxed because carrying these inline makes every `Result` in the crate pay for the widest error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeConflict {
    pub step: ProcedureLocalId,
    pub vessel: ProcedureLocalId,
    pub position: u32,
    /// Volume the step moves into or out of the position.
    pub moved: String,
    /// Volume already present before the step.
    pub present: String,
    /// The dead volume or working capacity the step crosses.
    pub limit: String,
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
    #[error(
        "pipetting step `{step}` withdraws {required} uL from `{vessel}` position {position}, which contains only {available} uL"
    )]
    InsufficientVolume {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
        required: String,
        available: String,
    },
    #[error(
        "pipetting step `{}` withdraws {} uL from `{}` position {}, leaving less than its {} uL dead volume from {} uL",
        .0.step, .0.moved, .0.vessel, .0.position, .0.limit, .0.present
    )]
    BelowDeadVolume(Box<VolumeConflict>),
    #[error(
        "pipetting step `{}` dispenses {} uL into `{}` position {}, taking it past its {} uL working capacity from {} uL",
        .0.step, .0.moved, .0.vessel, .0.position, .0.limit, .0.present
    )]
    ExceedsWorkingCapacity(Box<VolumeConflict>),
    #[error(
        "pipetting step `{step}` aspirates from `{vessel}`, which states no initial volume; only a material source may leave its fill to the adapter"
    )]
    UnvaluedSourceAspiration {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
    },
    #[error(
        "pipetting step `{step}` tracks the liquid surface of `{vessel}`, which states no initial volume, so the planned surface cannot be computed"
    )]
    UntrackableSource {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
    },
    #[error(
        "pipetting mix `{step}` requires {required} uL in `{vessel}` position {position}, which contains only {available} uL"
    )]
    InsufficientMixVolume {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
        required: String,
        available: String,
    },
}

fn build_liquid_ledger(
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
    use crate::{Length, Temperature, vocabulary};

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
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: Some(TemperatureRange::exact(
                        Temperature::parse_degrees_celsius("4").unwrap(),
                    )),
                },
                Vessel {
                    id: id("reactions"),
                    role: VesselRole::Product {
                        output: id("reaction"),
                    },
                    positions: 2,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
            ],
            vec![
                PipettingStep::Distribute {
                    id: id("add-water"),
                    source: location("water-source", 0),
                    destinations: vec![location("reactions", 0), location("reactions", 1)],
                    volume_each: Volume::parse_microlitres("0.5").unwrap(),
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                    fluid_path_group: None,
                    technique: TransferTechnique::default(),
                },
                PipettingStep::Transfer {
                    id: id("add-buffer"),
                    source: location("water-source", 0),
                    destination: location("reactions", 0),
                    volume: Volume::parse_microlitres("2").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: TransferTechnique::default(),
                },
                PipettingStep::Mix {
                    id: id("mix-reactions"),
                    targets: vec![location("reactions", 0), location("reactions", 1)],
                    cycles: 3,
                    volume: Volume::parse_microlitres("0.5").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: MixTechnique::default(),
                },
            ],
            PipettingConstraints::default(),
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
    fn a_canonical_vessel_rejects_keys_it_does_not_know() {
        // A writer still emitting a field this contract has moved or removed must be told, not
        // quietly trimmed. `source_temperature` used to live on the program's constraints.
        let stale_constraints = r#"{"source_temperature": {"minimum": 4, "maximum": 4}}"#;
        assert!(serde_json::from_str::<PipettingConstraints>(stale_constraints).is_err());

        let stale_vessel = r#"{
            "id": "cells",
            "role": {"kind": "intermediate"},
            "positions": 1,
            "initial_volume": {"value": {"type": "real", "value": "50"}, "unit": "http://qudt.org/vocab/unit/MicroL"}
        }"#;
        let error = serde_json::from_str::<Vessel>(stale_vessel).unwrap_err();
        assert!(
            error.to_string().contains("initial_volume"),
            "the unknown key is named: {error}"
        );
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
}
