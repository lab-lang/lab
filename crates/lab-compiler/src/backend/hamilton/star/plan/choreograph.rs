//! Lowering invocation-local logical transfers into channel operations.
//!
//! Batching rules, stated once:
//! - a tube or trough source admits one channel at a time, so shared-liquid
//!   distribution runs single-channel with multi-dispense: one aspirate
//!   carries as many targets as fit the tip's working volume;
//! - a transfer that mixes afterwards keeps its tip to itself: mixing with
//!   a shared multi-dispense tip would carry liquid between targets;
//! - the final dispense of a tip's load jets in blow-out mode (`dm1`); the
//!   dispenses before it use partial jets (`dm0`);
//! - tips are never reused across different source liquids.

use hamilton_star::catalog::{CorrectionCurve, TipType};

use crate::backend::AdapterConstraintError;
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::catalog::LabwareDefinition;
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::{
    ChannelLiquid, StarOperation, StarWell, TipClass, TipPickupPosition,
};
use crate::backend::hamilton::star::plan::liquids::{DeckIndex, LiquidState, wire_mm, wire_ul};
use crate::backend::resources::plate_wells;

/// One logical transfer the choreographer lowers.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    pub source: StarWell,
    pub target: StarWell,
    /// What the science asks for, µL.
    pub volume_ul: f64,
    /// Mix cycles and volume applied at the target with the same tip.
    pub mix_after: Option<(u32, f64)>,
}

impl Transfer {
    pub fn new(source: StarWell, target: StarWell, volume_ul: f64) -> Transfer {
        Transfer {
            source,
            target,
            volume_ul,
            mix_after: None,
        }
    }

    pub fn with_mix(mut self, mix: (u32, f64)) -> Transfer {
        self.mix_after = Some(mix);
        self
    }
}

/// Hands out tip positions across one stage's racks in column-major order,
/// counting consumption per rack resource.
pub struct TipFeeder {
    /// The stage resource prefix, e.g. `assembly_small_tips`.
    prefix: String,
    /// The tip the racks feed.
    pub tip: TipType,
    rack_count: usize,
    capacity: usize,
    wells: Vec<String>,
    rows: usize,
    cursor: usize,
}

impl TipFeeder {
    pub fn new(prefix: &str, deck: &DeckIndex, rack_count: usize, capacity: usize) -> TipFeeder {
        let labware: &LabwareDefinition = deck.site(&format!("{prefix}/1")).labware;
        let tip = labware
            .tip()
            .expect("profile validation accepted only tip racks for tip resources");
        let rows = match labware.layout {
            crate::backend::hamilton::star::catalog::LabwareLayout::Grid { rows, .. } => rows,
            _ => 8,
        };
        TipFeeder {
            prefix: prefix.to_string(),
            tip,
            rack_count,
            capacity,
            wells: plate_wells(capacity),
            rows,
            cursor: 0,
        }
    }

    /// The stage resource key of the rack holding position `index`.
    fn rack_resource(&self, index: usize) -> String {
        format!("{}/{}", self.prefix, index / self.capacity + 1)
    }

    /// Takes `count` tip positions as groups whose members sit in one rack
    /// column in consecutive rows — the arrangement a multi-channel pickup
    /// needs. Crossing a column or rack boundary starts a new group.
    fn take(&mut self, count: usize) -> Result<Vec<Vec<StarWell>>, StarPlanningError> {
        let total = self.rack_count * self.capacity;
        if self.cursor + count > total {
            return Err(AdapterConstraintError::CapacityExceeded {
                adapter: BACKEND.into(),
                operation: "tip_pickup".into(),
                subject: "automation_batch".into(),
                resource: self.prefix.clone(),
                required: (self.cursor + count) as u64,
                capacity: total as u64,
                unit: "tips".into(),
            }
            .into());
        }
        let mut groups: Vec<Vec<StarWell>> = Vec::new();
        for _ in 0..count {
            let rack = self.rack_resource(self.cursor);
            let well = self.wells[self.cursor % self.capacity].clone();
            let starts_group = match groups.last().and_then(|group| group.last()) {
                Some(previous) => {
                    previous.resource != rack || self.cursor.is_multiple_of(self.rows)
                }
                None => true,
            };
            if starts_group {
                groups.push(Vec::new());
            }
            groups
                .last_mut()
                .expect("a group was started for this position")
                .push(StarWell::new(rack, well));
            self.cursor += 1;
        }
        Ok(groups)
    }

