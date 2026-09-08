//! Operational configuration for the Opentrons OT-2 adapter.
//!
//! Facility allocation has already selected an exact Asset before this profile is read. The profile contains only checked configuration the implementation still needs to produce an executable protocol. It cannot select a facility Asset or another adapter.
//!
//! Every field has a default, so a profile states only what differs from the reference implementation configuration. Unknown keys are rejected, because a misspelled slot silently falling back to a default is how a protocol ends up aspirating from the wrong place.

mod defaults;
mod error;
mod schema;

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use error::Ot2ProfileError;
// These types occur in public profile fields, so they remain nameable without exposing the
// private source-module layout used to implement the schema.
pub use crate::backend::resources::{
    PlateCapacity, UnknownPlateGeometry, supported_plate_capacities,
};
pub use schema::{
    DeckLabware, Instruments, Ot2Resources, Pipette, ProtocolOptions, TechniqueCalibration,
    TemperatureModule, Thermocycler, TipRacks,
};

/// Deck slots an OT-2 can address. Slot 12 is the fixed trash.
const ADDRESSABLE_SLOTS: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"];
/// Slots the Thermocycler Module GEN2 occupies when installed.
const THERMOCYCLER_SLOTS: [&str; 4] = ["7", "8", "10", "11"];

/// The complete OT-2 implementation configuration consumed by planning and emission.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ot2AdapterProfile {
    /// File-stem label supplied by the exact Asset binding. It is review metadata, not profile input.
    #[serde(skip)]
    #[schemars(skip)]
    pub name: String,
    #[serde(default)]
    pub protocol: ProtocolOptions,
    #[serde(default)]
    pub instruments: Instruments,
    #[serde(default)]
    pub techniques: TechniqueCalibration,
    #[serde(default)]
    pub resources: Ot2Resources,
}

impl Default for Ot2AdapterProfile {
    fn default() -> Self {
        Self {
            name: "opentrons.ot2".to_owned(),
            protocol: ProtocolOptions::default(),
            instruments: Instruments::default(),
            techniques: TechniqueCalibration::default(),
            resources: Ot2Resources::default(),
        }
    }
}

impl Ot2AdapterProfile {
    /// Load operational configuration for one exact Asset binding.
    pub fn parse(name: &str, text: &str) -> Result<Self, Ot2ProfileError> {
        let mut profile: Self = toml::from_str(text)?;
        profile.name = name.to_owned();
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), Ot2ProfileError> {
        self.validate_techniques()?;
        let mut seen: Vec<(String, String)> = Vec::new();
        for (context, slots) in self.resource_claims() {
            if slots.is_empty() {
                return Err(Ot2ProfileError::NoSlots { context });
            }
            for slot in slots {
                if !ADDRESSABLE_SLOTS.contains(&slot.as_str()) {
                    return Err(Ot2ProfileError::UnknownSlot { context, slot });
                }
                if THERMOCYCLER_SLOTS.contains(&slot.as_str()) {
                    return Err(Ot2ProfileError::ThermocyclerSlot { context, slot });
                }
                if let Some((first, _)) = seen.iter().find(|(_, taken)| taken == &slot) {
                    return Err(Ot2ProfileError::SlotConflict {
                        slot,
                        first: first.clone(),
                        second: context,
                    });
                }
                seen.push((context.clone(), slot));
            }
        }
        Ok(())
    }

