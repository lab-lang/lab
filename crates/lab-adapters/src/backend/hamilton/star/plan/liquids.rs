//! Deterministic liquid bookkeeping: the per-well volume ledger and the
//! height derivation every command's Z parameters come from.
//!
//! Planning is a pure function of program and profile: every height is
//! computed from the catalog geometry, tracked volumes, and selected class,
//! never invented at runtime. The class margins are the whole vertical
//! safety policy:
//!
//! - aspiration immersion and bottom standoff come from the selected liquid
//!   class, so a mistracked surface clamps at an explicit calibrated floor;
//! - dispense and LLD clearances likewise come from that exact class;
//! - a bench that opts into gamma detection searches where the selected
//!   class says it should.

use std::collections::BTreeMap;

use crate::backend::hamilton::star::catalog::{DeckPosition, HeightModel};
use crate::backend::hamilton::star::liquid_classes::PipettingMargins;
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::StarWell;
use crate::backend::hamilton::star::profile::{ResolvedSite, StarAdapterProfile, StarProfileError};

/// Dead volume the operator loads beyond consumption in source tubes, µL.
pub const TUBE_DEAD_VOLUME_UL: f64 = 50.0;
/// Dead volume for operator-loaded work-plate wells, µL.
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
    /// Resolves every resource of a validated profile.
    pub fn build(profile: &StarAdapterProfile) -> Result<DeckIndex, StarPlanningError> {
        let mut resources = BTreeMap::new();
        let mut place = |key: String, site: Result<ResolvedSite, StarProfileError>| {
            site.map(|site| {
                resources.insert(key, site);
            })
        };
        place(
            "sources".into(),
            profile.resolve_labware(
                "resources.sources",
                &profile.resources.sources.site,
                &profile.resources.sources.labware,
            ),
        )?;
        place(
            "work".into(),
            profile.resolve_labware(
                "resources.work",
                &profile.resources.work.site,
                &profile.resources.work.labware,
            ),
        )?;
        for (prefix, racks) in [
            ("small_tips", &profile.resources.small_tips),
            ("large_tips", &profile.resources.large_tips),
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
    pub fn aspirate(
        &mut self,
        deck: &DeckIndex,
        well: &StarWell,
        volume_ul: f64,
        margins: &PipettingMargins,
    ) -> LiquidHeights {
        let position = deck.position(well);
        let (_, _, model) = deck.vessel(&well.resource);
        let surface = position.z + model.height_at(self.volume(well));
        let floor = position.z + margins.bottom_standoff_mm;
        let heights = LiquidHeights {
            position_z: wire_mm((surface - margins.aspiration_immersion_mm).max(floor)),
            lld_search_z: wire_mm(surface + margins.lld_search_clearance_mm),
            minimum_z: wire_mm(floor),
        };
        *self.volumes.entry(Self::key(well)).or_insert(0.0) -= volume_ul;
        *self.drawn.entry(Self::key(well)).or_insert(0.0) += volume_ul;
        heights
    }

    /// The heights for jet-dispensing `volume_ul` into a well: the ledger
    /// credit happens first so the jet clears the post-dispense surface. A
    /// an explicitly requested fixed height bypasses the tracked surface.
    pub fn dispense(
        &mut self,
        deck: &DeckIndex,
        well: &StarWell,
        volume_ul: f64,
        fixed_height_mm: Option<f64>,
        margins: &PipettingMargins,
    ) -> LiquidHeights {
        let position = deck.position(well);
        let (_, _, model) = deck.vessel(&well.resource);
        *self.volumes.entry(Self::key(well)).or_insert(0.0) += volume_ul;
        let height_above_bottom = match fixed_height_mm {
            Some(fixed) => fixed,
            None => model.height_at(self.volume(well)) + margins.dispense_clearance_mm,
        };
        let floor = position.z + margins.bottom_standoff_mm;
        LiquidHeights {
            position_z: wire_mm((position.z + height_above_bottom).max(floor)),
            lld_search_z: wire_mm(
                position.z + height_above_bottom + margins.lld_search_clearance_mm,
            ),
            minimum_z: wire_mm(floor),
        }
    }

    /// The heights for mixing in place at the current surface.
    pub fn mix(
        &mut self,
        deck: &DeckIndex,
        well: &StarWell,
        margins: &PipettingMargins,
    ) -> LiquidHeights {
        let position = deck.position(well);
        let (_, _, model) = deck.vessel(&well.resource);
        let surface = position.z + model.height_at(self.volume(well));
        let floor = position.z + margins.bottom_standoff_mm;
        LiquidHeights {
            position_z: wire_mm((surface - margins.aspiration_immersion_mm).max(floor)),
            lld_search_z: wire_mm(surface + margins.lld_search_clearance_mm),
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

    fn margins() -> PipettingMargins {
        PipettingMargins {
            aspiration_immersion_mm: 2.0,
            bottom_standoff_mm: 0.5,
            dispense_clearance_mm: 2.0,
            lld_search_clearance_mm: 5.0,
        }
    }

    #[test]
    fn a_source_surface_drops_across_successive_aspirates() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let tube = StarWell::new("sources", "A1");
        liquids.seed(&tube, 2000.0);
        let first = liquids.aspirate(&deck, &tube, 500.0, &margins());
        let second = liquids.aspirate(&deck, &tube, 500.0, &margins());
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
        let tube = StarWell::new("sources", "A1");
        let heights = liquids.aspirate(&deck, &tube, 10.0, &margins());
        assert_eq!(
            heights.position_z, heights.minimum_z,
            "with no tracked liquid the tip sits at the 0.5 mm floor, never below"
        );
    }

    #[test]
    fn dispensing_credits_the_well_before_placing_the_jet() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let well = StarWell::new("work", "A1");
        let heights = liquids.dispense(&deck, &well, 20.0, None, &margins());
        let position = deck.position(&well);
        let (_, _, model) = deck.vessel("work");
        let expected =
            wire_mm(position.z + model.height_at(20.0) + margins().dispense_clearance_mm);
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
    fn explicit_dispense_offsets_use_a_fixed_height_not_the_ledger() {
        let deck = deck();
        let mut liquids = LiquidState::new();
        let well = StarWell::new("work", "A1");
        let fixed_height_mm = 6.0;
        let heights = liquids.dispense(&deck, &well, 4.0, Some(fixed_height_mm), &margins());
        let position = deck.position(&well);
        assert_eq!(
            heights.position_z,
            wire_mm(position.z + fixed_height_mm),
            "the explicit height is measured above the well bottom"
        );
    }
}
