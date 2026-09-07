//! Operational configuration for the Hamilton STAR adapter.
//!
//! Facility allocation has already selected an exact Asset before this profile is read. The profile contains only checked configuration the implementation still needs to produce and execute reviewed firmware frames. It cannot select a facility Asset or another adapter.
//!
//! Every field has a default matching the reference implementation configuration, so a profile states only what differs. Unknown keys are rejected, because a misspelled site silently falling back to a default is how a protocol ends up aspirating from the wrong place. Labware sites are addressed as `"<carrier>/<site>"` with 1-based site numbers.

use std::collections::BTreeMap;

use schemars::JsonSchema;

use crate::backend::resources::PlateCapacity;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::backend::profile::TipRacks;

use crate::backend::hamilton::star::catalog::{
    self, CarrierDefinition, DeckPosition, LabwareDefinition,
};
use crate::backend::hamilton::star::liquid_classes::{
    LiquidClassError, LiquidClassLibraryRegistry,
};

/// The error raised when a profile cannot describe a workable bench.
#[derive(Debug, Error, PartialEq)]
pub enum StarProfileError {
    #[error("failed to parse STAR adapter profile TOML: {0}")]
    Toml(String),
    #[error("machine variant '{found}' is unknown; this backend knows 'star' and 'starlet'")]
    UnknownVariant { found: String },
    #[error(
        "the profile declares {found} pipetting channels; this backend plans for 8-channel machines only"
    )]
    UnsupportedChannels { found: usize },
    #[error(
        "carrier '{name}' names catalog entry '{catalog}', which does not exist; known carriers are {known}"
    )]
    UnknownCarrier {
        name: String,
        catalog: String,
        known: String,
    },
    #[error(
        "carrier '{name}' at rail {rail} spans through rail {end}, but the {variant} deck has {rails} rails"
    )]
    RailOutOfRange {
        name: String,
        rail: u32,
        end: u32,
        variant: &'static str,
        rails: u32,
    },
    #[error(
        "carriers '{first}' and '{second}' overlap on rail {rail}; every carrier needs its own rails"
    )]
    CarrierOverlap {
        first: String,
        second: String,
        rail: u32,
    },
    #[error(
        "'{context}' addresses site '{address}'; a site is '<carrier>/<site>' with a 1-based site number"
    )]
    BadSiteAddress { context: String, address: String },
    #[error("'{context}' addresses carrier '{name}', which the profile does not place")]
    UnplacedCarrier { context: String, name: String },
    #[error("'{context}' addresses site {site} of carrier '{name}', which has {sites} sites")]
    SiteOutOfRange {
        context: String,
        name: String,
        site: usize,
        sites: usize,
    },
    #[error(
        "site '{address}' is claimed by both '{first}' and '{second}'; each site holds one labware"
    )]
    SiteConflict {
        address: String,
        first: String,
        second: String,
    },
    #[error(
        "'{context}' names labware '{labware}', which does not exist; known labware are {known}"
    )]
    UnknownLabware {
        context: String,
        labware: String,
        known: String,
    },
    #[error(
        "'{context}' declares capacity {declared} for labware '{labware}', which has {actual} positions"
    )]
    CapacityMismatch {
        context: String,
        labware: String,
        declared: usize,
        actual: usize,
    },
    #[error("'{context}' needs a tip rack, but labware '{labware}' holds liquid")]
    TipRackExpected { context: String, labware: String },
    #[error("'{context}' needs a liquid vessel, but labware '{labware}' is a tip rack")]
    VesselExpected { context: String, labware: String },
    #[error(
        "liquid level detection mode '{found}' is unknown; this backend knows 'off' and 'gamma'"
    )]
    UnknownLldMode { found: String },
    #[error("'{context}' declares no sites; a stage resource needs at least one")]
    NoSites { context: String },
    #[error("the STAR liquid-class registry is invalid: {0}")]
    LiquidClasses(#[source] Box<LiquidClassError>),
    #[error("invalid STAR liquid handling: {0}")]
    LiquidHandling(String),
}

/// The machine variant, which fixes the deck's rail count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MachineVariant {
    /// 56 rails, 1545 mm deck.
    Star,
    /// 32 rails, 1005 mm deck.
    Starlet,
}

