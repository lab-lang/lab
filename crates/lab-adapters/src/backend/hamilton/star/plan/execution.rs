//! Resource-allocated STAR runs whose canonical liquid operations have been lowered to firmware
//! wire units.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::backend::hamilton::star::liquid_classes::{
    LiquidClassEvidence, LiquidClassIdentity, LiquidClassLibraryIdentity,
};
use crate::backend::hamilton::star::profile::LldPolicy;
use crate::backend::hamilton::star::profile::StarAdapterProfile;

/// A well on a named profile resource. Resource keys are stable strings shared by the deck
/// summary and emitters.
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

/// Every source fill, tip allocation, liquid class, and lowered run emitted for one task.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarExecutionPlan {
    pub schema_version: String,
    /// The explicit adapter implementation that produced this device plan.
    pub adapter: String,
    /// Checked implementation configuration for the allocated Asset binding.
    pub deck: StarAdapterProfile,
    /// The volume the operator loads into each source position: everything
    /// the runs consume plus the vessel's dead volume.
    pub source_fills: Vec<SourceFill>,
    /// Tips consumed per tip-rack resource, against its capacity.
    pub tip_usage: BTreeMap<String, usize>,
    /// Exact profile-selected library from which all class evidence below was
    /// resolved.
    pub liquid_class_library: LiquidClassLibraryIdentity,
    /// Exact liquid-class snapshots selected while lowering this plan. The
    /// identity of each snapshot is also carried by every liquid channel.
    pub liquid_classes: Vec<LiquidClassEvidence>,
    /// The ordered program: robot runs with the manual steps that follow
    /// each one.
    pub runs: Vec<StarRunPlan>,
}

/// One source position and the volume to load into it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceFill {
    /// The exact material allocation symbol or Procedure-input identity.
    pub key: String,
    pub location: StarWell,
    /// Total volume the runs draw, µL.
    pub consumed_ul: f64,
    /// What the operator loads: consumption plus the vessel dead volume.
    pub load_ul: f64,
}

/// One robot run and the manual steps that follow it before the next run
/// may start.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StarRunPlan {
    /// Stable identifier for this canonical run.
    pub id: String,
    pub title: String,
    pub operations: Vec<StarOperation>,
    pub manual_after: Vec<ManualStep>,
}

pub use lab_runfmt::ManualStep;

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
    /// Exact data-defined class that supplied correction and motion settings.
    pub liquid_class: LiquidClassIdentity,
    /// The liquid-class-corrected wire volume `av`/`dv`, 0.1 µL.
    pub corrected_volume: u32,
    /// Aspirate speed, 0.1 µL/s.
    pub aspirate_speed: u32,
    /// Dispense speed, 0.1 µL/s.
    pub dispense_speed: u32,
    /// Aspirate-side mix speed, 0.1 µL/s.
    pub aspirate_mix_speed: u32,
    /// Dispense-side mix speed, 0.1 µL/s.
    pub dispense_mix_speed: u32,
    /// Effective LLD mode after applying the class policy to the Asset
    /// profile's checked setting.
    pub lld: LldPolicy,
    pub gamma_lld_sensitivity: u32,
    pub pressure_lld_sensitivity: u32,
    /// Mix volume `mv`, 0.1 µL; zero when the operation does not mix.
    pub mix_volume: u32,
    /// Mix cycles `mc`.
    pub mix_cycles: u32,
}
