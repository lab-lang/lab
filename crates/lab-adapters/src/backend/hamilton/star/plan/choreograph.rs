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

use std::collections::BTreeMap;

use hamilton_star::catalog::TipType;

use crate::backend::AdapterConstraintError;
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::catalog::LabwareDefinition;
use crate::backend::hamilton::star::liquid_classes::{
    LiquidClass, LiquidClassError, LiquidClassEvidence, LiquidClassIdentity, LiquidClassLibrary,
    LiquidClassLldMode, LiquidClassQuery,
};
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::{
    ChannelLiquid, StarOperation, StarWell, TipClass, TipPickupPosition,
};
use crate::backend::hamilton::star::plan::liquids::{DeckIndex, LiquidState, wire_mm, wire_ul};
use crate::backend::hamilton::star::profile::LldPolicy;
use crate::backend::resources::{PlateCapacity, plate_wells};

/// One logical transfer the choreographer lowers.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    pub source: StarWell,
    pub target: StarWell,
    /// What the science asks for, µL.
    pub volume_ul: f64,
    /// Mix cycles and volume applied at the target with the same tip.
    pub mix_after: Option<(u32, f64)>,
    /// Open liquid vocabulary consumed only by liquid-class applicability.
    pub liquid: String,
    /// Open physical technique vocabulary consumed only by liquid-class
    /// applicability.
    pub technique: String,
}

impl Transfer {
    pub fn new(source: StarWell, target: StarWell, volume_ul: f64) -> Transfer {
        Transfer {
            source,
            target,
            volume_ul,
            mix_after: None,
            liquid: "aqueous".to_owned(),
            technique: "surface".to_owned(),
        }
    }

    pub fn with_mix(mut self, mix: (u32, f64)) -> Transfer {
        self.mix_after = Some(mix);
        self
    }

    pub fn with_liquid(mut self, liquid: impl Into<String>) -> Transfer {
        self.liquid = liquid.into();
        self
    }

    pub fn with_technique(mut self, technique: impl Into<String>) -> Transfer {
        self.technique = technique.into();
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
    /// Stable STAR catalog identifier used by liquid-class applicability.
    tip_id: &'static str,
    rack_count: usize,
    capacity: PlateCapacity,
    wells: Vec<String>,
    rows: usize,
    cursor: usize,
}

impl TipFeeder {
    pub fn new(
        prefix: &str,
        deck: &DeckIndex,
        rack_count: usize,
        capacity: PlateCapacity,
    ) -> TipFeeder {
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
            tip_id: labware.id,
            rack_count,
            capacity,
            wells: plate_wells(capacity),
            rows,
            cursor: 0,
        }
    }

    /// The stage resource key of the rack holding position `index`.
    fn rack_resource(&self, index: usize) -> String {
        format!("{}/{}", self.prefix, index / self.capacity.get() + 1)
    }

    /// Takes `count` tip positions as groups whose members sit in one rack
    /// column in consecutive rows — the arrangement a multi-channel pickup
    /// needs. Crossing a column or rack boundary starts a new group.
    fn take(&mut self, count: usize) -> Result<Vec<Vec<StarWell>>, StarPlanningError> {
        let total = self.rack_count * self.capacity.get();
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
            let well = self.wells[self.cursor % self.capacity.get()].clone();
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
                let start = rack * self.capacity.get();
                let used = self.cursor.saturating_sub(start).min(self.capacity.get());
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
    classes: &'a LiquidClassLibrary,
    profile_lld: LldPolicy,
    used_classes: BTreeMap<LiquidClassIdentity, LiquidClassEvidence>,
    operations: Vec<StarOperation>,
}

struct ChannelLiquidSpec<'a> {
    channel: usize,
    location: &'a StarWell,
    target_ul: f64,
    corrected_wire: u32,
    heights: crate::backend::hamilton::star::plan::liquids::LiquidHeights,
    mix: Option<(u32, f64)>,
    class: &'a LiquidClass,
}

