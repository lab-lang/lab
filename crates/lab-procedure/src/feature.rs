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

use crate::pipetting::{
    AspirationStrategy, DispenseStrategy, FluidPathPolicy, MixTechnique, PipettingProgramV1,
    PipettingStep, TransferTechnique,
};
use crate::thermal::ThermalProgramV1;

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

fn aspiration_feature(strategy: &AspirationStrategy) -> ProgramFeature {
    match strategy {
        AspirationStrategy::Liquid => ProgramFeature::AspirateLiquid,
        AspirationStrategy::TrackedLiquidSurface => ProgramFeature::AspirateTrackedSurface,
        AspirationStrategy::VesselBottom { .. } => ProgramFeature::AspirateVesselBottom,
    }
}

fn dispense_feature(strategy: &DispenseStrategy) -> ProgramFeature {
    match strategy {
        DispenseStrategy::Liquid => ProgramFeature::DispenseLiquid,
        DispenseStrategy::AboveLiquid => ProgramFeature::DispenseAboveLiquid,
        DispenseStrategy::VesselBottom { .. } => ProgramFeature::DispenseVesselBottom,
        DispenseStrategy::VesselTop { .. } => ProgramFeature::DispenseVesselTop,
        DispenseStrategy::MaterialSurface => ProgramFeature::DispenseMaterialSurface,
    }
}

fn fluid_path_feature(policy: &FluidPathPolicy) -> ProgramFeature {
    match policy {
        FluidPathPolicy::IsolatedDestinations => ProgramFeature::FluidPathIsolatedDestinations,
        FluidPathPolicy::SharedSourceNoReentry => ProgramFeature::FluidPathSharedSourceNoReentry,
    }
}

fn transfer_features(technique: &TransferTechnique, features: &mut BTreeSet<ProgramFeature>) {
    let TransferTechnique {
        aspiration,
        dispense,
        air_gap,
        blow_out,
        touch_tip,
    } = technique;
    features.insert(aspiration_feature(aspiration));
    features.insert(dispense_feature(dispense));
    if air_gap.is_some() {
        features.insert(ProgramFeature::AirGap);
    }
    if *blow_out {
        features.insert(ProgramFeature::PostDispenseBlowout);
    }
    if *touch_tip {
        features.insert(ProgramFeature::TouchTip);
    }
}

fn mix_features(technique: &MixTechnique, features: &mut BTreeSet<ProgramFeature>) {
    let MixTechnique {
        aspiration,
        dispense,
        blow_out,
        touch_tip,
    } = technique;
    features.insert(aspiration_feature(aspiration));
    features.insert(dispense_feature(dispense));
    if *blow_out {
        features.insert(ProgramFeature::PostDispenseBlowout);
    }
    if *touch_tip {
        features.insert(ProgramFeature::TouchTip);
    }
}

/// Every feature a pipetting program requires of its implementation.
pub fn pipetting_features(program: &PipettingProgramV1) -> BTreeSet<ProgramFeature> {
    let mut features = BTreeSet::new();
    if program.vessels.iter().any(|vessel| vessel.positions > 1) {
        features.insert(ProgramFeature::MultiPositionVessel);
    }
    if program
        .vessels
        .iter()
        .any(|vessel| vessel.temperature.is_some())
    {
        features.insert(ProgramFeature::VesselTemperatureControl);
    }
    for step in &program.steps {
        match step {
            PipettingStep::Transfer {
                fluid_path,
                fluid_path_group,
                technique,
                ..
            } => {
                features.insert(ProgramFeature::Transfer);
                features.insert(fluid_path_feature(fluid_path));
                if fluid_path_group.is_some() {
                    features.insert(ProgramFeature::FluidPathGroup);
                }
                transfer_features(technique, &mut features);
            }
            PipettingStep::Distribute {
                fluid_path,
                fluid_path_group,
                technique,
                ..
            } => {
                features.insert(ProgramFeature::Distribute);
                features.insert(fluid_path_feature(fluid_path));
                if fluid_path_group.is_some() {
                    features.insert(ProgramFeature::FluidPathGroup);
                }
                transfer_features(technique, &mut features);
            }
            PipettingStep::Mix {
                fluid_path,
                fluid_path_group,
                technique,
                ..
            } => {
                features.insert(ProgramFeature::Mix);
                features.insert(fluid_path_feature(fluid_path));
                if fluid_path_group.is_some() {
                    features.insert(ProgramFeature::FluidPathGroup);
                }
                mix_features(technique, &mut features);
            }
            PipettingStep::Barrier { .. } => {
                features.insert(ProgramFeature::Barrier);
            }
        }
    }
    features
}

