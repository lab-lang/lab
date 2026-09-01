use std::collections::BTreeSet;

use super::operation::{
    AspirationStrategy, DispenseStrategy, FluidPathPolicy, MixTechnique, PipettingStep,
    TransferTechnique,
};
use super::program::PipettingProgramV1;
use super::vessel::Vessel;
use crate::feature::ProgramFeature;

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
pub(crate) fn required_features(program: &PipettingProgramV1) -> BTreeSet<ProgramFeature> {
    let mut features = BTreeSet::new();
    for vessel in &program.vessels {
        let Vessel {
            id: _,
            role: _,
            positions,
            initial_volume_each: _,
            working_capacity_each,
            dead_volume_each,
            temperature,
        } = vessel;
        if *positions > 1 {
            features.insert(ProgramFeature::MultiPositionVessel);
        }
        if temperature.is_some() {
            features.insert(ProgramFeature::VesselTemperatureControl);
        }
        if working_capacity_each.is_some() || dead_volume_each.is_some() {
            features.insert(ProgramFeature::VesselVolumeLimits);
        }
    }
    for step in &program.steps {
        match step {
            PipettingStep::Transfer {
                id: _,
                source: _,
                destination: _,
                volume: _,
                fluid_path,
                fluid_path_group,
                technique,
            } => {
                features.insert(ProgramFeature::Transfer);
                features.insert(fluid_path_feature(fluid_path));
                if fluid_path_group.is_some() {
                    features.insert(ProgramFeature::FluidPathGroup);
                }
                transfer_features(technique, &mut features);
            }
            PipettingStep::Distribute {
                id: _,
                source: _,
                destinations: _,
                volume_each: _,
                fluid_path,
                fluid_path_group,
                technique,
            } => {
                features.insert(ProgramFeature::Distribute);
                features.insert(fluid_path_feature(fluid_path));
                if fluid_path_group.is_some() {
                    features.insert(ProgramFeature::FluidPathGroup);
                }
                transfer_features(technique, &mut features);
            }
            PipettingStep::Mix {
                id: _,
                targets: _,
                cycles: _,
                volume: _,
                fluid_path,
                fluid_path_group,
                technique,
            } => {
                features.insert(ProgramFeature::Mix);
                features.insert(fluid_path_feature(fluid_path));
                if fluid_path_group.is_some() {
                    features.insert(ProgramFeature::FluidPathGroup);
                }
                mix_features(technique, &mut features);
            }
            PipettingStep::Barrier { id: _, reason: _ } => {
                features.insert(ProgramFeature::Barrier);
            }
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::pipetting_features;
    use crate::pipetting::{Location, MaterialOutput, PipettingConstraints, VesselRole};
    use crate::{ProcedureLocalId, Temperature, TemperatureRange, Volume};

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
}