impl<'a> RunBuilder<'a> {
    pub fn new(
        deck: &'a DeckIndex,
        liquids: &'a mut LiquidState,
        small: Option<TipFeeder>,
        large: Option<TipFeeder>,
        classes: &'a LiquidClassLibrary,
        profile_lld: LldPolicy,
    ) -> RunBuilder<'a> {
        RunBuilder {
            deck,
            liquids,
            small,
            large,
            classes,
            profile_lld,
            used_classes: BTreeMap::new(),
            operations: Vec::new(),
        }
    }

    /// The finished operation list and the feeders (for usage accounting).
    pub fn finish(self) -> (Vec<StarOperation>, Vec<TipFeeder>, Vec<LiquidClassEvidence>) {
        let feeders = [self.small, self.large].into_iter().flatten().collect();
        (
            self.operations,
            feeders,
            self.used_classes.into_values().collect(),
        )
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

    fn tip_id(&self, class: TipClass) -> &'static str {
        match class {
            TipClass::Small => self.small.as_ref().expect("small tips are declared").tip_id,
            TipClass::Large => self.large.as_ref().expect("large tips are declared").tip_id,
        }
    }

    fn select_class(
        &self,
        class: TipClass,
        transfer: &Transfer,
    ) -> Result<LiquidClass, StarPlanningError> {
        let source_labware = self.deck.site(&transfer.source.resource).labware.id;
        let destination_labware = self.deck.site(&transfer.target.resource).labware.id;
        self.classes
            .select(LiquidClassQuery {
                liquid: &transfer.liquid,
                technique: &transfer.technique,
                tip: self.tip_id(class),
                source_labware,
                destination_labware,
                volume_ul: transfer.volume_ul,
            })
            .cloned()
            .map_err(Into::into)
    }

    fn effective_lld(&self, class: &LiquidClass) -> LldPolicy {
        match class.definition().lld.mode {
            LiquidClassLldMode::Off => LldPolicy::Off,
            LiquidClassLldMode::Profile => self.profile_lld,
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

    fn channel_liquid(&mut self, spec: ChannelLiquidSpec<'_>) -> ChannelLiquid {
        let (mix_cycles, mix_volume) = match spec.mix {
            Some((cycles, volume)) => (cycles, wire_ul(volume)),
            None => (0, 0),
        };
        let position = self.deck.position(spec.location);
        self.used_classes
            .entry(spec.class.identity().clone())
            .or_insert_with(|| spec.class.evidence());
        let speeds = &spec.class.definition().speeds;
        let lld = &spec.class.definition().lld;
        ChannelLiquid {
            channel: spec.channel,
            location: spec.location.clone(),
            x: wire_mm(position.x),
            y: wire_mm(position.y),
            position_z: spec.heights.position_z,
            lld_search_z: spec.heights.lld_search_z,
            minimum_z: spec.heights.minimum_z,
            target_ul: spec.target_ul,
            liquid_class: spec.class.identity().clone(),
            corrected_volume: spec.corrected_wire,
            aspirate_speed: wire_ul(speeds.aspirate_ul_s),
            dispense_speed: wire_ul(speeds.dispense_ul_s),
            aspirate_mix_speed: wire_ul(speeds.aspirate_mix_ul_s),
            dispense_mix_speed: wire_ul(speeds.dispense_mix_ul_s),
            lld: self.effective_lld(spec.class),
            gamma_lld_sensitivity: lld.gamma_sensitivity,
            pressure_lld_sensitivity: lld.pressure_sensitivity,
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
        let selected = transfers
            .iter()
            .map(|transfer| {
                self.select_class(class, transfer)
                    .map(|class| (transfer, class))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut chunks: Vec<Vec<usize>> = Vec::new();
        let mut load = 0.0;
        for (index, (transfer, liquid_class)) in selected.iter().enumerate() {
            let corrected = liquid_class.corrected_volume(transfer.volume_ul);
            if corrected > working {
                return Err(AdapterConstraintError::CapacityExceeded {
                    adapter: BACKEND.into(),
                    operation: "liquid_class_correction".into(),
                    subject: liquid_class.identity().id.clone(),
                    resource: "tip".into(),
                    required: wire_ul(corrected).into(),
                    capacity: wire_ul(working).into(),
                    unit: "0.1 uL".into(),
                }
                .into());
            }
            let alone = transfer.mix_after.is_some();
            let same_class = chunks
                .last()
                .and_then(|chunk| chunk.first())
                .is_none_or(|previous| selected[*previous].1.identity() == liquid_class.identity());
            let fits = load + corrected <= working && !alone && same_class;
            match chunks.last_mut() {
                Some(chunk) if fits && !chunk.is_empty() => {
                    chunk.push(index);
                    load += corrected;
                }
                _ => {
                    chunks.push(vec![index]);
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
            let source = &selected[chunk[0]].0.source;
            let liquid_class = &selected[chunk[0]].1;
            let corrected_each: Vec<u32> = chunk
                .iter()
                .map(|index| {
                    let (transfer, class) = &selected[*index];
                    wire_ul(class.corrected_volume(transfer.volume_ul))
                })
                .collect();
            let total_wire: u32 = corrected_each.iter().sum();
            let total_ul: f64 = chunk.iter().map(|index| selected[*index].0.volume_ul).sum();
            let heights = self.liquids.aspirate(
                self.deck,
                source,
                total_ul,
                &liquid_class.definition().margins,
            );
            let aspirate = self.channel_liquid(ChannelLiquidSpec {
                channel,
                location: source,
                target_ul: total_ul,
                corrected_wire: total_wire,
                heights,
                mix: None,
                class: liquid_class,
            });
            self.operations.push(StarOperation::Aspirate {
                tip: class,
                channels: vec![aspirate],
            });
            for (position, index) in chunk.iter().enumerate() {
                let (transfer, liquid_class) = &selected[*index];
                let heights = self.liquids.dispense(
                    self.deck,
                    &transfer.target,
                    transfer.volume_ul,
                    None,
                    &liquid_class.definition().margins,
                );
                let liquid = self.channel_liquid(ChannelLiquidSpec {
                    channel,
                    location: &transfer.target,
                    target_ul: transfer.volume_ul,
                    corrected_wire: corrected_each[position],
                    heights,
                    mix: transfer.mix_after,
                    class: liquid_class,
                });
                self.operations.push(StarOperation::Dispense {
                    tip: class,
                    // Partial jets leave the stop-back in the tip for the
                    // next target; the load's last dispense blows out.
                    mode: if position + 1 == chunk.len() { 1 } else { 0 },
                    channels: vec![liquid],
                });
            }
            self.discard(vec![channel]);
        }
        Ok(())
    }

    /// Carries one continuous fluid path through an ordered series on a single tip.
    ///
    /// Canonical steps that share a `fluid_path_group` must not change tips between them, so this
    /// is not `distribute`: each transfer's target becomes the next transfer's source, and the tip
    /// is discarded only once the series ends.
    pub fn chain(
        &mut self,
        class: TipClass,
        transfers: &[Transfer],
    ) -> Result<(), StarPlanningError> {
        if transfers.is_empty() {
            return Ok(());
        }
        let working = self.working_volume(class);
        let selected = transfers
            .iter()
            .map(|transfer| {
                self.select_class(class, transfer)
                    .map(|class| (transfer, class))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let first_class = selected[0].1.identity();
        if selected
            .iter()
            .any(|(_, class)| class.identity() != first_class)
        {
            return Err(LiquidClassError::Invalid(
                "one continuous fluid path selected more than one liquid class; split the path or supply a class that covers it"
                    .to_owned(),
            )
            .into());
        }
        for (transfer, liquid_class) in &selected {
            let corrected = liquid_class.corrected_volume(transfer.volume_ul);
            if corrected > working {
                return Err(AdapterConstraintError::CapacityExceeded {
                    adapter: BACKEND.into(),
                    operation: "chained_transfer".into(),
                    subject: "automation_batch".into(),
                    resource: "tip".into(),
                    required: wire_ul(corrected).into(),
                    capacity: wire_ul(working).into(),
                    unit: "0.1 uL".into(),
                }
                .into());
            }
        }
        let channels = self.pick_up(class, 1)?;
        let channel = channels[0];
        for (transfer, liquid_class) in &selected {
            let corrected = wire_ul(liquid_class.corrected_volume(transfer.volume_ul));
            let heights = self.liquids.aspirate(
                self.deck,
                &transfer.source,
                transfer.volume_ul,
                &liquid_class.definition().margins,
            );
            let aspirate = self.channel_liquid(ChannelLiquidSpec {
                channel,
                location: &transfer.source,
                target_ul: transfer.volume_ul,
                corrected_wire: corrected,
                heights,
                mix: None,
                class: liquid_class,
            });
            self.operations.push(StarOperation::Aspirate {
                tip: class,
                channels: vec![aspirate],
            });
            let heights = self.liquids.dispense(
                self.deck,
                &transfer.target,
                transfer.volume_ul,
                None,
                &liquid_class.definition().margins,
            );
            let liquid = self.channel_liquid(ChannelLiquidSpec {
                channel,
                location: &transfer.target,
                target_ul: transfer.volume_ul,
                corrected_wire: corrected,
                heights,
                mix: transfer.mix_after,
                class: liquid_class,
            });
            self.operations.push(StarOperation::Dispense {
                tip: class,
                mode: 1,
                channels: vec![liquid],
            });
        }
        self.discard(vec![channel]);
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
            let transfer = Transfer::new(well.clone(), well.clone(), mix.1)
                .with_liquid("aqueous")
                .with_technique("mix");
            let liquid_class = self.select_class(class, &transfer)?;
            let heights = self
                .liquids
                .mix(self.deck, well, &liquid_class.definition().margins);
            let liquid = self.channel_liquid(ChannelLiquidSpec {
                channel: channels[0],
                location: well,
                target_ul: 0.0,
                corrected_wire: 0,
                heights,
                mix: Some(mix),
                class: &liquid_class,
            });
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
    use crate::backend::hamilton::star::liquid_classes::LiquidClassLibrary;
    use crate::backend::hamilton::star::plan::liquids::{DeckIndex, LiquidState};
    use crate::backend::hamilton::star::profile::StarAdapterProfile;

    fn feeder(deck: &DeckIndex) -> TipFeeder {
        TipFeeder::new(
            "assembly_small_tips",
            deck,
            1,
            PlateCapacity::new(96).unwrap(),
        )
    }

    fn contributed_library() -> LiquidClassLibrary {
        LiquidClassLibrary::parse_toml(
            r#"
schema_version = "lab.hamilton-star-liquid-classes.v1"

[[classes]]
id = "org.example.viscous"
version = "3.2.1"

[classes.applicability]
liquids = ["*"]
techniques = ["*"]
tips = ["*"]
source_labware = ["*"]
destination_labware = ["*"]
min_volume_ul = 0.0
max_volume_ul = 60.0

[classes.correction]
points = [
  { target_ul = 0.0, commanded_ul = 0.0 },
  { target_ul = 60.0, commanded_ul = 75.0 },
]

[classes.speeds]
aspirate_ul_s = 42.0
dispense_ul_s = 43.0
aspirate_mix_ul_s = 44.0
dispense_mix_ul_s = 45.0

[classes.lld]
mode = "off"
gamma_sensitivity = 2
pressure_sensitivity = 3

[classes.margins]
aspiration_immersion_mm = 1.0
bottom_standoff_mm = 0.8
dispense_clearance_mm = 3.0
lld_search_clearance_mm = 6.0

[classes.calibration]
source = "example calibration"
source_version = "run-9"
instrument = "STAR-example"
performed_by = "example operator"
observed_at = "2026-09-05"
notes = "A class contributed entirely as data."
"#,
        )
        .expect("the contributed library validates")
    }

    #[test]
    fn choreographer_uses_a_contributed_class_without_rust_dispatch() {
        let profile = StarAdapterProfile::default();
        let deck = DeckIndex::build(&profile).expect("the reference bench resolves");
        let classes = contributed_library();
        let mut liquids = LiquidState::new();
        let source = StarWell::new("assembly_sources", "A1");
        liquids.seed(&source, 100.0);
        let mut builder = RunBuilder::new(
            &deck,
            &mut liquids,
            Some(feeder(&deck)),
            None,
            &classes,
            LldPolicy::Gamma,
        );
        builder
            .distribute(
                TipClass::Small,
                &[Transfer::new(
                    source,
                    StarWell::new("reaction_plate", "A1"),
                    20.0,
                )],
            )
            .expect("the data-defined class lowers");
        let (operations, _, evidence) = builder.finish();
        let channel = operations
            .iter()
            .find_map(|operation| match operation {
                StarOperation::Aspirate { channels, .. } => channels.first(),
                _ => None,
            })
            .expect("the transfer has an aspirate channel");

        assert_eq!(channel.liquid_class.id, "org.example.viscous");
        assert_eq!(channel.corrected_volume, 250);
        assert_eq!(channel.aspirate_speed, 420);
        assert_eq!(channel.lld, LldPolicy::Off);
        assert_eq!(evidence[0].identity, channel.liquid_class);
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
        let classes = LiquidClassLibrary::embedded_v1().expect("built-in classes validate");
        let mut builder = RunBuilder::new(
            &deck,
            &mut liquids,
            Some(feeder(&deck)),
            None,
            &classes,
            profile.run.lld,
        );
        builder
            .distribute(TipClass::Small, &transfers)
            .expect("the distribution lowers");
        let (operations, feeders, evidence) = builder.finish();
        let aspirates = operations
            .iter()
            .filter(|op| matches!(op, StarOperation::Aspirate { .. }))
            .count();
        assert_eq!(
            aspirates, 2,
            "two 46.4 µL piston commands carry four requested 20 µL dispenses"
        );
        assert_eq!(
            feeders[0].usage(),
            vec![("assembly_small_tips/1".to_string(), 2)],
            "one tip per load, no tip reuse across loads"
        );
        assert_eq!(evidence.len(), 1, "one exact class covers this run");
        assert_eq!(evidence[0].identity.content_sha256.len(), 64);
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
        let mut feeder = TipFeeder::new(
            "assembly_small_tips",
            &deck,
            1,
            PlateCapacity::new(96).unwrap(),
        );
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
        let mut feeder = TipFeeder::new(
            "assembly_small_tips",
            &deck,
            1,
            PlateCapacity::new(96).unwrap(),
        );
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
