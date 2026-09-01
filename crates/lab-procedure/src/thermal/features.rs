use std::collections::BTreeSet;

use super::{ThermalLoad, ThermalProgramV1, ThermalStage, ThermalStep};
use crate::feature::ProgramFeature;

/// Every feature a thermal program requires of its implementation.
pub(crate) fn required_features(program: &ThermalProgramV1) -> BTreeSet<ProgramFeature> {
    let ThermalProgramV1 {
        load,
        lid_temperature,
        stages,
        final_hold,
    } = program;
    let ThermalLoad {
        input: _,
        outputs: _,
        sample_count,
        volume_each: _,
    } = load;
    let mut features = BTreeSet::new();
    if *sample_count > 1 {
        features.insert(ProgramFeature::ThermalMultiSample);
    }
    if lid_temperature.is_some() {
        features.insert(ProgramFeature::ThermalHeatedLid);
    }
    if final_hold.is_some() {
        features.insert(ProgramFeature::ThermalFinalHold);
    }
    for stage in stages {
        let ThermalStage {
            id: _,
            repeats,
            steps,
        } = stage;
        if *repeats > 1 {
            features.insert(ProgramFeature::ThermalStageRepeat);
        }
        for step in steps {
            let ThermalStep {
                id: _,
                temperature: _,
                hold: _,
                ramp_rate,
            } = step;
            if ramp_rate.is_some() {
                features.insert(ProgramFeature::ThermalControlledRamp);
            }
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::thermal_features;
    use crate::{Duration, ProcedureLocalId, Temperature, TemperatureRampRate, Volume};

    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
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
