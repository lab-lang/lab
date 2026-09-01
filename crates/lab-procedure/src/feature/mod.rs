//! Fine-grained observable properties of a canonical Procedure program.
//!
//! A [`CapabilityFormula`](crate::CapabilityFormula) answers "which Asset can do this work" and is
//! deliberately coarse, because a facility's offering vocabulary describes classes of equipment. A
//! feature set answers a different question: "can this exact implementation realize this exact
//! program". An implementation declares the features it realizes, and a program carrying a feature
//! its implementation does not declare is rejected before any device document is emitted.
//!
//! Derivation matches every canonical variant and every technique field by name, so extending a
//! contract fails to compile until its features are stated here. That is the point: a semantic
//! value that no one maps is a value an adapter would otherwise silently drop.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{PipettingProgramV1, ThermalProgramV1};

/// One observable property an implementation must realize to run a program faithfully.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProgramFeature {
    /// A logical vessel addresses more than one position, so the realization must lay out and
    /// track every replicate rather than collapsing them onto one.
    MultiPositionVessel,
    Transfer,
    Distribute,
    Mix,
    Barrier,
    AspirateLiquid,
    AspirateTrackedSurface,
    AspirateVesselBottom,
    DispenseLiquid,
    DispenseAboveLiquid,
    DispenseVesselBottom,
    DispenseVesselTop,
    DispenseMaterialSurface,
    AirGap,
    PostDispenseBlowout,
    TouchTip,
    FluidPathIsolatedDestinations,
    FluidPathSharedSourceNoReentry,
    /// Ordered steps that must share one continuous fluid path.
    FluidPathGroup,
    /// A vessel whose staging temperature the program constrains.
    VesselTemperatureControl,
    /// A vessel stating a working capacity or a volume the program must not draw below.
    VesselVolumeLimits,
    ThermalStageRepeat,
    ThermalControlledRamp,
    ThermalHeatedLid,
    ThermalFinalHold,
    ThermalMultiSample,
}

impl ProgramFeature {
    /// A stable human-readable name for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MultiPositionVessel => "multi_position_vessel",
            Self::Transfer => "transfer",
            Self::Distribute => "distribute",
            Self::Mix => "mix",
            Self::Barrier => "barrier",
            Self::AspirateLiquid => "aspirate_liquid",
            Self::AspirateTrackedSurface => "aspirate_tracked_surface",
            Self::AspirateVesselBottom => "aspirate_vessel_bottom",
            Self::DispenseLiquid => "dispense_liquid",
            Self::DispenseAboveLiquid => "dispense_above_liquid",
            Self::DispenseVesselBottom => "dispense_vessel_bottom",
            Self::DispenseVesselTop => "dispense_vessel_top",
            Self::DispenseMaterialSurface => "dispense_material_surface",
            Self::AirGap => "air_gap",
            Self::PostDispenseBlowout => "post_dispense_blowout",
            Self::TouchTip => "touch_tip",
            Self::FluidPathIsolatedDestinations => "fluid_path_isolated_destinations",
            Self::FluidPathSharedSourceNoReentry => "fluid_path_shared_source_no_reentry",
            Self::FluidPathGroup => "fluid_path_group",
            Self::VesselTemperatureControl => "vessel_temperature_control",
            Self::VesselVolumeLimits => "vessel_volume_limits",
            Self::ThermalStageRepeat => "thermal_stage_repeat",
            Self::ThermalControlledRamp => "thermal_controlled_ramp",
            Self::ThermalHeatedLid => "thermal_heated_lid",
            Self::ThermalFinalHold => "thermal_final_hold",
            Self::ThermalMultiSample => "thermal_multi_sample",
        }
    }
}

impl std::fmt::Display for ProgramFeature {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Every feature a pipetting program requires of its implementation.
pub fn pipetting_features(program: &PipettingProgramV1) -> BTreeSet<ProgramFeature> {
    crate::pipetting::required_features(program)
}

/// Every feature a thermal program requires of its implementation.
pub fn thermal_features(program: &ThermalProgramV1) -> BTreeSet<ProgramFeature> {
    crate::thermal::required_features(program)
}
