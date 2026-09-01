//! The resource-allocated STAR execution plan: the science each artifact
//! carries, the deck and source allocation, and the lowered per-run
//! operation sequences whose numbers are already in firmware wire units.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::backend::hamilton::star::profile::StarAdapterProfile;

/// A well on a named plan resource. Resource keys are stable strings the
/// deck summary and emitters share: `source_rack`, `reaction_plate`,
/// `media_rack`, and indexed stage plates like `dna_plate/1`; tip racks are
/// `assembly_small_tips/1` and siblings.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct StarWell {
    pub resource: String,
    pub well: String,
}

impl StarWell {
    pub fn new(resource: impl Into<String>, well: impl Into<String>) -> StarWell {
        StarWell {
            resource: resource.into(),
            well: well.into(),
        }
    }
}

/// Every well, source position, and replicate the machine will touch,
/// allocated once and shared by every emitted artifact, plus the lowered
/// run sequences.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarExecutionPlan {
    pub schema_version: String,
    /// The explicit adapter implementation that produced this device plan.
    pub adapter: String,
    /// Checked implementation configuration for the allocated Asset binding.
    pub deck: StarAdapterProfile,
    /// Source-rack well for each assembly-stage reagent, DNA, and enzyme
    /// key.
    pub assembly_source_wells: BTreeMap<String, String>,
    /// Source-rack well for each transformation-stage cells/media key.
    pub transformation_source_wells: BTreeMap<String, String>,
    /// DNA-plate well holding each plasmid a strain is transformed from.
    pub dna_source_wells: BTreeMap<String, StarWell>,
    pub assemblies: Vec<StarAssemblyPlan>,
    pub strains: Vec<StarStrainPlan>,
    /// The volume the operator loads into each source position: everything
    /// the runs consume plus the vessel's dead volume.
    pub source_fills: Vec<SourceFill>,
    /// Tips consumed per tip-rack resource, against its capacity.
    pub tip_usage: BTreeMap<String, usize>,
    /// The ordered program: robot runs with the manual steps that follow
    /// each one.
    pub runs: Vec<StarRunPlan>,
}

/// One source position and the volume to load into it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceFill {
    /// The planning key (`reagent:…`, `dna:…`, `enzyme:…`, `cells:…`, or
    /// `medium`).
    pub key: String,
    pub location: StarWell,
    /// Total volume the runs draw, µL.
    pub consumed_ul: f64,
    /// What the operator loads: consumption plus the vessel dead volume.
    pub load_ul: f64,
}

/// One plasmid artifact assembled on the reaction plate.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarAssemblyPlan {
    pub artifact: String,
    pub sequence: String,
    pub backbone: String,
    pub components: Vec<String>,
    pub dependencies: Vec<String>,
    pub restriction_enzyme: String,
    pub assembly_replicates: u8,
    pub water_volume_ul: u16,
    pub assembly_wells: Vec<String>,
    pub chemistry: StarAssemblyChemistry,
}

/// Golden Gate reaction parameters stated by the plasmid design.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StarAssemblyChemistry {
    pub reaction_volume_ul: u16,
    pub part_volume_ul: u16,
    pub enzyme_volume_ul: u16,
    pub ligase_volume_ul: u16,
    pub buffer_volume_ul: u16,
    pub cycles: u16,
    pub digest_temperature_c: u16,
    pub digest_minutes: u16,
    pub ligate_temperature_c: u16,
    pub ligate_minutes: u16,
}

/// One strain artifact transformed from plasmids and plated.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarStrainPlan {
    pub artifact: String,
    pub host: String,
    pub plasmids: Vec<String>,
    pub dependencies: Vec<String>,
    pub selection: String,
    pub transformation_replicates: u8,
    pub plating_replicates: u8,
    pub serial_dilutions: u8,
    pub transformations: Vec<StarTransformationPlan>,
    pub plating: Vec<StarPlatingPlan>,
    pub chemistry: StarStrainChemistry,
}

