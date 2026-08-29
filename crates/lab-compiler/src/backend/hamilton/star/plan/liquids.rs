//! Deterministic liquid bookkeeping: the per-well volume ledger and the
//! height derivation every command's Z parameters come from.
//!
//! Planning is a pure function of program and profile: every height is
//! computed from the catalog geometry and the tracked volumes, never
//! detected at runtime. The named margins below are the whole safety
//! policy, stated once:
//!
//! - aspiration immerses 2 mm below the tracked surface, clamped to a
//!   0.5 mm standoff above the vessel bottom, so a mistracked surface digs
//!   at the floor rather than into the vessel;
//! - dispensing jets from 2 mm above the post-dispense surface;
//! - agar spotting dispenses at a fixed 6 mm above the well bottom
//!   (≈4 mm of agar fill plus the jet clearance), never tracking volume;
//! - the LLD search window opens 5 mm above the surface, matching the
//!   driver crate's clearance constant, so a bench that opts into gamma
//!   detection searches where planning says the liquid is.

use std::collections::BTreeMap;

use crate::backend::hamilton::star::catalog::{DeckPosition, HeightModel};
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::StarWell;
use crate::backend::hamilton::star::profile::{ResolvedSite, StarAdapterProfile, StarProfileError};

/// Aspiration depth below the tracked liquid surface, mm.
pub const IMMERSION_DEPTH_MM: f64 = 2.0;
/// The floor above the vessel bottom no tip goes below, mm.
pub const BOTTOM_STANDOFF_MM: f64 = 0.5;
/// The LLD search window above the tracked surface, mm.
pub const LLD_CLEARANCE_MM: f64 = 5.0;
/// Jet dispense clearance above the post-dispense surface, mm.
pub const DISPENSE_CLEARANCE_MM: f64 = 2.0;
/// Dead volume the operator loads beyond consumption: sample tubes, µL.
pub const TUBE_DEAD_VOLUME_UL: f64 = 50.0;
/// Dead volume for troughs, µL.
pub const TROUGH_DEAD_VOLUME_UL: f64 = 2000.0;
/// Dead volume for operator-loaded plate wells (the DNA plate), µL.
pub const PLATE_DEAD_VOLUME_UL: f64 = 5.0;

/// Converts millimeters to the firmware's 0.1 mm wire unit.
pub fn wire_mm(mm: f64) -> u32 {
    (mm * 10.0).round().max(0.0) as u32
}

/// Converts microliters to the firmware's 0.1 µL wire unit.
pub fn wire_ul(ul: f64) -> u32 {
    (ul * 10.0).round().max(0.0) as u32
}

/// Every plan resource resolved against the catalog, keyed by the stable
/// resource strings the execution plan uses.
pub struct DeckIndex {
    resources: BTreeMap<String, ResolvedSite>,
}

impl DeckIndex {
    /// Resolves every deck and stage resource of a validated profile.
    pub fn build(profile: &StarAdapterProfile) -> Result<DeckIndex, StarPlanningError> {
        let mut resources = BTreeMap::new();
        let mut place = |key: String, site: Result<ResolvedSite, StarProfileError>| {
            site.map(|site| {
                resources.insert(key, site);
            })
        };
        // The one physical source rack serves both stages, reloaded by the
        // operator between runs, so each stage gets its own ledger resource
        // over the same site: assembly's water in A1 and transformation's
        // cells in A1 are different liquids at different times.
        for key in ["assembly_sources", "transformation_sources"] {
            place(
                key.into(),
                profile.resolve_labware(
                    "deck.source_rack",
                    &profile.deck.source_rack.site,
                    &profile.deck.source_rack.labware,
                ),
            )?;
        }
        place(
            "reaction_plate".into(),
            profile.resolve_labware(
                "deck.reaction_plate",
                &profile.deck.reaction_plate.site,
                &profile.deck.reaction_plate.labware,
            ),
        )?;
        place(
            "media_rack".into(),
            profile.resolve_labware(
                "stages.plating.media_rack",
                &profile.stages.plating.media_rack.slot,
                &profile.stages.plating.media_rack.labware,
            ),
        )?;
        for (prefix, plates) in [
            ("dna_plate", &profile.stages.transformation.dna_plate),
            ("dilution_plate", &profile.stages.plating.dilution_plate),
            ("agar_plate", &profile.stages.plating.agar_plate),
        ] {
            for (index, slot) in plates.slots.iter().enumerate() {
                place(
                    format!("{prefix}/{}", index + 1),
                    profile.resolve_labware(prefix, slot, &plates.labware),
                )?;
            }
        }
        for (prefix, racks) in [
            ("assembly_small_tips", &profile.stages.assembly.small_tips),
            (
                "transformation_small_tips",
                &profile.stages.transformation.small_tips,
            ),
            (
                "transformation_large_tips",
                &profile.stages.transformation.large_tips,
            ),
            ("plating_small_tips", &profile.stages.plating.small_tips),
            ("plating_large_tips", &profile.stages.plating.large_tips),
        ] {
            for (index, slot) in racks.slots.iter().enumerate() {
                place(
                    format!("{prefix}/{}", index + 1),
                    profile.resolve_labware(prefix, slot, &racks.labware),
                )?;
            }
        }
        Ok(DeckIndex { resources })
    }

