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
// These types occur in public profile fields, so they remain nameable without exposing the
// private source-module layout used to implement the schema.
pub use crate::backend::opentrons::flex::profile::schema::{
    DeckLabware, FlexResources, FlexTechniqueCalibration, Instruments, Pipette, TemperatureModule,
    Thermocycler, TipRacks, Trash,
};
pub use crate::backend::resources::{
    PlateCapacity, UnknownPlateGeometry, supported_plate_capacities,
};

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
    pub resources: FlexResources,
    #[serde(default)]
    pub techniques: FlexTechniqueCalibration,
}

impl Default for FlexAdapterProfile {
    fn default() -> Self {
        Self {
            name: "opentrons.flex".to_owned(),
            instruments: Instruments::default(),
            resources: FlexResources::default(),
            techniques: FlexTechniqueCalibration::default(),
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
        self.validate_resources()?;
        self.techniques
            .validate()
            .map_err(|message| FlexProfileError::InvalidTechnique { message })?;
        let mut seen: Vec<(String, String)> = Vec::new();
        for (context, slots) in self.resource_claims() {
            if slots.is_empty() {
                return Err(FlexProfileError::NoSlots { context });
            }
            for slot in slots {
                if FlexSlot::parse(&slot).is_none() {
                    return Err(FlexProfileError::UnknownSlot { context, slot });
                }
                if let Some((first, _)) = seen.iter().find(|(_, taken)| taken == &slot) {
                    return Err(FlexProfileError::SlotConflict {
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

    fn validate_resources(&self) -> Result<(), FlexProfileError> {
        if self.resources.sources.model != "temperatureModuleV2" {
            return Err(FlexProfileError::WrongModuleModel {
                module: "temperature module",
                expected: "temperatureModuleV2",
                found: self.resources.sources.model.clone(),
            });
        }
        if self.resources.work.model != "thermocyclerModuleV2" {
            return Err(FlexProfileError::WrongModuleModel {
                module: "thermocycler",
                expected: "thermocyclerModuleV2",
                found: self.resources.work.model.clone(),
            });
        }
        if TrashArea::parse(&self.resources.trash.area).is_none() {
            return Err(FlexProfileError::UnknownTrashArea {
                found: self.resources.trash.area.clone(),
            });
        }
        match FlexSlot::parse(&self.resources.sources.slot) {
            None => {
                return Err(FlexProfileError::UnknownSlot {
                    context: "the temperature module".into(),
                    slot: self.resources.sources.slot.clone(),
                });
            }
            Some(slot) if slot.column() == 2 => {
                return Err(FlexProfileError::TemperatureModuleColumn {
                    slot: self.resources.sources.slot.clone(),
                });
            }
            Some(_) => {}
        }
        Ok(())
    }

    /// The trash area this profile places tips in, verified by
    /// [`Self::validate`].
    pub fn trash_area(&self) -> TrashArea {
        TrashArea::parse(&self.resources.trash.area)
            .expect("profile validation accepted only a known trash area")
    }

    /// Every addressable resource is present for the complete canonical program.
    fn resource_claims(&self) -> Vec<(String, Vec<String>)> {
        let mut claims = vec![
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
        ];
        claims.push((
            "the source module".to_owned(),
            vec![self.resources.sources.slot.clone()],
        ));
        claims.push((
            "bulk liquids".to_owned(),
            vec![self.resources.bulk.slot.clone()],
        ));
        claims.push((
            "small tips".to_owned(),
            self.resources.small_tips.slots.clone(),
        ));
        claims.push((
            "large tips".to_owned(),
            self.resources.large_tips.slots.clone(),
        ));
        claims
    }

    /// Labware load names this profile references, for reporting what an
    /// operator must have on hand.
    pub fn labware(&self) -> BTreeSet<String> {
        BTreeSet::from([
            self.resources.sources.labware.clone(),
            self.resources.work.labware.clone(),
            self.resources.bulk.labware.clone(),
            self.resources.small_tips.labware.clone(),
            self.resources.large_tips.labware.clone(),
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
        assert_eq!(profile.resources.sources.slot, "C1");
        assert_eq!(profile.resources.work.capacity.get(), 96);
        assert_eq!(profile.resources.trash.area, "movableTrashA3");
        assert_eq!(profile.resources.small_tips.slots, ["C2"]);
        assert_eq!(profile.resources.large_tips.slots, ["D2"]);
    }

    #[test]
    fn a_profile_overrides_only_what_it_states() {
        let profile = FlexAdapterProfile::parse(
            "bench-two",
            r#"
[resources.large_tips]
labware = "opentrons_flex_96_tiprack_1000ul"
slots = ["B2"]
capacity = 96
"#,
        )
        .unwrap();
        assert_eq!(profile.resources.large_tips.slots, ["B2"]);
        assert_eq!(
            profile.resources.small_tips.slots,
            ["C2"],
            "an unstated resource keeps the reference layout"
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
[resources.small_tips]
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
[resources.small_tips]
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
            "[resources.sources]\nmodel = \"temperatureModuleV2\"\nslot = \"C2\"\nlabware = \"opentrons_24_aluminumblock_nest_1.5ml_snapcap\"\ncapacity = 24\n",
        )
        .expect_err("module caddies exist in columns 1 and 3 only");
        assert!(error.to_string().contains("column 1 or 3"), "{error}");
    }

    #[test]
    fn rejects_a_staging_slot() {
        let error = FlexAdapterProfile::parse(
            "bench-two",
            r#"
[resources.small_tips]
labware = "opentrons_flex_96_tiprack_50ul"
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
            .expect_err("the removed scientific stage schema must not be accepted");
        assert!(error.to_string().contains("parse"), "{error}");
    }
}
