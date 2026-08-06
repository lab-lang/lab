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

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Deck slots an OT-2 can address. Slot 12 is the fixed trash.
const ADDRESSABLE_SLOTS: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"];
/// Slots the Thermocycler Module GEN2 occupies when installed.
const THERMOCYCLER_SLOTS: [&str; 4] = ["7", "8", "10", "11"];

#[derive(Debug, Error)]
pub enum Ot2ProfileError {
    #[error("failed to parse OT-2 target profile: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("target profile declares backend '{found}', but this backend is '{expected}'")]
    WrongBackend {
        expected: &'static str,
        found: String,
    },
    #[error("{context} names deck slot '{slot}', which an OT-2 does not address")]
    UnknownSlot { context: String, slot: String },
    #[error(
        "{context} claims deck slot '{slot}', which the installed thermocycler already occupies"
    )]
    ThermocyclerSlot { context: String, slot: String },
    #[error("deck slot '{slot}' is claimed by both {first} and {second} during {stage}")]
    SlotConflict {
        stage: &'static str,
        slot: String,
        first: String,
        second: String,
    },
    #[error("{context} must declare at least one deck slot")]
    NoSlots { context: String },
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetMetadata {
    #[serde(default = "default_profile_name")]
    pub name: String,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_api_level")]
    pub api_level: String,
}

impl Default for TargetMetadata {
    fn default() -> Self {
        Self {
            name: default_profile_name(),
            backend: default_backend(),
            api_level: default_api_level(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instruments {
    #[serde(default = "default_small_pipette")]
    pub small: Pipette,
    #[serde(default = "default_large_pipette")]
    pub large: Pipette,
}

impl Default for Instruments {
    fn default() -> Self {
        Self {
            small: default_small_pipette(),
            large: default_large_pipette(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pipette {
    pub model: String,
    pub mount: String,
}

/// Hardware present for every stage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedDeck {
    #[serde(default = "default_temperature_module")]
    pub temperature_module: TemperatureModule,
    #[serde(default = "default_thermocycler")]
    pub thermocycler: Thermocycler,
}

impl Default for TemperatureModule {
    fn default() -> Self {
        default_temperature_module()
    }
}

impl Default for Thermocycler {
    fn default() -> Self {
        default_thermocycler()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemperatureModule {
    pub model: String,
    pub slot: String,
    /// Rack of chilled source tubes carried on the module.
    pub labware: String,
    pub capacity: usize,
}

/// The thermocycler occupies fixed slots, so it declares no slot of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thermocycler {
    pub model: String,
    pub labware: String,
    pub capacity: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stages {
    #[serde(default = "default_assembly_stage")]
    pub assembly: AssemblyStage,
    #[serde(default = "default_transformation_stage")]
    pub transformation: TransformationStage,
    #[serde(default = "default_plating_stage")]
    pub plating: PlatingStage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyStage {
    #[serde(default = "default_assembly_small_tips")]
    pub small_tips: TipRacks,
}

impl Default for AssemblyStage {
    fn default() -> Self {
        default_assembly_stage()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformationStage {
    /// Plate holding the assembled plasmids a transformation draws from.
    #[serde(default = "default_transformation_dna_plate")]
    pub dna_plate: Plates,
    #[serde(default = "default_transformation_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_transformation_large_tips")]
    pub large_tips: TipRacks,
}

impl Default for TransformationStage {
    fn default() -> Self {
        default_transformation_stage()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatingStage {
    #[serde(default = "default_dilution_plate")]
    pub dilution_plate: Plates,
    #[serde(default = "default_agar_plate")]
    pub agar_plate: Plates,
    #[serde(default = "default_media_rack")]
    pub media_rack: MediaRack,
    #[serde(default = "default_plating_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_plating_large_tips")]
    pub large_tips: TipRacks,
}

impl Default for PlatingStage {
    fn default() -> Self {
        default_plating_stage()
    }
}

/// One or more identical plates. Allocation fills each in turn, so adding a
/// slot raises a build's capacity without changing any program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plates {
    pub labware: String,
    pub slots: Vec<String>,
    #[serde(default = "default_plate_capacity")]
    pub capacity: usize,
}

impl Plates {
    pub fn total_capacity(&self) -> usize {
        self.slots.len() * self.capacity
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TipRacks {
    pub labware: String,
    pub slots: Vec<String>,
    #[serde(default = "default_plate_capacity")]
    pub capacity: usize,
}

impl TipRacks {
    pub fn total_capacity(&self) -> usize {
        self.slots.len() * self.capacity
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRack {
    pub labware: String,
    pub slot: String,
    pub medium_well: String,
}

impl Ot2TargetProfile {
    pub fn parse(text: &str) -> Result<Self, Ot2ProfileError> {
        let profile: Self = toml::from_str(text)?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), Ot2ProfileError> {
        if self.target.backend != default_backend() {
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

fn default_profile_name() -> String {
    "opentrons-ot2".to_owned()
}

fn default_backend() -> String {
    "opentrons.ot2".to_owned()
}

fn default_api_level() -> String {
    "2.21".to_owned()
}

fn default_small_pipette() -> Pipette {
    Pipette {
        model: "p20_single_gen2".to_owned(),
        mount: "left".to_owned(),
    }
}

fn default_large_pipette() -> Pipette {
    Pipette {
        model: "p300_single_gen2".to_owned(),
        mount: "right".to_owned(),
    }
}

fn default_temperature_module() -> TemperatureModule {
    TemperatureModule {
        model: "temperature module gen2".to_owned(),
        slot: "1".to_owned(),
        labware: "opentrons_24_aluminumblock_nest_1.5ml_snapcap".to_owned(),
        capacity: 24,
    }
}

fn default_thermocycler() -> Thermocycler {
    Thermocycler {
        model: "thermocycler module gen2".to_owned(),
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        capacity: 96,
    }
}

fn default_plate_capacity() -> usize {
    96
}

fn default_assembly_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_tiprack_20ul".to_owned(),
        slots: vec!["2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_assembly_stage() -> AssemblyStage {
    AssemblyStage {
        small_tips: default_assembly_small_tips(),
    }
}

fn default_transformation_dna_plate() -> Plates {
    Plates {
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        slots: vec!["2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_transformation_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_tiprack_20ul".to_owned(),
        slots: vec!["3".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_transformation_large_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_200ul".to_owned(),
        slots: vec!["6".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_transformation_stage() -> TransformationStage {
    TransformationStage {
        dna_plate: default_transformation_dna_plate(),
        small_tips: default_transformation_small_tips(),
        large_tips: default_transformation_large_tips(),
    }
}

fn default_dilution_plate() -> Plates {
    Plates {
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        slots: vec!["2".to_owned(), "3".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_agar_plate() -> Plates {
    Plates {
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        slots: vec!["5".to_owned(), "6".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_media_rack() -> MediaRack {
    MediaRack {
        labware: "opentrons_15_tuberack_falcon_15ml_conical".to_owned(),
        slot: "4".to_owned(),
        medium_well: "A1".to_owned(),
    }
}

fn default_plating_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_20ul".to_owned(),
        slots: vec!["9".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_plating_large_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_200ul".to_owned(),
        slots: vec!["1".to_owned()],
        capacity: default_plate_capacity(),
    }
}

fn default_plating_stage() -> PlatingStage {
    PlatingStage {
        dilution_plate: default_dilution_plate(),
        agar_plate: default_agar_plate(),
        media_rack: default_media_rack(),
        small_tips: default_plating_small_tips(),
        large_tips: default_plating_large_tips(),
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