impl MachineVariant {
    pub fn rails(self) -> u32 {
        match self {
            MachineVariant::Star => 56,
            MachineVariant::Starlet => 32,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            MachineVariant::Star => "star",
            MachineVariant::Starlet => "starlet",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    #[serde(default = "default_variant")]
    pub variant: MachineVariant,
    /// Pipetting channel count. Planning batches for the 8-channel
    /// machine; other counts are rejected until a bench proves them out.
    #[serde(default = "default_channels")]
    pub channels: usize,
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            variant: default_variant(),
            channels: default_channels(),
        }
    }
}

/// One catalog carrier placed on the deck.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CarrierPlacement {
    /// The catalog carrier id.
    pub catalog: String,
    /// The 1-based rail the carrier's left edge sits on.
    pub rail: u32,
}

/// One labware on one carrier site.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlacedLabware {
    /// `"<carrier>/<site>"`, 1-based site number.
    pub site: String,
    /// The catalog labware id.
    pub labware: String,
    #[serde(default = "default_plate_capacity")]
    pub capacity: PlateCapacity,
}

/// Carrier placements on the physical deck.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StarDeck {
    #[serde(default = "default_carriers")]
    pub carriers: BTreeMap<String, CarrierPlacement>,
}

impl Default for StarDeck {
    fn default() -> Self {
        Self {
            carriers: default_carriers(),
        }
    }
}

/// Physical resources consumed by the canonical pipetting interpreter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StarResources {
    #[serde(default = "default_sources")]
    pub sources: PlacedLabware,
    #[serde(default = "default_work")]
    pub work: PlacedLabware,
    #[serde(default = "default_small_tips")]
    pub small_tips: TipRacks,
    #[serde(default = "default_large_tips")]
    pub large_tips: TipRacks,
}

impl Default for StarResources {
    fn default() -> Self {
        Self {
            sources: default_sources(),
            work: default_work(),
            small_tips: default_small_tips(),
            large_tips: default_large_tips(),
        }
    }
}

/// The liquid level detection policy a bench opts into. Planning always
/// computes deterministic heights; gamma detection adds a runtime check on
/// top of them, it never replaces them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LldPolicy {
    #[default]
    Off,
    Gamma,
}

/// Knobs the runner and command lowering read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunOptions {
    /// Where `lab run` parks the autoload after setup; absent means the
    /// autoload is raised to safe Z but not parked, because the maximum
    /// track depends on the bench.
    #[serde(default)]
    pub autoload_park_track: Option<u32>,
    /// Minimum traverse height between positions, millimeters.
    #[serde(default = "default_traverse_height")]
    pub traverse_height_mm: f64,
    #[serde(default)]
    pub lld: LldPolicy,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            autoload_park_track: None,
            traverse_height_mm: default_traverse_height(),
            lld: LldPolicy::Off,
        }
    }
}

/// The complete STAR implementation configuration consumed by planning and emission.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StarAdapterProfile {
    /// File-stem label supplied by the exact Asset binding. It is review metadata, not profile input.
    #[serde(skip)]
    #[schemars(skip)]
    pub name: String,
    #[serde(default)]
    pub machine: Machine,
    #[serde(default)]
    pub deck: StarDeck,
    #[serde(default)]
    pub resources: StarResources,
    #[serde(default)]
    pub run: RunOptions,
    /// Package-local versioned liquid-class registrations and the exact
    /// library this Asset uses.
    #[serde(default)]
    pub liquid_classes: LiquidClassLibraryRegistry,
    #[serde(default)]
    pub liquid_handling: super::liquid_handling::StarLiquidHandling,
}

impl Default for StarAdapterProfile {
    fn default() -> Self {
        Self {
            name: "hamilton.star".to_owned(),
            machine: Machine::default(),
            deck: StarDeck::default(),
            resources: StarResources::default(),
            run: RunOptions::default(),
            liquid_classes: LiquidClassLibraryRegistry::default(),
            liquid_handling: Default::default(),
        }
    }
}

/// A site address resolved against the catalog: everything needed to place
/// wells in deck millimeters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedSite {
    pub carrier: &'static CarrierDefinition,
    pub rail: u32,
    /// 0-based site index.
    pub site: usize,
    pub labware: &'static LabwareDefinition,
}

impl ResolvedSite {
    /// The deck position of a well (or tip spot) on this site.
    pub fn well(&self, well: &str) -> Option<DeckPosition> {
        catalog::well_position(self.carrier, self.rail, self.site, self.labware, well)
    }
}