    /// The resolved site behind a resource key.
    pub fn site(&self, resource: &str) -> &ResolvedSite {
        self.resources
            .get(resource)
            .expect("every plan resource key was resolved when the index was built")
    }

    /// The deck position of a well on a resource.
    pub fn position(&self, well: &StarWell) -> DeckPosition {
        self.site(&well.resource)
            .well(&well.well)
            .expect("planning addresses only wells its allocators handed out")
    }

    /// The vessel geometry behind a resource key: `(bottom-relative depth,
    /// working volume, height model)`.
    pub fn vessel(&self, resource: &str) -> (f64, f64, HeightModel) {
        let (_, depth, working, model) = self
            .site(resource)
            .labware
            .vessel()
            .expect("liquid operations target only vessel labware");
        (depth, working, model)
    }
}

/// The heights one channel operation uses, in wire units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidHeights {
    /// The liquid position `zl`, 0.1 mm.
    pub position_z: u32,
    /// The LLD search height `lp`, 0.1 mm.
    pub lld_search_z: u32,
    /// The minimum height `zx`, 0.1 mm.
    pub minimum_z: u32,
}

/// The per-well volume ledger. Sources are seeded with their planned fills;
/// destination wells accumulate what the runs deliver.
#[derive(Clone, Debug, Default)]
pub struct LiquidState {
    volumes: BTreeMap<(String, String), f64>,
    /// Total drawn per well across the whole program, for computing the
    /// fills the operator loads.
    drawn: BTreeMap<(String, String), f64>,
}

impl LiquidState {
    pub fn new() -> LiquidState {
        LiquidState::default()
    }

    fn key(well: &StarWell) -> (String, String) {
        (well.resource.clone(), well.well.clone())
    }

    /// Seeds a well with a starting volume (the planned fill).
    pub fn seed(&mut self, well: &StarWell, volume_ul: f64) {
        self.volumes.insert(Self::key(well), volume_ul);
    }

    pub fn volume(&self, well: &StarWell) -> f64 {
        self.volumes.get(&Self::key(well)).copied().unwrap_or(0.0)
    }

    /// Everything drawn so far, keyed by `(resource, well)`.
    pub fn drawn(&self) -> &BTreeMap<(String, String), f64> {
        &self.drawn
    }

    /// The heights for aspirating `volume_ul` from a well, computed at the
    /// current surface, then the ledger debit.
    pub fn aspirate(&mut self, deck: &DeckIndex, well: &StarWell, volume_ul: f64) -> LiquidHeights {
        let position = deck.position(well);
        let (_, _, model) = deck.vessel(&well.resource);
        let surface = position.z + model.height_at(self.volume(well));
        let floor = position.z + BOTTOM_STANDOFF_MM;
        let heights = LiquidHeights {
            position_z: wire_mm((surface - IMMERSION_DEPTH_MM).max(floor)),
            lld_search_z: wire_mm(surface + LLD_CLEARANCE_MM),
            minimum_z: wire_mm(floor),
        };
        *self.volumes.entry(Self::key(well)).or_insert(0.0) -= volume_ul;
        *self.drawn.entry(Self::key(well)).or_insert(0.0) += volume_ul;
        heights
    }