/// Every feature a thermal program requires of its implementation.
pub fn thermal_features(program: &ThermalProgramV1) -> BTreeSet<ProgramFeature> {
    let mut features = BTreeSet::new();
    if program.load.sample_count > 1 {
        features.insert(ProgramFeature::ThermalMultiSample);
    }
    if program.lid_temperature.is_some() {
        features.insert(ProgramFeature::ThermalHeatedLid);
    }
    if program.final_hold.is_some() {
        features.insert(ProgramFeature::ThermalFinalHold);
    }
    for stage in &program.stages {
        if stage.repeats > 1 {
            features.insert(ProgramFeature::ThermalStageRepeat);
        }
        for step in &stage.steps {
            if step.ramp_rate.is_some() {
                features.insert(ProgramFeature::ThermalControlledRamp);
            }
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use super::{ProgramFeature, pipetting_features, thermal_features};
    use crate::pipetting::{
        AspirationStrategy, DispenseStrategy, FluidPathPolicy, Location, MaterialOutput,
        PipettingConstraints, PipettingProgramV1, PipettingStep, TransferTechnique, Vessel,
        VesselRole,
    };
    use crate::quantity::{Duration, Temperature, TemperatureRange, Volume};
    use crate::thermal::{ThermalLoad, ThermalProgramV1, ThermalStage, ThermalStep};
    use crate::{ProcedureLocalId, quantity::TemperatureRampRate};

    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn at(vessel: &str, position: u32) -> Location {
        Location {
            vessel: id(vessel),
            position,
        }
    }

    #[test]
    fn a_replicated_vessel_demands_multi_position_support() {
        let program = PipettingProgramV1::new(
            Vec::new(),
            vec![MaterialOutput { id: id("product") }],
            vec![Vessel {
                id: id("plate"),
                role: VesselRole::Product {
                    output: id("product"),
                },
                positions: 2,
                working_capacity_each: None,
                dead_volume_each: None,
                initial_volume_each: None,
                temperature: None,
            }],
            vec![PipettingStep::Mix {
                id: id("mix"),
                targets: vec![at("plate", 0)],
                cycles: 1,
                volume: Volume::parse_microlitres("5").unwrap(),
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: Default::default(),
            }],
            PipettingConstraints::default(),
        );
        let features = pipetting_features(&program);
        assert!(features.contains(&ProgramFeature::MultiPositionVessel));
        assert!(features.contains(&ProgramFeature::Mix));
        assert!(!features.contains(&ProgramFeature::Transfer));
    }

    #[test]
    fn every_stated_technique_becomes_a_feature() {
        let program = PipettingProgramV1::new(
            Vec::new(),
            vec![MaterialOutput { id: id("product") }],
            vec![
                Vessel {
                    id: id("source"),
                    role: VesselRole::Intermediate,
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: Some(TemperatureRange::exact(
                        Temperature::parse_degrees_celsius("4").unwrap(),
                    )),
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
            vec![PipettingStep::Transfer {
                id: id("spot"),
                source: at("source", 0),
                destination: at("plate", 0),
                volume: Volume::parse_microlitres("4").unwrap(),
                fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                fluid_path_group: Some(id("series")),
                technique: TransferTechnique {
                    aspiration: AspirationStrategy::TrackedLiquidSurface,
                    dispense: DispenseStrategy::MaterialSurface,
                    air_gap: Some(Volume::parse_microlitres("10").unwrap()),
                    blow_out: true,
                    touch_tip: true,
                },
            }],
            PipettingConstraints::default(),
        );
        let features = pipetting_features(&program);
        for expected in [
            ProgramFeature::Transfer,
            ProgramFeature::AspirateTrackedSurface,
            ProgramFeature::DispenseMaterialSurface,
            ProgramFeature::AirGap,
            ProgramFeature::PostDispenseBlowout,
            ProgramFeature::TouchTip,
            ProgramFeature::FluidPathSharedSourceNoReentry,
            ProgramFeature::FluidPathGroup,
            ProgramFeature::VesselTemperatureControl,
        ] {
            assert!(features.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn thermal_controls_become_features_only_when_present() {
        let plain = ThermalProgramV1 {
            load: ThermalLoad {
                input: 0,
                outputs: vec![id("product")],
                sample_count: 1,
                volume_each: Volume::parse_microlitres("20").unwrap(),
            },
            lid_temperature: None,
            stages: vec![ThermalStage {
                id: id("hold"),
                repeats: 1,
                steps: vec![ThermalStep {
                    id: id("step"),
                    temperature: Temperature::parse_degrees_celsius("37").unwrap(),
                    hold: Duration::parse_seconds("60").unwrap(),
                    ramp_rate: None,
                }],
            }],
            final_hold: None,
        };
        assert!(thermal_features(&plain).is_empty());

        let mut rich = plain.clone();
        rich.load.sample_count = 8;
        rich.lid_temperature = Some(Temperature::parse_degrees_celsius("105").unwrap());
        rich.final_hold = Some(Temperature::parse_degrees_celsius("4").unwrap());
        rich.stages[0].repeats = 30;
        rich.stages[0].steps[0].ramp_rate =
            Some(TemperatureRampRate::parse_degrees_celsius_per_second("2").unwrap());
        let features = thermal_features(&rich);
        for expected in [
            ProgramFeature::ThermalMultiSample,
            ProgramFeature::ThermalHeatedLid,
            ProgramFeature::ThermalFinalHold,
            ProgramFeature::ThermalStageRepeat,
            ProgramFeature::ThermalControlledRamp,
        ] {
            assert!(features.contains(&expected), "missing {expected}");
        }
    }
}
