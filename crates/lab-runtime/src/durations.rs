//! The time model a simulation runs on.
//!
//! Thermal durations are computed exactly from the profile a document
//! carries: ramps at the stated or device-maximum rate, plus holds, across
//! every repeat. Everything else is an estimate this model states as data:
//! per-frame costs for STAR commands and human times for handoffs and
//! manual steps. Estimates are serializable so measured run ledgers can
//! calibrate them later; nothing here pretends to more precision than it
//! has.

use std::collections::BTreeMap;

use lab_instruments::{ThermalLimits, ThermalProfile};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DurationModel {
    /// The model's name, recorded in every trace it produces.
    pub name: String,
    /// Block temperature assumed before the first thermal program.
    pub ambient_celsius: f64,
    /// A human carries labware between stations and confirms.
    pub handoff_seconds: f64,
    /// A human performs a non-movement step and confirms.
    pub manual_seconds: f64,
    /// A motorized door opens or closes.
    pub door_seconds: f64,
    /// A STAR frame whose command has no table entry.
    pub star_frame_default_seconds: f64,
    /// Per-command STAR frame costs, keyed by module and code, e.g. `C0TP`.
    pub star_frame_seconds: BTreeMap<String, f64>,
}

impl Default for DurationModel {
    fn default() -> Self {
        // Coarse, deliberately conservative estimates. A real bench's run
        // ledger is the calibration source; these are starting points.
        let star_frame_seconds = BTreeMap::from(
            [
                ("C0TT", 0.5),  // define a tip type: bookkeeping only
                ("C0TP", 8.0),  // pick up tips
                ("C0TR", 6.0),  // discard tips
                ("C0AS", 12.0), // aspirate, with liquid seek
                ("C0DS", 10.0), // dispense
                ("C0ZA", 3.0),  // retract to Z-safety
            ]
            .map(|(code, seconds)| (code.to_string(), seconds)),
        );
        Self {
            name: "default-v0".to_string(),
            ambient_celsius: 25.0,
            handoff_seconds: 90.0,
            manual_seconds: 180.0,
            door_seconds: 5.0,
            star_frame_default_seconds: 5.0,
            star_frame_seconds,
        }
    }
}

impl DurationModel {
    /// The cost of one STAR frame, from the table or the stated default.
    pub fn star_frame_seconds(&self, module: &str, code: &str) -> f64 {
        let key = format!("{module}{code}");
        self.star_frame_seconds
            .get(&key)
            .copied()
            .unwrap_or(self.star_frame_default_seconds)
    }

    /// The exact duration of a thermal profile from a starting block
    /// temperature, and the block temperature it ends at. Ramps run at the
    /// step's stated rate or the device maximum; holds are as written;
    /// repeats carry the block temperature across iterations.
    pub fn thermal_profile_seconds(
        &self,
        profile: &ThermalProfile,
        limits: &ThermalLimits,
        starting_celsius: f64,
    ) -> (f64, f64) {
        let mut seconds = 0.0;
        let mut block = starting_celsius;
        for stage in &profile.stages {
            for _ in 0..stage.repeats {
                for step in &stage.steps {
                    let rate = step
                        .ramp_c_per_s
                        .filter(|rate| *rate > 0.0)
                        .unwrap_or(limits.ramp_max_c_per_s);
                    seconds += (step.celsius - block).abs() / rate;
                    seconds += step.hold_seconds;
                    block = step.celsius;
                }
            }
        }
        (seconds, block)
    }

    /// The duration of a hold command: one ramp to the target.
    pub fn thermal_ramp_seconds(
        &self,
        limits: &ThermalLimits,
        from_celsius: f64,
        to_celsius: f64,
    ) -> f64 {
        (to_celsius - from_celsius).abs() / limits.ramp_max_c_per_s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_instruments::{ThermalStage, ThermalStep};

    fn odtc_like_limits() -> ThermalLimits {
        ThermalLimits {
            block_min_celsius: 4.0,
            block_max_celsius: 99.0,
            lid_min_celsius: 30.0,
            lid_max_celsius: 115.0,
            ramp_max_c_per_s: 4.4,
            per_step_lid: true,
        }
    }

    fn step(celsius: f64, hold_seconds: f64) -> ThermalStep {
        ThermalStep {
            celsius,
            hold_seconds,
            ramp_c_per_s: None,
            lid_celsius: None,
        }
    }

    #[test]
    fn a_golden_gate_profile_computes_exactly() {
        // 30 cycles of 37 °C / 90 s + 16 °C / 180 s, then 60 °C / 300 s,
        // from 25 °C ambient at the 4.4 °C/s device maximum.
        let profile = ThermalProfile {
            stages: vec![
                ThermalStage {
                    steps: vec![step(37.0, 90.0), step(16.0, 180.0)],
                    repeats: 30,
                },
                ThermalStage {
                    steps: vec![step(60.0, 300.0)],
                    repeats: 1,
                },
            ],
        };
        let model = DurationModel::default();
        let (seconds, final_celsius) =
            model.thermal_profile_seconds(&profile, &odtc_like_limits(), 25.0);

        // Holds: 30 * (90 + 180) + 300 = 8400 s.
        // Ramps: 25→37 once (12/4.4), then 37↔16 across the cycles: 21 °C
        // each way, 59 crossings (30 down, 29 back up), then 16→60 (44).
        let ramp = (12.0 + 59.0 * 21.0 + 44.0) / 4.4;
        assert!(
            (seconds - (8400.0 + ramp)).abs() < 1e-9,
            "expected {} got {seconds}",
            8400.0 + ramp
        );
        assert_eq!(final_celsius, 60.0, "the block ends at the last plateau");
    }

    #[test]
    fn frame_costs_come_from_the_table_with_a_stated_default() {
        let model = DurationModel::default();
        assert_eq!(model.star_frame_seconds("C0", "TP"), 8.0);
        assert_eq!(
            model.star_frame_seconds("C0", "XX"),
            model.star_frame_default_seconds
        );
    }
}