impl StarAdapterProfile {
    /// Load operational configuration for one exact Asset binding.
    pub fn parse(name: &str, text: &str) -> Result<Self, StarProfileError> {
        let mut profile: Self =
            toml::from_str(text).map_err(|error| StarProfileError::Toml(error.to_string()))?;
        profile.name = name.to_owned();
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), StarProfileError> {
        self.liquid_handling
            .validate()
            .map_err(StarProfileError::LiquidHandling)?;
        if self.machine.channels != 8 {
            return Err(StarProfileError::UnsupportedChannels {
                found: self.machine.channels,
            });
        }
        for (context, slots) in [
            (
                "resources.small_tips",
                self.resources.small_tips.slots.as_slice(),
            ),
            (
                "resources.large_tips",
                self.resources.large_tips.slots.as_slice(),
            ),
        ] {
            if slots.is_empty() {
                return Err(StarProfileError::NoSites {
                    context: context.to_owned(),
                });
            }
        }
        self.liquid_classes
            .resolve_selected()
            .map_err(|source| StarProfileError::LiquidClasses(Box::new(source)))?;
        self.validate_carriers()?;
        let mut claimed: Vec<(String, String)> = Vec::new();
        for (context, address, labware, capacity, needs_tips) in self.site_claims() {
            let resolved = self.resolve_labware(&context, &address, &labware)?;
            if let Some((first, _)) = claimed.iter().find(|(_, taken)| taken == &address) {
                return Err(StarProfileError::SiteConflict {
                    address,
                    first: first.clone(),
                    second: context,
                });
            }
            claimed.push((context.clone(), address.clone()));
            if resolved.labware.capacity != capacity.get() {
                return Err(StarProfileError::CapacityMismatch {
                    context,
                    labware,
                    declared: capacity.get(),
                    actual: resolved.labware.capacity,
                });
            }
            match (needs_tips, resolved.labware.tip().is_some()) {
                (true, false) => {
                    return Err(StarProfileError::TipRackExpected { context, labware });
                }
                (false, true) => {
                    return Err(StarProfileError::VesselExpected { context, labware });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_carriers(&self) -> Result<(), StarProfileError> {
        let variant = self.machine.variant;
        let mut spans: Vec<(&String, u32, u32)> = Vec::new();
        for (name, placement) in &self.deck.carriers {
            let Some(definition) = catalog::carrier(&placement.catalog) else {
                return Err(StarProfileError::UnknownCarrier {
                    name: name.clone(),
                    catalog: placement.catalog.clone(),
                    known: known_carriers(),
                });
            };
            let end = placement.rail + definition.width_rails - 1;
            if placement.rail == 0 || end > variant.rails() {
                return Err(StarProfileError::RailOutOfRange {
                    name: name.clone(),
                    rail: placement.rail,
                    end,
                    variant: variant.name(),
                    rails: variant.rails(),
                });
            }
            for (other, start, other_end) in &spans {
                if placement.rail <= *other_end && *start <= end {
                    return Err(StarProfileError::CarrierOverlap {
                        first: (*other).clone(),
                        second: name.clone(),
                        rail: placement.rail.max(*start),
                    });
                }
            }
            spans.push((name, placement.rail, end));
        }
        Ok(())
    }

    /// Every canonical resource's `(context, site address, labware, declared
    /// capacity, expects tips)`, for validation and deck summaries.
    fn site_claims(&self) -> Vec<(String, String, String, PlateCapacity, bool)> {
        let mut claims = vec![
            (
                "resources.sources".to_string(),
                self.resources.sources.site.clone(),
                self.resources.sources.labware.clone(),
                self.resources.sources.capacity,
                false,
            ),
            (
                "resources.work".to_string(),
                self.resources.work.site.clone(),
                self.resources.work.labware.clone(),
                self.resources.work.capacity,
                false,
            ),
        ];
        let mut add_racks = |context: &str, racks: &TipRacks, tips: bool| {
            for (index, slot) in racks.slots.iter().enumerate() {
                claims.push((
                    format!("{context}[{index}]"),
                    slot.clone(),
                    racks.labware.clone(),
                    racks.capacity,
                    tips,
                ));
            }
        };
        add_racks("resources.small_tips", &self.resources.small_tips, true);
        add_racks("resources.large_tips", &self.resources.large_tips, true);
        claims
    }

    /// Resolves a `"<carrier>/<site>"` address and a labware name against
    /// the placed carriers and the catalog.
    pub fn resolve_labware(
        &self,
        context: &str,
        address: &str,
        labware_id: &str,
    ) -> Result<ResolvedSite, StarProfileError> {
        let Some((carrier_name, site_number)) = parse_site_address(address) else {
            return Err(StarProfileError::BadSiteAddress {
                context: context.to_string(),
                address: address.to_string(),
            });
        };
        let Some(placement) = self.deck.carriers.get(carrier_name) else {
            return Err(StarProfileError::UnplacedCarrier {
                context: context.to_string(),
                name: carrier_name.to_string(),
            });
        };
        let carrier = catalog::carrier(&placement.catalog)
            .expect("carrier placements were validated against the catalog");
        if site_number == 0 || site_number > carrier.sites.len() {
            return Err(StarProfileError::SiteOutOfRange {
                context: context.to_string(),
                name: carrier_name.to_string(),
                site: site_number,
                sites: carrier.sites.len(),
            });
        }
        let Some(labware) = catalog::labware(labware_id) else {
            return Err(StarProfileError::UnknownLabware {
                context: context.to_string(),
                labware: labware_id.to_string(),
                known: known_labware(),
            });
        };
        Ok(ResolvedSite {
            carrier,
            rail: placement.rail,
            site: site_number - 1,
            labware,
        })
    }
}

fn parse_site_address(address: &str) -> Option<(&str, usize)> {
    let (carrier, site) = address.split_once('/')?;
    let site: usize = site.parse().ok()?;
    (!carrier.is_empty()).then_some((carrier, site))
}

fn known_carriers() -> String {
    catalog::CARRIERS
        .iter()
        .map(|carrier| format!("'{}'", carrier.id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn known_labware() -> String {
    catalog::LABWARE
        .iter()
        .map(|labware| format!("'{}'", labware.id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn default_variant() -> MachineVariant {
    MachineVariant::Starlet
}

fn default_channels() -> usize {
    8
}

fn default_plate_capacity() -> PlateCapacity {
    star_capacity(96)
}

fn default_traverse_height() -> f64 {
    245.0
}

/// The reference bench: one carrier each for tips, sources, and work vessels.
fn default_carriers() -> BTreeMap<String, CarrierPlacement> {
    BTreeMap::from([
        (
            "tips".to_string(),
            CarrierPlacement {
                catalog: "tip_carrier_480".into(),
                rail: 1,
            },
        ),
        (
            "sources".to_string(),
            CarrierPlacement {
                catalog: "tube_carrier_24".into(),
                rail: 7,
            },
        ),
        (
            "work".to_string(),
            CarrierPlacement {
                catalog: "plate_carrier_l5".into(),
                rail: 9,
            },
        ),
    ])
}

fn default_sources() -> PlacedLabware {
    PlacedLabware {
        site: "sources/1".into(),
        labware: "sample_tubes_24".into(),
        capacity: star_capacity(24),
    }
}

fn default_work() -> PlacedLabware {
    PlacedLabware {
        site: "work/1".into(),
        labware: "pcr_plate_96".into(),
        capacity: star_capacity(96),
    }
}

fn default_small_tips() -> TipRacks {
    TipRacks {
        labware: "tip_rack_50ul_filter".into(),
        slots: vec!["tips/1".into()],
        capacity: star_capacity(96),
    }
}

fn default_large_tips() -> TipRacks {
    TipRacks {
        labware: "tip_rack_1000ul_filter".into(),
        slots: vec!["tips/2".into()],
        capacity: star_capacity(96),
    }
}

/// A literal geometry this compiler ships as a STAR default.
fn star_capacity(capacity: usize) -> PlateCapacity {
    PlateCapacity::new(capacity).expect("built-in STAR defaults declare addressable geometries")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_bench_validates() {
        let profile = StarAdapterProfile::default();
        profile
            .validate()
            .expect("the defaults describe a coherent bench");
    }

    #[test]
    fn a_minimal_profile_parses_with_defaults() {
        let profile = StarAdapterProfile::parse("star-runtime", "")
            .expect("an empty profile takes every checked implementation default");
        assert_eq!(profile.name, "star-runtime");
        assert_eq!(
            profile.machine.variant,
            MachineVariant::Starlet,
            "the reference bench is a STARlet"
        );
        assert_eq!(
            profile.liquid_classes.selected_library.id,
            "org.lab-lang.hamilton.liquid-classes"
        );
    }

    #[test]
    fn every_tip_resource_requires_at_least_one_site() {
        let mut profile = StarAdapterProfile::default();
        profile.resources.small_tips.slots.clear();

        assert_eq!(
            profile.validate(),
            Err(StarProfileError::NoSites {
                context: "resources.small_tips".to_owned(),
            })
        );
    }

    #[test]
    fn profile_validation_rejects_an_unregistered_liquid_class_library() {
        let mut profile = StarAdapterProfile::default();
        profile.liquid_classes.selected_library.version = "99.0.0".to_owned();

        let error = profile
            .validate()
            .expect_err("library selection is checked with the rest of the Asset profile");
        assert!(
            matches!(
                &error,
                StarProfileError::LiquidClasses(source)
                    if matches!(source.as_ref(), LiquidClassError::UnknownSelectedLibrary { .. })
            ),
            "the error retains the registry failure: {error}"
        );
    }

    #[test]
    fn an_embedded_target_or_adapter_selector_is_rejected() {
        let error =
            StarAdapterProfile::parse("star-runtime", "[target]\nbackend = \"opentrons.flex\"\n")
                .expect_err("only the exact Asset binding may select an adapter");
        assert!(error.to_string().contains("target"), "{error}");
    }

    #[test]
    fn scientific_stage_profiles_are_rejected() {
        let error = StarAdapterProfile::parse(
            "star-runtime",
            "[stages.assembly]\nsmall_tips = { labware = \"tip_rack_50ul_filter\", slots = [\"tips/1\"], capacity = 96 }\n",
        )
        .expect_err("the canonical adapter has no assembly-specific profile surface");
        assert!(error.to_string().contains("stages"), "{error}");
    }

    #[test]
    fn a_carrier_off_the_deck_names_the_rail_span() {
        let mut profile = StarAdapterProfile::default();
        profile.deck.carriers.insert(
            "tips".into(),
            CarrierPlacement {
                catalog: "tip_carrier_480".into(),
                rail: 30,
            },
        );
        let error = profile
            .validate()
            .expect_err("a six-rail carrier at rail 30 runs past a 32-rail deck");
        assert!(
            matches!(
                error,
                StarProfileError::RailOutOfRange {
                    end: 35,
                    rails: 32,
                    ..
                }
            ),
            "the error names where the carrier ends: {error}"
        );
    }

    #[test]
    fn overlapping_carriers_are_rejected() {
        let mut profile = StarAdapterProfile::default();
        profile.deck.carriers.insert(
            "rogue".into(),
            CarrierPlacement {
                catalog: "tube_carrier_24".into(),
                rail: 10,
            },
        );
        let error = profile
            .validate()
            .expect_err("rail 10 is inside the work carrier's span");
        assert!(
            matches!(error, StarProfileError::CarrierOverlap { .. }),
            "the error names both carriers: {error}"
        );
    }

    #[test]
    fn two_resources_cannot_share_a_site() {
        let mut profile = StarAdapterProfile::default();
        profile.resources.large_tips.slots = vec!["work/1".into()];
        let error = profile
            .validate()
            .expect_err("work/1 already holds the work plate");
        assert!(
            matches!(error, StarProfileError::SiteConflict { .. }),
            "the error names both claimants: {error}"
        );
    }

    #[test]
    fn a_vessel_where_tips_belong_is_rejected() {
        let mut profile = StarAdapterProfile::default();
        profile.resources.small_tips.labware = "pcr_plate_96".into();
        let error = profile
            .validate()
            .expect_err("a PCR plate cannot feed tips");
        assert!(
            matches!(
                error,
                StarProfileError::CapacityMismatch { .. }
                    | StarProfileError::TipRackExpected { .. }
            ),
            "the error explains the role mismatch: {error}"
        );
    }

    #[test]
    fn labware_capacity_must_match_the_catalog() {
        let mut profile = StarAdapterProfile::default();
        profile.resources.sources.capacity = star_capacity(96);
        let error = profile
            .validate()
            .expect_err("the tube strip holds 24 positions, not 96");
        assert_eq!(
            error,
            StarProfileError::CapacityMismatch {
                context: "resources.sources".into(),
                labware: "sample_tubes_24".into(),
                declared: 96,
                actual: 24,
            },
            "the error carries both numbers"
        );
    }

    #[test]
    fn site_addresses_resolve_to_catalog_geometry() {
        let profile = StarAdapterProfile::default();
        let site = profile
            .resolve_labware("test", "work/1", "pcr_plate_96")
            .expect("the work plate site resolves");
        assert_eq!(site.rail, 9, "work sits on rail 9");
        let a1 = site.well("A1").expect("A1 resolves");
        assert!(
            (a1.x - (100.0 + 8.0 * 22.5 + 4.0 + 9.5)).abs() < 1e-9,
            "A1 sits at the rail-9 carrier's first site"
        );
    }
}