    /// The heights for jet-dispensing `volume_ul` into a well: the ledger
    /// credit happens first so the jet clears the post-dispense surface. A
    /// fixed height (agar spotting) bypasses the tracked surface.
    pub fn dispense(
        &mut self,
        deck: &DeckIndex,
        well: &StarWell,
        volume_ul: f64,
        fixed_height_mm: Option<f64>,
    ) -> LiquidHeights {
        let position = deck.position(well);
        let (_, _, model) = deck.vessel(&well.resource);
        *self.volumes.entry(Self::key(well)).or_insert(0.0) += volume_ul;
        let height_above_bottom = match fixed_height_mm {
            Some(fixed) => fixed,
            None => model.height_at(self.volume(well)) + DISPENSE_CLEARANCE_MM,
        };
        let floor = position.z + BOTTOM_STANDOFF_MM;
        LiquidHeights {
            position_z: wire_mm((position.z + height_above_bottom).max(floor)),
            lld_search_z: wire_mm(position.z + height_above_bottom + LLD_CLEARANCE_MM),
            minimum_z: wire_mm(floor),
        }
    }

    /// The heights for mixing in place at the current surface.
    pub fn mix(&mut self, deck: &DeckIndex, well: &StarWell) -> LiquidHeights {
        let position = deck.position(well);
        let (_, _, model) = deck.vessel(&well.resource);
        let surface = position.z + model.height_at(self.volume(well));
        let floor = position.z + BOTTOM_STANDOFF_MM;
        LiquidHeights {
            position_z: wire_mm((surface - IMMERSION_DEPTH_MM).max(floor)),
            lld_search_z: wire_mm(surface + LLD_CLEARANCE_MM),
            minimum_z: wire_mm(floor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::hamilton::star::profile::StarAdapterProfile;

    fn deck() -> DeckIndex {
        DeckIndex::build(&StarAdapterProfile::default()).expect("the reference bench resolves")
    }

    #[test]
    fn a_source_surface_drops_across_successive_aspirates() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let tube = StarWell::new("assembly_sources", "A1");
        liquids.seed(&tube, 2000.0);
        let first = liquids.aspirate(&deck, &tube, 500.0);
        let second = liquids.aspirate(&deck, &tube, 500.0);
        assert!(
            second.position_z < first.position_z,
            "drawing 500 µL lowers the next aspirate: {} then {}",
            first.position_z,
            second.position_z
        );
    }

    #[test]
    fn an_empty_well_clamps_to_the_bottom_standoff() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let tube = StarWell::new("assembly_sources", "A1");
        let heights = liquids.aspirate(&deck, &tube, 10.0);
        assert_eq!(
            heights.position_z, heights.minimum_z,
            "with no tracked liquid the tip sits at the 0.5 mm floor, never below"
        );
    }

    #[test]
    fn dispensing_credits_the_well_before_placing_the_jet() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let well = StarWell::new("reaction_plate", "A1");
        let heights = liquids.dispense(&deck, &well, 20.0, None);
        let position = deck.position(&well);
        let (_, _, model) = deck.vessel("reaction_plate");
        let expected = wire_mm(position.z + model.height_at(20.0) + DISPENSE_CLEARANCE_MM);
        assert_eq!(
            heights.position_z, expected,
            "the jet clears the surface the dispense itself creates"
        );
        assert_eq!(
            liquids.volume(&well),
            20.0,
            "the ledger credited the dispense"
        );
    }

    #[test]
    fn agar_spots_use_the_fixed_height_not_the_ledger() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let well = StarWell::new("agar_plate/1", "A1");
        let fixed_height_mm = 6.0;
        let heights = liquids.dispense(&deck, &well, 4.0, Some(fixed_height_mm));
        let position = deck.position(&well);
        assert_eq!(
            heights.position_z,
            wire_mm(position.z + fixed_height_mm),
            "spotting height is the documented 6 mm above the well bottom"
        );
    }
}
