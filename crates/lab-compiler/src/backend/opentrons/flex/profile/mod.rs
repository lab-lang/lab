//! Operational configuration for the Opentrons Flex adapter.
//!
//! Facility allocation has already selected an exact Asset before this profile is read. The profile contains only checked configuration the implementation still needs to produce an executable protocol. It cannot select a facility Asset or another adapter.
//!
//! Every field has a default, so a profile states only what differs from the reference implementation configuration. Unknown keys are rejected, because a misspelled slot silently falling back to a default is how a protocol ends up aspirating from the wrong place.

mod defaults;
mod error;
mod schema;

use std::collections::BTreeSet;

use opentrons_protocol::{FlexPipetteName, FlexSlot, TrashArea};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use crate::backend::opentrons::flex::profile::error::FlexProfileError;
// Only the field types other `flex` submodules reach into directly
// are re-exported; the rest of the schema stays behind `FlexAdapterProfile`.
use crate::backend::opentrons::flex::profile::schema::{
    FlexDeck, FlexTechniqueCalibration, Instruments,
};
pub use crate::backend::opentrons::flex::profile::schema::{Pipette, Stages, TipRacks};

/// Slots the installed thermocycler occupies.
const THERMOCYCLER_SLOTS: [FlexSlot; 2] = [FlexSlot::A1, FlexSlot::B1];

/// The complete Flex implementation configuration consumed by planning and emission.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlexAdapterProfile {
    /// File-stem label supplied by the exact Asset binding. It is review metadata, not profile input.
    #[serde(skip)]
    #[schemars(skip)]
    pub name: String,
    #[serde(default)]
    pub instruments: Instruments,
    #[serde(default)]
    pub deck: FlexDeck,
    #[serde(default)]
    pub stages: Stages,
    #[serde(default)]
    pub techniques: FlexTechniqueCalibration,
}

impl Default for FlexAdapterProfile {
    fn default() -> Self {
        Self {
            name: "opentrons.flex".to_owned(),
            instruments: Instruments::default(),
            deck: FlexDeck::default(),
            techniques: FlexTechniqueCalibration::default(),
            stages: Stages::default(),
        }
    }
}

