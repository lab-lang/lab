//! Site configuration for one OT-2 bench.
//!
//! A profile describes the laboratory, not the science: which modules are
//! installed, which labware sits in which deck slot, and which pipette is on
//! which mount. Two laboratories running the same Lab program supply different
//! profiles; neither program changes.
//!
//! Every field has a default, so a profile states only what differs from the
//! bench this backend was developed against. Unknown keys are rejected, because
//! a misspelled slot silently falling back to a default is how a protocol ends
//! up aspirating from the wrong place.

mod defaults;
mod error;
mod schema;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use error::Ot2ProfileError;
// Only the field types other `opentrons_ot2` submodules reach into directly
// are re-exported; the rest of the schema stays behind `Ot2TargetProfile`.
use schema::{Instruments, SharedDeck, TargetMetadata};
pub use schema::{Plates, Stages};

/// Deck slots an OT-2 can address. Slot 12 is the fixed trash.
const ADDRESSABLE_SLOTS: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"];
/// Slots the Thermocycler Module GEN2 occupies when installed.
const THERMOCYCLER_SLOTS: [&str; 4] = ["7", "8", "10", "11"];

/// The complete OT-2 site configuration consumed by planning and emission.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ot2TargetProfile {
    #[serde(default)]
    pub target: TargetMetadata,
    #[serde(default)]
    pub instruments: Instruments,
    #[serde(default)]
    pub deck: SharedDeck,
    #[serde(default)]
    pub stages: Stages,
}

impl Ot2TargetProfile {
    pub fn parse(text: &str) -> Result<Self, Ot2ProfileError> {
        let profile: Self = toml::from_str(text)?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), Ot2ProfileError> {
        if self.target.backend != defaults::default_backend() {
            return Err(Ot2ProfileError::WrongBackend {
                expected: "opentrons.ot2",
                found: self.target.backend.clone(),
            });
        }
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
    use super::*;

    #[test]
    fn an_empty_profile_describes_the_reference_bench() {
        let profile = Ot2TargetProfile::parse("").unwrap();
        assert_eq!(profile, Ot2TargetProfile::default());
        assert_eq!(profile.deck.temperature_module.slot, "1");
        assert_eq!(profile.stages.plating.agar_plate.slots, ["5", "6"]);
        assert_eq!(profile.stages.plating.agar_plate.total_capacity(), 192);
    }

    #[test]
    fn a_profile_overrides_only_what_it_states() {
        let profile = Ot2TargetProfile::parse(
            r#"
[target]
name = "bench-two"

[stages.plating.agar_plate]
labware = "nest_96_wellplate_100ul_pcr_full_skirt"
slots = ["5"]
capacity = 96
"#,
        )
        .unwrap();
        assert_eq!(profile.target.name, "bench-two");
        assert_eq!(profile.stages.plating.agar_plate.slots, ["5"]);
        assert_eq!(
            profile.stages.plating.dilution_plate.slots,
            ["2", "3"],
            "an unstated stage keeps the reference layout"
        );
    }

    #[test]
    fn rejects_labware_placed_under_the_thermocycler() {
        let error = Ot2TargetProfile::parse(
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
        let error = Ot2TargetProfile::parse(
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
        let error = Ot2TargetProfile::parse("[stages.plating]\nagar_plates = 2\n")
            .expect_err("a misspelled key must not fall back to a default");
        assert!(error.to_string().contains("parse"), "{error}");
    }
}