    fn validate_techniques(&self) -> Result<(), Ot2ProfileError> {
        let calibration = &self.techniques;
        for (parameter, value) in [
            ("aspiration_rate", calibration.aspiration_rate),
            ("mix_aspiration_rate", calibration.mix_aspiration_rate),
            (
                "distribution_aspiration_rate",
                calibration.distribution_aspiration_rate,
            ),
            ("dispense_rate", calibration.dispense_rate),
            (
                "tracked_meniscus_offset_mm",
                calibration.tracked_meniscus_offset_mm,
            ),
            (
                "tracked_usable_depth_offset_mm",
                calibration.tracked_usable_depth_offset_mm,
            ),
            (
                "tracked_minimum_height_mm",
                calibration.tracked_minimum_height_mm,
            ),
            ("above_liquid_offset_mm", calibration.above_liquid_offset_mm),
            ("touch_tip_speed_mm_s", calibration.touch_tip_speed_mm_s),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(Ot2ProfileError::InvalidTechnique {
                    parameter,
                    message: "must be finite and greater than zero",
                });
            }
        }
        for (parameter, value) in [
            (
                "material_surface_offset_mm",
                calibration.material_surface_offset_mm,
            ),
            (
                "touch_tip_vertical_offset_mm",
                calibration.touch_tip_vertical_offset_mm,
            ),
        ] {
            if !value.is_finite() {
                return Err(Ot2ProfileError::InvalidTechnique {
                    parameter,
                    message: "must be finite",
                });
            }
        }
        for (parameter, value) in [("touch_tip_radius", calibration.touch_tip_radius)] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(Ot2ProfileError::InvalidTechnique {
                    parameter,
                    message: "must be finite and between zero and one",
                });
            }
        }
        if calibration.tracked_source_volume_ul == 0 {
            return Err(Ot2ProfileError::InvalidTechnique {
                parameter: "tracked_source_volume_ul",
                message: "must be greater than zero",
            });
        }
        Ok(())
    }

    /// Every addressable resource is present for the complete canonical program.
    fn resource_claims(&self) -> Vec<(String, Vec<String>)> {
        vec![
            (
                "bulk liquids".to_owned(),
                vec![self.resources.bulk.slot.clone()],
            ),
            (
                "material surfaces".to_owned(),
                vec![self.resources.surface.slot.clone()],
            ),
            (
                "the source module".to_owned(),
                vec![self.resources.sources.slot.clone()],
            ),
            (
                "small tips".to_owned(),
                self.resources.small_tips.slots.clone(),
            ),
            (
                "large tips".to_owned(),
                self.resources.large_tips.slots.clone(),
            ),
        ]
    }

    /// Labware load names this profile references, for reporting what an
    /// operator must have on hand.
    pub fn labware(&self) -> BTreeSet<String> {
        BTreeSet::from([
            self.resources.sources.labware.clone(),
            self.resources.work.labware.clone(),
            self.resources.bulk.labware.clone(),
            self.resources.surface.labware.clone(),
            self.resources.small_tips.labware.clone(),
            self.resources.large_tips.labware.clone(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::opentrons::ot2::profile::*;

    #[test]
    fn an_empty_profile_describes_the_reference_bench() {
        let profile = Ot2AdapterProfile::parse("reference-bench", "").unwrap();
        assert_eq!(profile.name, "reference-bench");
        assert_eq!(profile.protocol.api_level, "2.21");
        assert_eq!(profile.resources.sources.slot, "1");
        assert_eq!(profile.resources.work.capacity.get(), 96);
        assert_eq!(profile.resources.small_tips.slots, ["2"]);
        assert_eq!(profile.resources.large_tips.slots, ["6"]);
        assert_eq!(profile.techniques.touch_tip_vertical_offset_mm, -14.0);
    }

    #[test]
    fn physical_resources_cannot_share_a_slot() {
        let error = Ot2AdapterProfile::parse(
            "bench-three",
            r#"
[resources.sources]
model = "temperature module gen2"
slot = "2"
labware = "opentrons_24_aluminumblock_nest_1.5ml_snapcap"
capacity = 24
"#,
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("deck slot '2'")
                && message.contains("the source module")
                && message.contains("small tips"),
            "all resources are present for the canonical program: {message}"
        );
    }

    #[test]
    fn a_profile_overrides_only_what_it_states() {
        let profile = Ot2AdapterProfile::parse(
            "bench-two",
            r#"
[resources.large_tips]
labware = "opentrons_96_filtertiprack_200ul"
slots = ["5"]
capacity = 96
"#,
        )
        .unwrap();
        assert_eq!(profile.resources.large_tips.slots, ["5"]);
        assert_eq!(
            profile.resources.small_tips.slots,
            ["2"],
            "an unstated resource keeps the reference layout"
        );
    }

    #[test]
    fn the_loader_supplies_the_profile_name_and_protocol_options_are_explicit() {
        let profile = Ot2AdapterProfile::parse("ot2-runtime", "[protocol]\napi_level = \"2.20\"\n")
            .expect("the exact Asset binding supplies the profile label");
        assert_eq!(profile.name, "ot2-runtime");
        assert_eq!(profile.protocol.api_level, "2.20");
    }

    #[test]
    fn rejects_an_embedded_target_or_adapter_selector() {
        let error =
            Ot2AdapterProfile::parse("ot2-runtime", "[target]\nbackend = \"opentrons.flex\"\n")
                .expect_err("only the exact Asset binding may select an adapter");
        assert!(error.to_string().contains("target"), "{error}");
    }

    #[test]
    fn rejects_labware_placed_under_the_thermocycler() {
        let error = Ot2AdapterProfile::parse(
            "bench-two",
            r#"
[resources.small_tips]
labware = "opentrons_96_tiprack_20ul"
slots = ["7"]
capacity = 96
"#,
        )
        .expect_err("slot 7 is occupied by the thermocycler");
        assert!(error.to_string().contains("thermocycler"), "{error}");
    }

    #[test]
    fn rejects_two_resources_in_one_slot() {
        let error = Ot2AdapterProfile::parse(
            "bench-two",
            r#"
[resources.large_tips]
labware = "opentrons_96_filtertiprack_200ul"
slots = ["2"]
capacity = 96
"#,
        )
        .expect_err("slot 2 already holds the small tips");
        assert!(error.to_string().contains("claimed by both"), "{error}");
    }

    #[test]
    fn rejects_an_unknown_key_rather_than_silently_ignoring_it() {
        let error = Ot2AdapterProfile::parse("bench-two", "[stages.assembly]\nsmall_tips = 2\n")
            .expect_err("the removed scientific stage schema must not be accepted");
        assert!(error.to_string().contains("parse"), "{error}");
    }

    #[test]
    fn rejects_unsafe_technique_calibration() {
        let error = Ot2AdapterProfile::parse("bench-two", "[techniques]\ntouch_tip_radius = 1.5\n")
            .expect_err("fractions outside the unit interval are unsafe");
        assert!(error.to_string().contains("touch_tip_radius"));
    }
}
