use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::vessel::Location;
use crate::procedure::{Length, ProcedureLocalId, Volume};

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