impl FlexAdapterProfile {
    /// Load operational configuration for one exact Asset binding.
    pub fn parse(name: &str, text: &str) -> Result<Self, FlexProfileError> {
        let mut profile: Self = toml::from_str(text)?;
        profile.name = name.to_owned();
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), FlexProfileError> {
        self.validate_instruments()?;
        self.validate_deck()?;
        self.techniques
            .validate()
            .map_err(|message| FlexProfileError::InvalidTechnique { message })?;
        for (stage, claims) in [
            ("assembly", self.assembly_claims()),
            ("transformation", self.transformation_claims()),
            ("plating", self.plating_claims()),
        ] {
            let mut seen: Vec<(String, String)> = Vec::new();
            for (context, slots) in claims {
                if slots.is_empty() {
                    return Err(FlexProfileError::NoSlots { context });
                }
                for slot in slots {
                    if FlexSlot::parse(&slot).is_none() {
                        return Err(FlexProfileError::UnknownSlot { context, slot });
                    }
                    if let Some((first, _)) = seen.iter().find(|(_, taken)| taken == &slot) {
                        return Err(FlexProfileError::SlotConflict {
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

    fn validate_instruments(&self) -> Result<(), FlexProfileError> {
        for (instrument, pipette) in [
            ("small", &self.instruments.small),
            ("large", &self.instruments.large),
        ] {
            if FlexPipetteName::parse(&pipette.model).is_none() {
                return Err(FlexProfileError::UnknownPipette {
                    instrument,
                    model: pipette.model.clone(),
                });
            }
            if !["left", "right"].contains(&pipette.mount.as_str()) {
                return Err(FlexProfileError::UnknownMount {
                    instrument,
                    mount: pipette.mount.clone(),
                });
            }
        }
        if self.instruments.small.mount == self.instruments.large.mount {
            return Err(FlexProfileError::SharedMount {
                mount: self.instruments.small.mount.clone(),
            });
        }
        Ok(())
    }

    fn validate_deck(&self) -> Result<(), FlexProfileError> {
        if self.deck.temperature_module.model != "temperatureModuleV2" {
            return Err(FlexProfileError::WrongModuleModel {
                module: "temperature module",
                expected: "temperatureModuleV2",
                found: self.deck.temperature_module.model.clone(),
            });
        }
        if self.deck.thermocycler.model != "thermocyclerModuleV2" {
            return Err(FlexProfileError::WrongModuleModel {
                module: "thermocycler",
                expected: "thermocyclerModuleV2",
                found: self.deck.thermocycler.model.clone(),
            });
        }
        if TrashArea::parse(&self.deck.trash.area).is_none() {
            return Err(FlexProfileError::UnknownTrashArea {
                found: self.deck.trash.area.clone(),
            });
        }
        match FlexSlot::parse(&self.deck.temperature_module.slot) {
            None => {
                return Err(FlexProfileError::UnknownSlot {
                    context: "the temperature module".into(),
                    slot: self.deck.temperature_module.slot.clone(),
                });
            }
            Some(slot) if slot.column() == 2 => {
                return Err(FlexProfileError::TemperatureModuleColumn {
                    slot: self.deck.temperature_module.slot.clone(),
                });
            }
            Some(_) => {}
        }
        Ok(())
    }

    /// The trash area this profile places tips in, verified by
    /// [`Self::validate`].
    pub fn trash_area(&self) -> TrashArea {
        TrashArea::parse(&self.deck.trash.area)
            .expect("profile validation accepted only a known trash area")
    }

    /// Deck claims present in every stage: the thermocycler's two slots and
    /// the trash bin's slot.
    fn fixed_claims(&self) -> Vec<(String, Vec<String>)> {
        vec![
            (
                "the thermocycler".to_owned(),
                THERMOCYCLER_SLOTS
                    .iter()
                    .map(|slot| slot.as_str().to_owned())
                    .collect(),
            ),
            (
                "the trash bin".to_owned(),
                vec![self.trash_area().slot().as_str().to_owned()],
            ),
        ]
    }

    fn temperature_claim(&self) -> (String, Vec<String>) {
        (
            "the temperature module".to_owned(),
            vec![self.deck.temperature_module.slot.clone()],
        )
    }

    fn assembly_claims(&self) -> Vec<(String, Vec<String>)> {
        let mut claims = self.fixed_claims();
        claims.push(self.temperature_claim());
        claims.push((
            "assembly small tips".to_owned(),
            self.stages.assembly.small_tips.slots.clone(),
        ));
        claims
    }

    fn transformation_claims(&self) -> Vec<(String, Vec<String>)> {
        let stage = &self.stages.transformation;
        let mut claims = self.fixed_claims();
        claims.push(self.temperature_claim());
        claims.push(("the DNA plate".to_owned(), stage.dna_plate.slots.clone()));
        claims.push((
            "transformation small tips".to_owned(),
            stage.small_tips.slots.clone(),
        ));
        claims.push((
            "transformation large tips".to_owned(),
            stage.large_tips.slots.clone(),
        ));
        claims
    }

    fn plating_claims(&self) -> Vec<(String, Vec<String>)> {
        let stage = &self.stages.plating;
        let mut claims = self.fixed_claims();
        claims.push((
            "the dilution plate".to_owned(),
            stage.dilution_plate.slots.clone(),
        ));
        claims.push(("the agar plate".to_owned(), stage.agar_plate.slots.clone()));
        claims.push((
            "the media rack".to_owned(),
            vec![stage.media_rack.slot.clone()],
        ));
        claims.push((
            "plating small tips".to_owned(),
            stage.small_tips.slots.clone(),
        ));
        claims.push((
            "plating large tips".to_owned(),
            stage.large_tips.slots.clone(),
        ));
        claims
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
    use crate::backend::opentrons::flex::profile::*;

    #[test]
    fn an_empty_profile_describes_the_reference_bench() {
        let profile = FlexAdapterProfile::parse("reference-bench", "").unwrap();
        assert_eq!(profile.name, "reference-bench");
        assert_eq!(profile.deck.temperature_module.slot, "C1");
        assert_eq!(profile.deck.trash.area, "movableTrashA3");
        assert_eq!(profile.stages.plating.agar_plate.slots, ["B2", "B3"]);
        assert_eq!(profile.stages.plating.agar_plate.total_capacity(), 192);
    }

    #[test]
    fn a_profile_overrides_only_what_it_states() {
        let profile = FlexAdapterProfile::parse(
            "bench-two",
            r#"
[stages.plating.agar_plate]
labware = "nest_96_wellplate_100ul_pcr_full_skirt"
slots = ["B2"]
capacity = 96
"#,
        )
        .unwrap();
        assert_eq!(profile.stages.plating.agar_plate.slots, ["B2"]);
        assert_eq!(
            profile.stages.plating.dilution_plate.slots,
            ["C2", "C3"],
            "an unstated stage keeps the reference layout"
        );
    }

    #[test]
    fn rejects_an_embedded_target_or_adapter_selector() {
        let error =
            FlexAdapterProfile::parse("flex-runtime", "[target]\nbackend = \"opentrons.ot2\"\n")
                .expect_err("only the exact Asset binding may select an adapter");
        assert!(error.to_string().contains("target"), "{error}");
    }

    #[test]
    fn rejects_an_ot2_pipette_on_a_flex_bench() {
        let error = FlexAdapterProfile::parse(
            "bench-two",
            "[instruments.small]\nmodel = \"p20_single_gen2\"\nmount = \"left\"\n",
        )
        .expect_err("gen2 pipettes do not fit a Flex");
        assert!(error.to_string().contains("p20_single_gen2"), "{error}");
    }

    #[test]
    fn rejects_labware_placed_under_the_thermocycler() {
        let error = FlexAdapterProfile::parse(
            "bench-two",
            r#"
[stages.assembly.small_tips]
labware = "opentrons_flex_96_tiprack_50ul"
slots = ["A1"]
capacity = 96
"#,
        )
        .expect_err("slot A1 is occupied by the thermocycler");
        assert!(error.to_string().contains("thermocycler"), "{error}");
    }

    #[test]
    fn rejects_labware_placed_in_the_trash_slot() {
        let error = FlexAdapterProfile::parse(
            "bench-two",
            r#"
[stages.assembly.small_tips]
labware = "opentrons_flex_96_tiprack_50ul"
slots = ["A3"]
capacity = 96
"#,
        )
        .expect_err("slot A3 holds the trash bin");
        assert!(error.to_string().contains("trash"), "{error}");
    }

    #[test]
    fn rejects_a_temperature_module_in_column_2() {
        let error = FlexAdapterProfile::parse(
            "bench-two",
            "[deck.temperature_module]\nmodel = \"temperatureModuleV2\"\nslot = \"C2\"\nlabware = \"opentrons_24_aluminumblock_nest_1.5ml_snapcap\"\ncapacity = 24\n",
        )
        .expect_err("module caddies exist in columns 1 and 3 only");
        assert!(error.to_string().contains("column 1 or 3"), "{error}");
    }

    #[test]
    fn rejects_a_staging_slot() {
        let error = FlexAdapterProfile::parse(
            "bench-two",
            r#"
[stages.transformation.dna_plate]
labware = "nest_96_wellplate_100ul_pcr_full_skirt"
slots = ["C4"]
capacity = 96
"#,
        )
        .expect_err("staging slots are not pipette-addressable");
        assert!(error.to_string().contains("C4"), "{error}");
    }

    #[test]
    fn rejects_an_unknown_key_rather_than_silently_ignoring_it() {
        let error = FlexAdapterProfile::parse("bench-two", "[stages.plating]\nagar_plates = 2\n")
            .expect_err("a misspelled key must not fall back to a default");
        assert!(error.to_string().contains("parse"), "{error}");
    }
}