    /// Tips consumed so far, per rack resource key.
    pub fn usage(&self) -> Vec<(String, usize)> {
        (0..self.rack_count)
            .map(|rack| {
                let start = rack * self.capacity;
                let used = self.cursor.saturating_sub(start).min(self.capacity);
                (format!("{}/{}", self.prefix, rack + 1), used)
            })
            .collect()
    }
}

/// Builds one run's operation list.
pub struct RunBuilder<'a> {
    deck: &'a DeckIndex,
    liquids: &'a mut LiquidState,
    small: Option<TipFeeder>,
    large: Option<TipFeeder>,
    curves: Curves,
    operations: Vec<StarOperation>,
}

/// The vendored water correction curves, chosen per tip class: the
/// standard-volume filter surface table covers the small tip's 0.5–300 µL
/// range densely; the high-volume filter jet table covers the large tip's.
pub struct Curves {
    small: CorrectionCurve,
    large: CorrectionCurve,
}

impl Default for Curves {
    fn default() -> Curves {
        Curves {
            small: hamilton_star::catalog::water_standard_volume_filter_surface(),
            large: hamilton_star::catalog::water_high_volume_filter_jet(),
        }
    }
}

impl Curves {
    fn corrected(&self, class: TipClass, target_ul: f64) -> f64 {
        match class {
            TipClass::Small => self.small.corrected_volume(target_ul),
            TipClass::Large => self.large.corrected_volume(target_ul),
        }
    }
}