/// Heat-shock transformation and plating parameters stated by the strain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StarStrainChemistry {
    pub cell_volume_ul: u16,
    pub dna_volume_ul: u16,
    pub recovery_volume_ul: u16,
    pub cold_minutes: u16,
    pub heat_shock_temperature_c: u16,
    pub heat_shock_minutes: u16,
    pub recovery_temperature_c: u16,
    pub recovery_minutes: u16,
    pub medium_volume_ul: u16,
    pub culture_volume_ul: u16,
    pub colony_volume_ul: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarTransformationPlan {
    pub culture_well: String,
    /// DNA-plate wells whose contents enter this reaction.
    pub source_wells: Vec<StarWell>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarPlatingPlan {
    pub culture_well: String,
    pub dilution_wells: Vec<StarWell>,
    pub agar_wells: Vec<Vec<StarWell>>,
}

/// One robot run and the manual steps that follow it before the next run
/// may start.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarRunPlan {
    /// The run's file stem, e.g. `assembly_run`.
    pub id: String,
    pub title: String,
    pub operations: Vec<StarOperation>,
    pub manual_after: Vec<ManualStep>,
    /// The thermal programs behind this run's manual steps, structured for
    /// projection into separate thermocycler documents. On a standalone STAR
    /// adapter the operator prose in `manual_after` is the whole story, so
    /// these never reach the serialized manifest.
    #[serde(skip)]
    pub thermal_after: Vec<ThermalRequirement>,
}

/// A thermal program a run needs after its liquid handling. Each
/// requirement shadows one step of `manual_after` (named by
/// `fallback_index`) so a facility plan can replace the prose with an exact
/// thermocycler binding.
#[derive(Clone, Debug, PartialEq)]
pub struct ThermalRequirement {
    /// Stable identity within the run, e.g. `assembly_thermocycle`.
    pub id: String,
    pub title: String,
    /// The deck resource that carries the reactions through the program.
    pub plate: String,
    pub profile: lab_instruments::ThermalProfile,
    /// Temperature held after the profile ends, until retrieval.
    pub final_hold_celsius: Option<f64>,
    /// Approximate per-well fill, which thermocyclers use to pick a
    /// volume-dependent control class.
    pub fill_volume_ul: f64,
    /// The position of this requirement's operator fallback in the run's
    /// `manual_after`.
    pub fallback_index: usize,
}

pub use crate::runfmt::ManualStep;

/// The two tip sizes a run draws on, mapped to concrete racks and driver
/// tip types by the profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TipClass {
    Small,
    Large,
}

/// One lowered machine operation. Positions are 0.1 mm, volumes 0.1 µL,
/// speeds 0.1 µL/s — the wire units the firmware frames carry — alongside
/// the resource labels the operator-facing descriptions are written from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum StarOperation {
    /// `TP`: pick up one tip per listed channel.
    PickUpTips {
        tip: TipClass,
        /// Begin-of-pickup Z, 0.1 mm, including the size-class correction.
        begin_z: u32,
        /// End-of-pickup Z, 0.1 mm.
        end_z: u32,
        positions: Vec<TipPickupPosition>,
    },
    /// `AS`: one aspirate across the listed channels.
    Aspirate {
        tip: TipClass,
        channels: Vec<ChannelLiquid>,
    },
    /// `DS`: one dispense across the listed channels.
    Dispense {
        tip: TipClass,
        /// `dm` mode: 0 partial jet, 1 blow-out jet.
        mode: u32,
        channels: Vec<ChannelLiquid>,
    },
    /// `TR`: drop the listed channels' tips into the tip waste.
    DiscardTips { channels: Vec<usize> },
}

/// One channel's tip pickup position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TipPickupPosition {
    pub channel: usize,
    pub location: StarWell,
    /// Deck X, 0.1 mm.
    pub x: u32,
    /// Deck Y, 0.1 mm.
    pub y: u32,
}

/// One channel's share of a liquid operation, with its heights resolved
/// against the tracked well volumes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChannelLiquid {
    pub channel: usize,
    pub location: StarWell,
    /// Deck X, 0.1 mm.
    pub x: u32,
    /// Deck Y, 0.1 mm.
    pub y: u32,
    /// The liquid position `zl`/dispense position, 0.1 mm.
    pub position_z: u32,
    /// The LLD search height `lp`, 0.1 mm.
    pub lld_search_z: u32,
    /// The minimum height `zx`, 0.1 mm: the vessel bottom standoff.
    pub minimum_z: u32,
    /// What the science asked for, µL.
    pub target_ul: f64,
    /// The liquid-class-corrected wire volume `av`/`dv`, 0.1 µL.
    pub corrected_volume: u32,
    /// Mix volume `mv`, 0.1 µL; zero when the operation does not mix.
    pub mix_volume: u32,
    /// Mix cycles `mc`.
    pub mix_cycles: u32,
}
