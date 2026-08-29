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
// Only the field types other `ot2` submodules reach into directly
// are re-exported; the rest of the schema stays behind `Ot2AdapterProfile`.
use schema::{Instruments, ProtocolOptions, SharedDeck};
pub use schema::{Plates, Stages};

/// Deck slots an OT-2 can address. Slot 12 is the fixed trash.
const ADDRESSABLE_SLOTS: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"];
/// Slots the Thermocycler Module GEN2 occupies when installed.
const THERMOCYCLER_SLOTS: [&str; 4] = ["7", "8", "10", "11"];

/// The complete OT-2 implementation configuration consumed by planning and emission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    pub deck: SharedDeck,
    #[serde(default)]
    pub stages: Stages,
}

impl Default for Ot2AdapterProfile {
    fn default() -> Self {
        Self {
            name: "opentrons.ot2".to_owned(),
            protocol: ProtocolOptions::default(),
            instruments: Instruments::default(),
            deck: SharedDeck::default(),
            stages: Stages::default(),
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
        for (stage, claims) in [
            ("assembly", self.assembly_claims()),
            ("transformation", self.transformation_claims()),
            ("plating", self.plating_claims()),
        ] {
            let mut seen: Vec<(String, String)> = Vec::new();
            for (context, slots) in claims {
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
                            stage,
                            slot,
                            first: first.clone(),
                            second: context,
                        });
                    }
                    seen.push((context.clone(), slot));
                }
            }
        }
        Ok(())
    }

    fn temperature_claim(&self) -> (String, Vec<String>) {
        (
            "the temperature module".to_owned(),
            vec![self.deck.temperature_module.slot.clone()],
        )
    }

    fn assembly_claims(&self) -> Vec<(String, Vec<String>)> {
        vec![
            self.temperature_claim(),
            (
                "assembly small tips".to_owned(),
                self.stages.assembly.small_tips.slots.clone(),
            ),
        ]
    }

    fn transformation_claims(&self) -> Vec<(String, Vec<String>)> {
        let stage = &self.stages.transformation;
        vec![
            self.temperature_claim(),
            ("the DNA plate".to_owned(), stage.dna_plate.slots.clone()),
            (
                "transformation small tips".to_owned(),
                stage.small_tips.slots.clone(),
            ),
            (
                "transformation large tips".to_owned(),
                stage.large_tips.slots.clone(),
            ),
        ]
    }

    fn plating_claims(&self) -> Vec<(String, Vec<String>)> {
        let stage = &self.stages.plating;
        vec![
            (
                "the dilution plate".to_owned(),
                stage.dilution_plate.slots.clone(),
            ),
            ("the agar plate".to_owned(), stage.agar_plate.slots.clone()),
            (
                "the media rack".to_owned(),
                vec![stage.media_rack.slot.clone()],
            ),
            (
                "plating small tips".to_owned(),
                stage.small_tips.slots.clone(),
            ),
            (
                "plating large tips".to_owned(),
                stage.large_tips.slots.clone(),
            ),
        ]
    }

    /// Labware load names this profile references, for reporting what an
    /// operator must have on hand.
    pub fn labware(&self) -> BTreeSet<String> {
        let stages = &self.stages;
        BTreeSet::from([
            self.deck.temperature_module.labware.clone(),
            self.deck.thermocycler.labware.clone(),
            stages.assembly.small_tips.labware.clone(),
            stages.transformation.dna_plate.labware.clone(),
            stages.transformation.small_tips.labware.clone(),
            stages.transformation.large_tips.labware.clone(),
            stages.plating.dilution_plate.labware.clone(),
            stages.plating.agar_plate.labware.clone(),
            stages.plating.media_rack.labware.clone(),
            stages.plating.small_tips.labware.clone(),
            stages.plating.large_tips.labware.clone(),
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
        assert_eq!(profile.deck.temperature_module.slot, "1");
        assert_eq!(profile.stages.plating.agar_plate.slots, ["5", "6"]);
        assert_eq!(profile.stages.plating.agar_plate.total_capacity(), 192);
    }

    #[test]
    fn a_profile_overrides_only_what_it_states() {
        let profile = Ot2AdapterProfile::parse(
            "bench-two",
            r#"
[stages.plating.agar_plate]
labware = "nest_96_wellplate_100ul_pcr_full_skirt"
slots = ["5"]
capacity = 96
"#,
        )
        .unwrap();
        assert_eq!(profile.stages.plating.agar_plate.slots, ["5"]);
        assert_eq!(
            profile.stages.plating.dilution_plate.slots,
            ["2", "3"],
            "an unstated stage keeps the reference layout"
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
[stages.assembly.small_tips]
labware = "opentrons_96_tiprack_20ul"
slots = ["7"]
capacity = 96
"#,
        )
        .expect_err("slot 7 is occupied by the thermocycler");
        assert!(error.to_string().contains("thermocycler"), "{error}");
    }

    #[test]
    fn rejects_two_labware_in_one_slot_during_a_stage() {
        let error = Ot2AdapterProfile::parse(
            "bench-two",
            r#"
[stages.plating.agar_plate]
labware = "nest_96_wellplate_100ul_pcr_full_skirt"
slots = ["4"]
capacity = 96
"#,
        )
        .expect_err("slot 4 already holds the media rack");
        assert!(error.to_string().contains("claimed by both"), "{error}");
    }

    #[test]
    fn rejects_an_unknown_key_rather_than_silently_ignoring_it() {
        let error = Ot2AdapterProfile::parse("bench-two", "[stages.plating]\nagar_plates = 2\n")
            .expect_err("a misspelled key must not fall back to a default");
        assert!(error.to_string().contains("parse"), "{error}");
    }
}