impl<'a> RunBuilder<'a> {
    pub fn new(
        deck: &'a DeckIndex,
        liquids: &'a mut LiquidState,
        small: Option<TipFeeder>,
        large: Option<TipFeeder>,
    ) -> RunBuilder<'a> {
        RunBuilder {
            deck,
            liquids,
            small,
            large,
            curves: Curves::default(),
            operations: Vec::new(),
        }
    }

    /// The finished operation list and the feeders (for usage accounting).
    pub fn finish(self) -> (Vec<StarOperation>, Vec<TipFeeder>) {
        let feeders = [self.small, self.large].into_iter().flatten().collect();
        (self.operations, feeders)
    }

    fn feeder(&mut self, class: TipClass) -> &mut TipFeeder {
        match class {
            TipClass::Small => self
                .small
                .as_mut()
                .expect("the run declared a small tip rack before drawing small tips"),
            TipClass::Large => self
                .large
                .as_mut()
                .expect("the run declared a large tip rack before drawing large tips"),
        }
    }

    fn tip(&self, class: TipClass) -> TipType {
        match class {
            TipClass::Small => self.small.as_ref().expect("small tips are declared").tip,
            TipClass::Large => self.large.as_ref().expect("large tips are declared").tip,
        }
    }

    /// The working volume one tip load may carry, µL, after correction.
    fn working_volume(&self, class: TipClass) -> f64 {
        self.tip(class).max_volume
    }

    /// Picks up `count` tips onto channels `0..count`, splitting the
    /// pickup where the rack column breaks alignment. Returns the channel
    /// indices.
    fn pick_up(&mut self, class: TipClass, count: usize) -> Result<Vec<usize>, StarPlanningError> {
        let tip = self.tip(class);
        let groups = self.feeder(class).take(count)?;
        let mut channel = 0usize;
        for group in groups {
            let first = self
                .deck
                .site(&group[0].resource)
                .well(&group[0].well)
                .expect("tip positions come from the rack's own well list");
            // The driver-crate pickup window: begin at the spot plane plus
            // the tip's total length (with the empirical size-class
            // correction), press down by the length that stays proud of
            // the cone.
            let fitting = hamilton_star::catalog::fitting_depth(tip.size).0;
            let begin =
                ((first.z + tip.total_length.0) * 10.0).round() as i32 + tip.pickup_z_correction();
            let travel = ((tip.total_length.0 - fitting) * 10.0).round() as i32;
            let positions = group
                .iter()
                .map(|location| {
                    let position = self
                        .deck
                        .site(&location.resource)
                        .well(&location.well)
                        .expect("tip positions come from the rack's own well list");
                    let pickup = TipPickupPosition {
                        channel,
                        location: location.clone(),
                        x: wire_mm(position.x),
                        y: wire_mm(position.y),
                    };
                    channel += 1;
                    pickup
                })
                .collect();
            self.operations.push(StarOperation::PickUpTips {
                tip: class,
                begin_z: begin.max(0) as u32,
                end_z: (begin - travel).max(0) as u32,
                positions,
            });
        }
        Ok((0..count).collect())
    }

    fn discard(&mut self, channels: Vec<usize>) {
        self.operations
            .push(StarOperation::DiscardTips { channels });
    }

    fn channel_liquid(
        &mut self,
        channel: usize,
        location: &StarWell,
        target_ul: f64,
        corrected_wire: u32,
        heights: crate::backend::hamilton::star::plan::liquids::LiquidHeights,
        mix: Option<(u32, f64)>,
    ) -> ChannelLiquid {
        let (mix_cycles, mix_volume) = match mix {
            Some((cycles, volume)) => (cycles, wire_ul(volume)),
            None => (0, 0),
        };
        let position = self.deck.position(location);
        ChannelLiquid {
            channel,
            location: location.clone(),
            x: wire_mm(position.x),
            y: wire_mm(position.y),
            position_z: heights.position_z,
            lld_search_z: heights.lld_search_z,
            minimum_z: heights.minimum_z,
            target_ul,
            corrected_volume: corrected_wire,
            mix_volume,
            mix_cycles,
        }
    }

    /// Distributes from one source well to many targets with one channel:
    /// a fresh tip, targets chunked so each aspirate's corrected load fits
    /// the tip, blow-out on each chunk's last dispense, and the tip
    /// discarded afterwards. A transfer that mixes gets a chunk (and so a
    /// tip load) of its own.
    pub fn distribute(
        &mut self,
        class: TipClass,
        transfers: &[Transfer],
    ) -> Result<(), StarPlanningError> {
        if transfers.is_empty() {
            return Ok(());
        }
        let working = self.working_volume(class);
        let mut chunks: Vec<Vec<&Transfer>> = Vec::new();
        let mut load = 0.0;
        for transfer in transfers {
            let corrected = self.curves.corrected(class, transfer.volume_ul);
            let alone = transfer.mix_after.is_some();
            let fits = load + corrected <= working && !alone;
            match chunks.last_mut() {
                Some(chunk) if fits && !chunk.is_empty() => {
                    chunk.push(transfer);
                    load += corrected;
                }
                _ => {
                    chunks.push(vec![transfer]);
                    load = corrected;
                }
            }
            if alone {
                // Force the next transfer into a new chunk.
                load = working + 1.0;
            }
        }

        for chunk in chunks {
            let channels = self.pick_up(class, 1)?;
            let channel = channels[0];
            let source = &chunk[0].source;
            let corrected_each: Vec<u32> = chunk
                .iter()
                .map(|transfer| wire_ul(self.curves.corrected(class, transfer.volume_ul)))
                .collect();
            let total_wire: u32 = corrected_each.iter().sum();
            let total_ul = f64::from(total_wire) / 10.0;
            let heights = self.liquids.aspirate(self.deck, source, total_ul);
            let aspirate = self.channel_liquid(
                channel,
                &source.clone(),
                total_ul,
                total_wire,
                heights,
                None,
            );
            self.operations.push(StarOperation::Aspirate {
                tip: class,
                channels: vec![aspirate],
            });
            for (index, transfer) in chunk.iter().enumerate() {
                let heights =
                    self.liquids
                        .dispense(self.deck, &transfer.target, transfer.volume_ul, None);
                let liquid = self.channel_liquid(
                    channel,
                    &transfer.target,
                    transfer.volume_ul,
                    corrected_each[index],
                    heights,
                    transfer.mix_after,
                );
                self.operations.push(StarOperation::Dispense {
                    tip: class,
                    // Partial jets leave the stop-back in the tip for the
                    // next target; the load's last dispense blows out.
                    mode: if index + 1 == chunk.len() { 1 } else { 0 },
                    channels: vec![liquid],
                });
            }
            self.discard(vec![channel]);
        }
        Ok(())
    }

    /// Mixes each well in place with a fresh tip: an aspirate of zero
    /// volume carrying the mix cycles.
    pub fn mix_wells(
        &mut self,
        class: TipClass,
        wells: &[StarWell],
        mix: (u32, f64),
    ) -> Result<(), StarPlanningError> {
        for well in wells {
            let channels = self.pick_up(class, 1)?;
            let heights = self.liquids.mix(self.deck, well);
            let liquid = self.channel_liquid(channels[0], well, 0.0, 0, heights, Some(mix));
            self.operations.push(StarOperation::Aspirate {
                tip: class,
                channels: vec![liquid],
            });
            self.discard(channels);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::hamilton::star::plan::liquids::{DeckIndex, LiquidState};
    use crate::backend::hamilton::star::profile::StarAdapterProfile;

    fn feeder(deck: &DeckIndex) -> TipFeeder {
        TipFeeder::new("assembly_small_tips", deck, 1, 96)
    }

    #[test]
    fn multi_dispense_chunks_split_at_the_tip_working_volume() {
        let profile = StarAdapterProfile::default();
        let deck = DeckIndex::build(&profile).expect("the reference bench resolves");
        let mut liquids = LiquidState::new();
        // 4 × 20 µL of water correct to ~4 × 23.2 µL; a 60 µL working
        // volume fits two per load, so four targets need two tip loads.
        let source = StarWell::new("assembly_sources", "A1");
        let transfers: Vec<Transfer> = (0..4)
            .map(|row| {
                Transfer::new(
                    source.clone(),
                    StarWell::new("reaction_plate", format!("{}1", char::from(b'A' + row))),
                    20.0,
                )
            })
            .collect();
        let mut builder = RunBuilder::new(&deck, &mut liquids, Some(feeder(&deck)), None);
        builder
            .distribute(TipClass::Small, &transfers)
            .expect("the distribution lowers");
        let (operations, feeders) = builder.finish();
        let aspirates = operations
            .iter()
            .filter(|op| matches!(op, StarOperation::Aspirate { .. }))
            .count();
        assert_eq!(
            aspirates, 2,
            "two 46.4 µL loads carry four 23.2 µL dispenses"
        );
        assert_eq!(
            feeders[0].usage(),
            vec![("assembly_small_tips/1".to_string(), 2)],
            "one tip per load, no tip reuse across loads"
        );
        let modes: Vec<u32> = operations
            .iter()
            .filter_map(|op| match op {
                StarOperation::Dispense { mode, .. } => Some(*mode),
                _ => None,
            })
            .collect();
        assert_eq!(
            modes,
            vec![0, 1, 0, 1],
            "each load's last dispense blows out; the ones before jet partially"
        );
    }

    #[test]
    fn tip_exhaustion_names_the_rack_resource() {
        let profile = StarAdapterProfile::default();
        let deck = DeckIndex::build(&profile).expect("the reference bench resolves");
        let mut feeder = TipFeeder::new("assembly_small_tips", &deck, 1, 96);
        feeder.take(96).expect("the rack holds 96 tips");
        let error = feeder.take(1).expect_err("the 97th tip does not exist");
        let message = error.to_string();
        assert!(
            message.contains("assembly_small_tips") && message.contains("97"),
            "the error names the rack and the requirement: {message}"
        );
    }

    #[test]
    fn multi_channel_pickups_split_at_rack_column_boundaries() {
        let profile = StarAdapterProfile::default();
        let deck = DeckIndex::build(&profile).expect("the reference bench resolves");
        let mut feeder = TipFeeder::new("assembly_small_tips", &deck, 1, 96);
        feeder.take(6).expect("six tips leave two in the column");
        let groups = feeder.take(4).expect("four more tips exist");
        assert_eq!(
            groups.len(),
            2,
            "two tips finish column 1 and two start column 2 — a channel batch cannot span the gap"
        );
        assert_eq!(groups[0].len(), 2, "G1 and H1 finish the column");
        assert_eq!(
            groups[1][0].well, "A2",
            "the next group starts the new column"
        );
    }
}
