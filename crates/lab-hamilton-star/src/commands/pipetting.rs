//! The 8-channel pipetting commands on the master controller: channel
//! initialization, tip pickup and discard, aspirate, dispense, channel
//! movement, and channel queries.
//!
//! All positions are 0.1 mm, volumes 0.1 µL, pipetting speeds 0.1 µL/s, Z
//! and swap speeds 0.1 mm/s, and settling times 0.1 s. Per-channel values
//! are built from sparse `(channel, value)` inputs: coordinate lists fill
//! unused positions with zero, liquid-parameter lists with a copy of the
//! first value, both per the firmware's don't-care rules.

use crate::commands::Command;
use crate::commands::system::TipPickupMethod;
use crate::errors::{CommandError, check_range};
use crate::framing::{ChannelPattern, ChannelValues, Fill, FrameBuilder, Module};
use crate::response::{FieldSpec, ResponseParseError, parse_fields};

/// One channel's deck target: which channel goes where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelTarget {
    /// 0-based channel index.
    pub channel: usize,
    /// `xp`: deck X in 0.1 mm, 0–25000.
    pub x: u32,
    /// `yp`: deck Y in 0.1 mm, 0–6500.
    pub y: u32,
}

fn check_targets(targets: &[ChannelTarget]) -> Result<(), CommandError> {
    for target in targets {
        check_range(
            "xp",
            "channel X position",
            "0.1 mm",
            f64::from(target.x),
            0.0,
            25000.0,
        )?;
        check_range(
            "yp",
            "channel Y position",
            "0.1 mm",
            f64::from(target.y),
            0.0,
            6500.0,
        )?;
    }
    Ok(())
}

fn split_targets(
    targets: &[ChannelTarget],
    machine_channels: usize,
) -> Result<(ChannelValues<u32>, ChannelValues<u32>, ChannelPattern), CommandError> {
    let xs: Vec<(usize, u32)> = targets.iter().map(|t| (t.channel, t.x)).collect();
    let ys: Vec<(usize, u32)> = targets.iter().map(|t| (t.channel, t.y)).collect();
    let channels: Vec<usize> = targets.iter().map(|t| t.channel).collect();
    Ok((
        ChannelValues::from_sparse(&xs, machine_channels, Fill::Zero, 0)?,
        ChannelValues::from_sparse(&ys, machine_channels, Fill::Zero, 0)?,
        ChannelPattern::from_channels(&channels, machine_channels)?,
    ))
}

/// The tip discard methods of the `TR`/`DI` `ti` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipDiscardMethod {
    /// Code 0: place and shift. Deposit heights reference the tip cone end.
    PlaceAndShift,
    /// Code 1: drop, no shift. Deposit heights reference the stop disk.
    #[default]
    Drop,
}

impl TipDiscardMethod {
    pub fn code(self) -> u32 {
        match self {
            TipDiscardMethod::PlaceAndShift => 0,
            TipDiscardMethod::Drop => 1,
        }
    }
}

/// `DI` — initialize the pipetting channels, discarding any mounted tips at
/// the given positions. The reply arrives when the channels finish homing
/// (120 s timeout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeChannels {
    xp: ChannelValues<u32>,
    yp: ChannelValues<u32>,
    tm: ChannelPattern,
    begin_z: u32,
    end_z: u32,
    end_of_command_z: u32,
    tip_type: u32,
    method: TipDiscardMethod,
}

impl InitializeChannels {
    /// `begin_z`/`end_z` are the tip-deposit Z range; `end_of_command_z` is
    /// where the channels finish. With `PlaceAndShift` the deposit heights
    /// reference the tip cone end, with `Drop` the stop disk.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        targets: &[ChannelTarget],
        machine_channels: usize,
        begin_z: u32,
        end_z: u32,
        end_of_command_z: u32,
        tip_type: u32,
        method: TipDiscardMethod,
    ) -> Result<InitializeChannels, CommandError> {
        check_targets(targets)?;
        check_range(
            "tp",
            "begin of tip deposit Z",
            "0.1 mm",
            f64::from(begin_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "tz",
            "end of tip deposit Z",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "te",
            "Z at end of command",
            "0.1 mm",
            f64::from(end_of_command_z),
            0.0,
            3600.0,
        )?;
        check_range("tt", "tip type index", "", f64::from(tip_type), 0.0, 99.0)?;
        let (xp, yp, tm) = split_targets(targets, machine_channels)?;
        Ok(InitializeChannels {
            xp,
            yp,
            tm,
            begin_z,
            end_z,
            end_of_command_z,
            tip_type,
            method,
        })
    }
}

impl Command for InitializeChannels {
    const CODE: &'static str = "DI";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint_list("xp", 5, &self.xp)
            .uint_list("yp", 4, &self.yp)
            .uint("tp", 4, self.begin_z)
            .uint("tz", 4, self.end_z)
            .uint("te", 4, self.end_of_command_z)
            .flag_list("tm", &self.tm)
            .uint("tt", 2, self.tip_type)
            .uint("ti", 1, self.method.code())
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `TP` — pick up tips (120 s timeout). The firmware plans all X/Y/Z motion
/// itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipPickup {
    xp: ChannelValues<u32>,
    yp: ChannelValues<u32>,
    tm: ChannelPattern,
    tip_type: u32,
    begin_z: u32,
    end_z: u32,
    traverse_height: u32,
    method: TipPickupMethod,
}

/// The default minimum traverse height for channel commands: 245.0 mm.
pub const DEFAULT_TRAVERSE_HEIGHT: u32 = 2450;

impl TipPickup {
    /// `begin_z` is where the pickup starts (tip-spot Z plus the total tip
    /// length), `end_z` where the press ends (`begin_z` minus tip length
    /// less fitting depth), both in 0.1 mm.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        targets: &[ChannelTarget],
        machine_channels: usize,
        tip_type: u32,
        begin_z: u32,
        end_z: u32,
        traverse_height: u32,
        method: TipPickupMethod,
    ) -> Result<TipPickup, CommandError> {
        check_targets(targets)?;
        check_range("tt", "tip type index", "", f64::from(tip_type), 0.0, 99.0)?;
        check_range(
            "tp",
            "begin of tip pickup Z",
            "0.1 mm",
            f64::from(begin_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "tz",
            "end of tip pickup Z",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "th",
            "minimum traverse height",
            "0.1 mm",
            f64::from(traverse_height),
            0.0,
            3600.0,
        )?;
        let (xp, yp, tm) = split_targets(targets, machine_channels)?;
        Ok(TipPickup {
            xp,
            yp,
            tm,
            tip_type,
            begin_z,
            end_z,
            traverse_height,
            method,
        })
    }
}

impl Command for TipPickup {
    const CODE: &'static str = "TP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint_list("xp", 5, &self.xp)
            .uint_list("yp", 4, &self.yp)
            .flag_list("tm", &self.tm)
            .uint("tt", 2, self.tip_type)
            .uint("tp", 4, self.begin_z)
            .uint("tz", 4, self.end_z)
            .uint("th", 4, self.traverse_height)
            .uint("td", 1, self.method.code())
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The reply to a tip discard: the per-channel `kz`/`vz` height lists, when
/// the firmware reports them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TipDiscardReport {
    pub kz: Vec<i64>,
    pub vz: Vec<i64>,
}

/// `TR` — discard tips (120 s timeout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipDiscard {
    xp: ChannelValues<u32>,
    yp: ChannelValues<u32>,
    tm: ChannelPattern,
    begin_z: u32,
    end_z: u32,
    traverse_height: u32,
    end_of_command_z: u32,
    method: TipDiscardMethod,
}

impl TipDiscard {
    /// With `PlaceAndShift`, `begin_z`/`end_z` reference the tip cone end:
    /// deposit Z + 59.9 mm and deposit Z + 49.9 mm (empirical constants).
    /// With `Drop` they reference the stop disk.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        targets: &[ChannelTarget],
        machine_channels: usize,
        begin_z: u32,
        end_z: u32,
        traverse_height: u32,
        end_of_command_z: u32,
        method: TipDiscardMethod,
    ) -> Result<TipDiscard, CommandError> {
        check_targets(targets)?;
        check_range(
            "tp",
            "begin of tip deposit Z",
            "0.1 mm",
            f64::from(begin_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "tz",
            "end of tip deposit Z",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "th",
            "minimum traverse height",
            "0.1 mm",
            f64::from(traverse_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "te",
            "Z at end of command",
            "0.1 mm",
            f64::from(end_of_command_z),
            0.0,
            3600.0,
        )?;
        let (xp, yp, tm) = split_targets(targets, machine_channels)?;
        Ok(TipDiscard {
            xp,
            yp,
            tm,
            begin_z,
            end_z,
            traverse_height,
            end_of_command_z,
            method,
        })
    }
}

impl Command for TipDiscard {
    const CODE: &'static str = "TR";
    type Response = TipDiscardReport;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint_list("xp", 5, &self.xp)
            .uint_list("yp", 4, &self.yp)
            .flag_list("tm", &self.tm)
            .uint("tp", 4, self.begin_z)
            .uint("tz", 4, self.end_z)
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_of_command_z)
            .uint("ti", 1, self.method.code())
    }
    fn parse_response(payload: &str) -> Result<TipDiscardReport, ResponseParseError> {
        // The height lists are informational; a reply without them is still
        // a successful discard.
        let kz = parse_fields(payload, &[FieldSpec::int_list("kz", 4)])
            .ok()
            .and_then(|f| f.int_list("kz").map(<[i64]>::to_vec))
            .unwrap_or_default();
        let vz = parse_fields(payload, &[FieldSpec::int_list("vz", 4)])
            .ok()
            .and_then(|f| f.int_list("vz").map(<[i64]>::to_vec))
            .unwrap_or_default();
        Ok(TipDiscardReport { kz, vz })
    }
}

/// The aspiration types of the `AS` `at` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AspirationType {
    /// Code 0: simple aspiration.
    #[default]
    Simple,
    /// Code 1: sequence aspiration.
    Sequence,
    /// Code 2: aspirate until the cup is emptied.
    CupEmptied,
}

impl AspirationType {
    pub fn code(self) -> u32 {
        match self {
            AspirationType::Simple => 0,
            AspirationType::Sequence => 1,
            AspirationType::CupEmptied => 2,
        }
    }
}

/// The liquid level detection modes of the `lm` parameter. Pressure and
/// dual modes are aspirate-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LldMode {
    /// Code 0: off — the command trusts the given liquid surface.
    #[default]
    Off,
    /// Code 1: capacitive (gamma) detection.
    Gamma,
    /// Code 2: pressure detection.
    Pressure,
    /// Code 3: gamma and pressure combined.
    Dual,
    /// Code 4: Z touch-off.
    ZTouchOff,
}

impl LldMode {
    pub fn code(self) -> u32 {
        match self {
            LldMode::Off => 0,
            LldMode::Gamma => 1,
            LldMode::Pressure => 2,
            LldMode::Dual => 3,
            LldMode::ZTouchOff => 4,
        }
    }
}

/// One channel's aspirate parameters, in wire units. The defaults are the
/// documented firmware defaults; positions, volumes, and heights carry no
/// default because guessing them moves the machine blind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AspirateChannel {
    /// 0-based channel index.
    pub channel: usize,
    /// `xp`: deck X, 0.1 mm.
    pub x: u32,
    /// `yp`: deck Y, 0.1 mm.
    pub y: u32,
    /// `at`: aspiration type.
    pub aspiration_type: AspirationType,
    /// `lp`: LLD search height, 0.1 mm. Well-bottom Z plus container height
    /// plus 2.7 mm for wells (5 mm for other containers).
    pub lld_search_height: u32,
    /// `ch`: clot detection height, 0.1 mm, 0–500.
    pub clot_detect_height: u32,
    /// `zl`: liquid surface Z when LLD is off, 0.1 mm.
    pub liquid_surface: u32,
    /// `po`: transport-air pull-out distance, 0.1 mm. Default 100.
    pub pull_out_distance: u32,
    /// `zu`: second-section height, 0.1 mm. Default 32.
    pub second_section_height: u32,
    /// `zr`: second-section ratio, 0–10000. Default 6180.
    pub second_section_ratio: u32,
    /// `zx`: minimum height (maximum immersion), 0.1 mm.
    pub minimum_height: u32,
    /// `ip`: immersion depth, 0.1 mm.
    pub immersion_depth: u32,
    /// `it`: immersion direction, `false` = down.
    pub immersion_direction_up: bool,
    /// `fp`: surface-following distance, 0.1 mm.
    pub surface_following_distance: u32,
    /// `av`: aspirate volume, 0.1 µL, 0–12500. Apply the liquid-class
    /// correction before setting this.
    pub volume: u32,
    /// `as`: aspirate speed, 0.1 µL/s, 4–5000. Default 1000.
    pub speed: u32,
    /// `ta`: transport air volume, 0.1 µL, 0–500.
    pub transport_air: u32,
    /// `ba`: blow-out air volume, 0.1 µL, 0–9999.
    pub blow_out_air: u32,
    /// `oa`: pre-wetting volume, 0.1 µL, 0–999.
    pub pre_wetting: u32,
    /// `lm`: liquid level detection mode.
    pub lld_mode: LldMode,
    /// `ll`: gamma LLD sensitivity, 1 (highest) – 4 (lowest).
    pub gamma_lld_sensitivity: u32,
    /// `lv`: pressure LLD sensitivity, 1 (highest) – 4 (lowest).
    pub pressure_lld_sensitivity: u32,
    /// `zo`: aspirate position above Z touch-off, 0.1 mm, 0–100.
    pub aspirate_above_touch_off: u32,
    /// `ld`: dual-LLD height difference, 0.1 mm, 0–99.
    pub dual_lld_height_difference: u32,
    /// `de`: swap speed, 0.1 mm/s, 3–1600. Default 100.
    pub swap_speed: u32,
    /// `wt`: settling time, 0.1 s, 0–99.
    pub settling_time: u32,
    /// `mv`: mix volume, 0.1 µL.
    pub mix_volume: u32,
    /// `mc`: mix cycles, 0–99.
    pub mix_cycles: u32,
    /// `mp`: mix position from surface, 0.1 mm.
    pub mix_position: u32,
    /// `ms`: mix speed, 0.1 µL/s. Default 1000.
    pub mix_speed: u32,
    /// `mh`: mix surface-following distance, 0.1 mm.
    pub mix_surface_follow: u32,
    /// `gi`: TADM limit-curve index.
    pub limit_curve_index: u32,
    /// `lk`: second-section flag.
    pub use_second_section: bool,
    /// `ik`: retract height over the second section, 0.1 mm.
    pub second_section_retract: u32,
    /// `sd`: second-section speed. Default 500.
    pub second_section_speed: u32,
    /// `se`: second-section end speed. Default 500.
    pub second_section_end_speed: u32,
    /// `sz`: second-section Z speed. Default 300.
    pub second_section_z_speed: u32,
    /// `io`: cup upper edge, 0.1 mm.
    pub cup_upper_edge: u32,
}

impl AspirateChannel {
    /// A channel with every documented default and all positions, heights,
    /// and volumes zero. Set the deck target, the liquid surface, and the
    /// volume before use.
    pub fn at(channel: usize, x: u32, y: u32) -> AspirateChannel {
        AspirateChannel {
            channel,
            x,
            y,
            aspiration_type: AspirationType::Simple,
            lld_search_height: 0,
            clot_detect_height: 0,
            liquid_surface: 0,
            pull_out_distance: 100,
            second_section_height: 32,
            second_section_ratio: 6180,
            minimum_height: 0,
            immersion_depth: 0,
            immersion_direction_up: false,
            surface_following_distance: 0,
            volume: 0,
            speed: 1000,
            transport_air: 0,
            blow_out_air: 0,
            pre_wetting: 0,
            lld_mode: LldMode::Off,
            gamma_lld_sensitivity: 1,
            pressure_lld_sensitivity: 1,
            aspirate_above_touch_off: 0,
            dual_lld_height_difference: 0,
            swap_speed: 100,
            settling_time: 0,
            mix_volume: 0,
            mix_cycles: 0,
            mix_position: 0,
            mix_speed: 1000,
            mix_surface_follow: 0,
            limit_curve_index: 0,
            use_second_section: false,
            second_section_retract: 0,
            second_section_speed: 500,
            second_section_end_speed: 500,
            second_section_z_speed: 300,
            cup_upper_edge: 0,
        }
    }

    fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "xp",
            "channel X position",
            "0.1 mm",
            f64::from(self.x),
            0.0,
            25000.0,
        )?;
        check_range(
            "yp",
            "channel Y position",
            "0.1 mm",
            f64::from(self.y),
            0.0,
            6500.0,
        )?;
        check_range(
            "lp",
            "LLD search height",
            "0.1 mm",
            f64::from(self.lld_search_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "ch",
            "clot detection height",
            "0.1 mm",
            f64::from(self.clot_detect_height),
            0.0,
            500.0,
        )?;
        check_range(
            "zl",
            "liquid surface",
            "0.1 mm",
            f64::from(self.liquid_surface),
            0.0,
            3600.0,
        )?;
        check_range(
            "po",
            "pull-out distance",
            "0.1 mm",
            f64::from(self.pull_out_distance),
            0.0,
            3600.0,
        )?;
        check_range(
            "zu",
            "second-section height",
            "0.1 mm",
            f64::from(self.second_section_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "zr",
            "second-section ratio",
            "",
            f64::from(self.second_section_ratio),
            0.0,
            10000.0,
        )?;
        check_range(
            "zx",
            "minimum height",
            "0.1 mm",
            f64::from(self.minimum_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "ip",
            "immersion depth",
            "0.1 mm",
            f64::from(self.immersion_depth),
            0.0,
            3600.0,
        )?;
        check_range(
            "fp",
            "surface-following distance",
            "0.1 mm",
            f64::from(self.surface_following_distance),
            0.0,
            3600.0,
        )?;
        check_range(
            "av",
            "aspirate volume",
            "0.1 µL",
            f64::from(self.volume),
            0.0,
            12500.0,
        )?;
        check_range(
            "as",
            "aspirate speed",
            "0.1 µL/s",
            f64::from(self.speed),
            4.0,
            5000.0,
        )?;
        check_range(
            "ta",
            "transport air volume",
            "0.1 µL",
            f64::from(self.transport_air),
            0.0,
            500.0,
        )?;
        check_range(
            "ba",
            "blow-out air volume",
            "0.1 µL",
            f64::from(self.blow_out_air),
            0.0,
            9999.0,
        )?;
        check_range(
            "oa",
            "pre-wetting volume",
            "0.1 µL",
            f64::from(self.pre_wetting),
            0.0,
            999.0,
        )?;
        check_range(
            "ll",
            "gamma LLD sensitivity",
            "",
            f64::from(self.gamma_lld_sensitivity),
            1.0,
            4.0,
        )?;
        check_range(
            "lv",
            "pressure LLD sensitivity",
            "",
            f64::from(self.pressure_lld_sensitivity),
            1.0,
            4.0,
        )?;
        check_range(
            "zo",
            "aspirate position above touch-off",
            "0.1 mm",
            f64::from(self.aspirate_above_touch_off),
            0.0,
            100.0,
        )?;
        check_range(
            "ld",
            "dual-LLD height difference",
            "0.1 mm",
            f64::from(self.dual_lld_height_difference),
            0.0,
            99.0,
        )?;
        check_range(
            "de",
            "swap speed",
            "0.1 mm/s",
            f64::from(self.swap_speed),
            3.0,
            1600.0,
        )?;
        check_range(
            "wt",
            "settling time",
            "0.1 s",
            f64::from(self.settling_time),
            0.0,
            99.0,
        )?;
        check_range(
            "mv",
            "mix volume",
            "0.1 µL",
            f64::from(self.mix_volume),
            0.0,
            12500.0,
        )?;
        check_range(
            "mc",
            "mix cycles",
            "",
            f64::from(self.mix_cycles),
            0.0,
            99.0,
        )?;
        check_range(
            "mp",
            "mix position",
            "0.1 mm",
            f64::from(self.mix_position),
            0.0,
            999.0,
        )?;
        check_range(
            "ms",
            "mix speed",
            "0.1 µL/s",
            f64::from(self.mix_speed),
            4.0,
            5000.0,
        )?;
        check_range(
            "mh",
            "mix surface-following distance",
            "0.1 mm",
            f64::from(self.mix_surface_follow),
            0.0,
            3600.0,
        )?;
        check_range(
            "gi",
            "limit-curve index",
            "",
            f64::from(self.limit_curve_index),
            0.0,
            999.0,
        )?;
        check_range(
            "ik",
            "second-section retract height",
            "0.1 mm",
            f64::from(self.second_section_retract),
            0.0,
            3600.0,
        )?;
        check_range(
            "io",
            "cup upper edge",
            "0.1 mm",
            f64::from(self.cup_upper_edge),
            0.0,
            9999.0,
        )?;
        Ok(())
    }
}

/// Builds one per-channel list from a field of the channel parameter
/// structs.
fn liquid_list<C>(
    channels: &[C],
    machine_channels: usize,
    field: impl Fn(&C) -> (usize, u32),
) -> Result<ChannelValues<u32>, CommandError> {
    let pairs: Vec<(usize, u32)> = channels.iter().map(field).collect();
    Ok(ChannelValues::from_sparse(
        &pairs,
        machine_channels,
        Fill::FirstValue,
        0,
    )?)
}

/// `AS` — aspirate (300 s timeout). The firmware plans all motion within
/// the command; nothing else needs to move first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aspirate {
    tm: ChannelPattern,
    machine_channels: usize,
    channels: Vec<AspirateChannel>,
    traverse_height: u32,
    end_z: u32,
    tadm_algorithm: bool,
    tadm_recording: bool,
}

impl Aspirate {
    /// `traverse_height` (`th`) and `end_z` (`te`) apply to all channels.
    pub fn new(
        channels: Vec<AspirateChannel>,
        machine_channels: usize,
        traverse_height: u32,
        end_z: u32,
    ) -> Result<Aspirate, CommandError> {
        check_range(
            "th",
            "minimum traverse height",
            "0.1 mm",
            f64::from(traverse_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "te",
            "Z at end of command",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        for channel in &channels {
            channel.validate()?;
        }
        let indices: Vec<usize> = channels.iter().map(|c| c.channel).collect();
        let tm = ChannelPattern::from_channels(&indices, machine_channels)?;
        Ok(Aspirate {
            tm,
            machine_channels,
            channels,
            traverse_height,
            end_z,
            tadm_algorithm: false,
            tadm_recording: false,
        })
    }

    fn list(&self, field: impl Fn(&AspirateChannel) -> u32) -> ChannelValues<u32> {
        liquid_list(&self.channels, self.machine_channels, |c| {
            (c.channel, field(c))
        })
        .expect("channel indices were validated at construction")
    }
}

impl Command for Aspirate {
    const CODE: &'static str = "AS";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let xs: Vec<(usize, u32)> = self.channels.iter().map(|c| (c.channel, c.x)).collect();
        let ys: Vec<(usize, u32)> = self.channels.iter().map(|c| (c.channel, c.y)).collect();
        let xp = ChannelValues::from_sparse(&xs, self.machine_channels, Fill::Zero, 0)
            .expect("channel indices were validated at construction");
        let yp = ChannelValues::from_sparse(&ys, self.machine_channels, Fill::Zero, 0)
            .expect("channel indices were validated at construction");
        builder
            .uint_list("at", 1, &self.list(|c| c.aspiration_type.code()))
            .flag_list("tm", &self.tm)
            .uint_list("xp", 5, &xp)
            .uint_list("yp", 4, &yp)
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
            .uint_list("lp", 4, &self.list(|c| c.lld_search_height))
            .uint_list("ch", 3, &self.list(|c| c.clot_detect_height))
            .uint_list("zl", 4, &self.list(|c| c.liquid_surface))
            .uint_list("po", 4, &self.list(|c| c.pull_out_distance))
            .uint_list("zu", 4, &self.list(|c| c.second_section_height))
            .uint_list("zr", 5, &self.list(|c| c.second_section_ratio))
            .uint_list("zx", 4, &self.list(|c| c.minimum_height))
            .uint_list("ip", 4, &self.list(|c| c.immersion_depth))
            .uint_list("it", 1, &self.list(|c| u32::from(c.immersion_direction_up)))
            .uint_list("fp", 4, &self.list(|c| c.surface_following_distance))
            .uint_list("av", 5, &self.list(|c| c.volume))
            .uint_list("as", 4, &self.list(|c| c.speed))
            .uint_list("ta", 3, &self.list(|c| c.transport_air))
            .uint_list("ba", 4, &self.list(|c| c.blow_out_air))
            .uint_list("oa", 3, &self.list(|c| c.pre_wetting))
            .uint_list("lm", 1, &self.list(|c| c.lld_mode.code()))
            .uint_list("ll", 1, &self.list(|c| c.gamma_lld_sensitivity))
            .uint_list("lv", 1, &self.list(|c| c.pressure_lld_sensitivity))
            .uint_list("zo", 3, &self.list(|c| c.aspirate_above_touch_off))
            .uint_list("ld", 2, &self.list(|c| c.dual_lld_height_difference))
            .uint_list("de", 4, &self.list(|c| c.swap_speed))
            .uint_list("wt", 2, &self.list(|c| c.settling_time))
            .uint_list("mv", 5, &self.list(|c| c.mix_volume))
            .uint_list("mc", 2, &self.list(|c| c.mix_cycles))
            .uint_list("mp", 3, &self.list(|c| c.mix_position))
            .uint_list("ms", 4, &self.list(|c| c.mix_speed))
            .uint_list("mh", 4, &self.list(|c| c.mix_surface_follow))
            .uint_list("gi", 3, &self.list(|c| c.limit_curve_index))
            .flag("gj", self.tadm_algorithm)
            .flag("gk", self.tadm_recording)
            .uint_list("lk", 1, &self.list(|c| u32::from(c.use_second_section)))
            .uint_list("ik", 4, &self.list(|c| c.second_section_retract))
            .uint_list("sd", 4, &self.list(|c| c.second_section_speed))
            .uint_list("se", 4, &self.list(|c| c.second_section_end_speed))
            .uint_list("sz", 4, &self.list(|c| c.second_section_z_speed))
            .uint_list("io", 4, &self.list(|c| c.cup_upper_edge))
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The dispense modes of the `DS` `dm` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispenseMode {
    /// Code 0: partial volume, jet.
    PartialJet,
    /// Code 1: blow-out, jet.
    BlowOutJet,
    /// Code 2: partial volume, at surface.
    PartialSurface,
    /// Code 3: blow-out, at surface.
    BlowOutSurface,
    /// Code 4: empty the tip at a fixed position.
    EmptyTip,
}

impl DispenseMode {
    pub fn code(self) -> u32 {
        match self {
            DispenseMode::PartialJet => 0,
            DispenseMode::BlowOutJet => 1,
            DispenseMode::PartialSurface => 2,
            DispenseMode::BlowOutSurface => 3,
            DispenseMode::EmptyTip => 4,
        }
    }

    /// The documented mode selection: emptying wins, then jet versus
    /// surface, each in partial or blow-out form.
    pub fn select(jet: bool, blow_out: bool, empty: bool) -> DispenseMode {
        if empty {
            DispenseMode::EmptyTip
        } else {
            match (jet, blow_out) {
                (true, true) => DispenseMode::BlowOutJet,
                (true, false) => DispenseMode::PartialJet,
                (false, true) => DispenseMode::BlowOutSurface,
                (false, false) => DispenseMode::PartialSurface,
            }
        }
    }
}

/// One channel's dispense parameters, in wire units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispenseChannel {
    /// 0-based channel index.
    pub channel: usize,
    /// `dm`: dispense mode.
    pub mode: DispenseMode,
    /// `xp`: deck X, 0.1 mm.
    pub x: u32,
    /// `yp`: deck Y, 0.1 mm.
    pub y: u32,
    /// `zx`: minimum height, 0.1 mm.
    pub minimum_height: u32,
    /// `lp`: LLD search height, 0.1 mm.
    pub lld_search_height: u32,
    /// `zl`: liquid surface Z when LLD is off, 0.1 mm.
    pub liquid_surface: u32,
    /// `po`: transport-air pull-out distance, 0.1 mm. Default 100.
    pub pull_out_distance: u32,
    /// `ip`: immersion depth, 0.1 mm.
    pub immersion_depth: u32,
    /// `it`: immersion direction, `false` = down.
    pub immersion_direction_up: bool,
    /// `fp`: surface-following distance, 0.1 mm.
    pub surface_following_distance: u32,
    /// `zu`: second-section height, 0.1 mm. Default 32.
    pub second_section_height: u32,
    /// `zr`: second-section ratio. Default 6180.
    pub second_section_ratio: u32,
    /// `dv`: dispense volume, 0.1 µL, 0–12500.
    pub volume: u32,
    /// `ds`: dispense speed, 0.1 µL/s, 4–5000. Default 1200.
    pub speed: u32,
    /// `ss`: cut-off speed, 0.1 µL/s. Default 50.
    pub cut_off_speed: u32,
    /// `rv`: stop-back volume, 0.1 µL, 0–180.
    pub stop_back_volume: u32,
    /// `ta`: transport air volume, 0.1 µL, 0–500.
    pub transport_air: u32,
    /// `ba`: blow-out air volume, 0.1 µL, 0–9999.
    pub blow_out_air: u32,
    /// `lm`: liquid level detection mode. Pressure and dual LLD are
    /// aspirate-only.
    pub lld_mode: LldMode,
    /// `zo`: dispense position above Z touch-off, 0.1 mm, 0–100.
    pub dispense_above_touch_off: u32,
    /// `ll`: gamma LLD sensitivity, 1–4.
    pub gamma_lld_sensitivity: u32,
    /// `lv`: pressure LLD sensitivity, 1–4.
    pub pressure_lld_sensitivity: u32,
    /// `de`: swap speed, 0.1 mm/s, 3–1600.
    pub swap_speed: u32,
    /// `wt`: settling time, 0.1 s, 0–99.
    pub settling_time: u32,
    /// `mv`: mix volume, 0.1 µL.
    pub mix_volume: u32,
    /// `mc`: mix cycles, 0–99.
    pub mix_cycles: u32,
    /// `mp`: mix position from surface, 0.1 mm.
    pub mix_position: u32,
    /// `ms`: mix speed, 0.1 µL/s.
    pub mix_speed: u32,
    /// `mh`: mix surface-following distance, 0.1 mm.
    pub mix_surface_follow: u32,
    /// `gi`: TADM limit-curve index.
    pub limit_curve_index: u32,
}

impl DispenseChannel {
    /// A channel with every documented default. Set the deck target, the
    /// surface, the mode, and the volume before use.
    pub fn at(channel: usize, x: u32, y: u32) -> DispenseChannel {
        DispenseChannel {
            channel,
            mode: DispenseMode::BlowOutJet,
            x,
            y,
            minimum_height: 0,
            lld_search_height: 0,
            liquid_surface: 0,
            pull_out_distance: 100,
            immersion_depth: 0,
            immersion_direction_up: false,
            surface_following_distance: 0,
            second_section_height: 32,
            second_section_ratio: 6180,
            volume: 0,
            speed: 1200,
            cut_off_speed: 50,
            stop_back_volume: 0,
            transport_air: 0,
            blow_out_air: 0,
            lld_mode: LldMode::Off,
            dispense_above_touch_off: 0,
            gamma_lld_sensitivity: 1,
            pressure_lld_sensitivity: 1,
            swap_speed: 100,
            settling_time: 0,
            mix_volume: 0,
            mix_cycles: 0,
            mix_position: 0,
            mix_speed: 10,
            mix_surface_follow: 0,
            limit_curve_index: 0,
        }
    }

    fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "xp",
            "channel X position",
            "0.1 mm",
            f64::from(self.x),
            0.0,
            25000.0,
        )?;
        check_range(
            "yp",
            "channel Y position",
            "0.1 mm",
            f64::from(self.y),
            0.0,
            6500.0,
        )?;
        check_range(
            "zx",
            "minimum height",
            "0.1 mm",
            f64::from(self.minimum_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "lp",
            "LLD search height",
            "0.1 mm",
            f64::from(self.lld_search_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "zl",
            "liquid surface",
            "0.1 mm",
            f64::from(self.liquid_surface),
            0.0,
            3600.0,
        )?;
        check_range(
            "po",
            "pull-out distance",
            "0.1 mm",
            f64::from(self.pull_out_distance),
            0.0,
            3600.0,
        )?;
        check_range(
            "ip",
            "immersion depth",
            "0.1 mm",
            f64::from(self.immersion_depth),
            0.0,
            3600.0,
        )?;
        check_range(
            "fp",
            "surface-following distance",
            "0.1 mm",
            f64::from(self.surface_following_distance),
            0.0,
            3600.0,
        )?;
        check_range(
            "zu",
            "second-section height",
            "0.1 mm",
            f64::from(self.second_section_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "zr",
            "second-section ratio",
            "",
            f64::from(self.second_section_ratio),
            0.0,
            10000.0,
        )?;
        check_range(
            "dv",
            "dispense volume",
            "0.1 µL",
            f64::from(self.volume),
            0.0,
            12500.0,
        )?;
        check_range(
            "ds",
            "dispense speed",
            "0.1 µL/s",
            f64::from(self.speed),
            4.0,
            5000.0,
        )?;
        check_range(
            "ss",
            "cut-off speed",
            "0.1 µL/s",
            f64::from(self.cut_off_speed),
            4.0,
            5000.0,
        )?;
        check_range(
            "rv",
            "stop-back volume",
            "0.1 µL",
            f64::from(self.stop_back_volume),
            0.0,
            180.0,
        )?;
        check_range(
            "ta",
            "transport air volume",
            "0.1 µL",
            f64::from(self.transport_air),
            0.0,
            500.0,
        )?;
        check_range(
            "ba",
            "blow-out air volume",
            "0.1 µL",
            f64::from(self.blow_out_air),
            0.0,
            9999.0,
        )?;
        check_range(
            "zo",
            "dispense position above touch-off",
            "0.1 mm",
            f64::from(self.dispense_above_touch_off),
            0.0,
            100.0,
        )?;
        check_range(
            "ll",
            "gamma LLD sensitivity",
            "",
            f64::from(self.gamma_lld_sensitivity),
            1.0,
            4.0,
        )?;
        check_range(
            "lv",
            "pressure LLD sensitivity",
            "",
            f64::from(self.pressure_lld_sensitivity),
            1.0,
            4.0,
        )?;
        check_range(
            "de",
            "swap speed",
            "0.1 mm/s",
            f64::from(self.swap_speed),
            3.0,
            1600.0,
        )?;
        check_range(
            "wt",
            "settling time",
            "0.1 s",
            f64::from(self.settling_time),
            0.0,
            99.0,
        )?;
        check_range(
            "mv",
            "mix volume",
            "0.1 µL",
            f64::from(self.mix_volume),
            0.0,
            12500.0,
        )?;
        check_range(
            "mc",
            "mix cycles",
            "",
            f64::from(self.mix_cycles),
            0.0,
            99.0,
        )?;
        check_range(
            "mp",
            "mix position",
            "0.1 mm",
            f64::from(self.mix_position),
            0.0,
            999.0,
        )?;
        check_range(
            "ms",
            "mix speed",
            "0.1 µL/s",
            f64::from(self.mix_speed),
            4.0,
            5000.0,
        )?;
        check_range(
            "mh",
            "mix surface-following distance",
            "0.1 mm",
            f64::from(self.mix_surface_follow),
            0.0,
            3600.0,
        )?;
        check_range(
            "gi",
            "limit-curve index",
            "",
            f64::from(self.limit_curve_index),
            0.0,
            999.0,
        )?;
        Ok(())
    }
}

/// `DS` — dispense (300 s timeout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispense {
    tm: ChannelPattern,
    machine_channels: usize,
    channels: Vec<DispenseChannel>,
    traverse_height: u32,
    end_z: u32,
    /// `dj`: side touch-off distance, 0.1 mm, 0–45. Nonzero disables LLD
    /// and Z touch-off.
    side_touch_off_distance: u32,
    tadm_algorithm: bool,
    tadm_recording: bool,
}

impl Dispense {
    pub fn new(
        channels: Vec<DispenseChannel>,
        machine_channels: usize,
        traverse_height: u32,
        end_z: u32,
        side_touch_off_distance: u32,
    ) -> Result<Dispense, CommandError> {
        check_range(
            "th",
            "minimum traverse height",
            "0.1 mm",
            f64::from(traverse_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "te",
            "Z at end of command",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        check_range(
            "dj",
            "side touch-off distance",
            "0.1 mm",
            f64::from(side_touch_off_distance),
            0.0,
            45.0,
        )?;
        for channel in &channels {
            channel.validate()?;
        }
        let indices: Vec<usize> = channels.iter().map(|c| c.channel).collect();
        let tm = ChannelPattern::from_channels(&indices, machine_channels)?;
        Ok(Dispense {
            tm,
            machine_channels,
            channels,
            traverse_height,
            end_z,
            side_touch_off_distance,
            tadm_algorithm: false,
            tadm_recording: false,
        })
    }

    fn list(&self, field: impl Fn(&DispenseChannel) -> u32) -> ChannelValues<u32> {
        liquid_list(&self.channels, self.machine_channels, |c| {
            (c.channel, field(c))
        })
        .expect("channel indices were validated at construction")
    }
}

impl Command for Dispense {
    const CODE: &'static str = "DS";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let xs: Vec<(usize, u32)> = self.channels.iter().map(|c| (c.channel, c.x)).collect();
        let ys: Vec<(usize, u32)> = self.channels.iter().map(|c| (c.channel, c.y)).collect();
        let xp = ChannelValues::from_sparse(&xs, self.machine_channels, Fill::Zero, 0)
            .expect("channel indices were validated at construction");
        let yp = ChannelValues::from_sparse(&ys, self.machine_channels, Fill::Zero, 0)
            .expect("channel indices were validated at construction");
        builder
            .uint_list("dm", 1, &self.list(|c| c.mode.code()))
            .flag_list("tm", &self.tm)
            .uint_list("xp", 5, &xp)
            .uint_list("yp", 4, &yp)
            .uint_list("zx", 4, &self.list(|c| c.minimum_height))
            .uint_list("lp", 4, &self.list(|c| c.lld_search_height))
            .uint_list("zl", 4, &self.list(|c| c.liquid_surface))
            .uint_list("po", 4, &self.list(|c| c.pull_out_distance))
            .uint_list("ip", 4, &self.list(|c| c.immersion_depth))
            .uint_list("it", 1, &self.list(|c| u32::from(c.immersion_direction_up)))
            .uint_list("fp", 4, &self.list(|c| c.surface_following_distance))
            .uint_list("zu", 4, &self.list(|c| c.second_section_height))
            .uint_list("zr", 5, &self.list(|c| c.second_section_ratio))
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
            .uint_list("dv", 5, &self.list(|c| c.volume))
            .uint_list("ds", 4, &self.list(|c| c.speed))
            .uint_list("ss", 4, &self.list(|c| c.cut_off_speed))
            .uint_list("rv", 3, &self.list(|c| c.stop_back_volume))
            .uint_list("ta", 3, &self.list(|c| c.transport_air))
            .uint_list("ba", 4, &self.list(|c| c.blow_out_air))
            .uint_list("lm", 1, &self.list(|c| c.lld_mode.code()))
            .uint("dj", 2, self.side_touch_off_distance)
            .uint_list("zo", 3, &self.list(|c| c.dispense_above_touch_off))
            .uint_list("ll", 1, &self.list(|c| c.gamma_lld_sensitivity))
            .uint_list("lv", 1, &self.list(|c| c.pressure_lld_sensitivity))
            .uint_list("de", 4, &self.list(|c| c.swap_speed))
            .uint_list("wt", 2, &self.list(|c| c.settling_time))
            .uint_list("mv", 5, &self.list(|c| c.mix_volume))
            .uint_list("mc", 2, &self.list(|c| c.mix_cycles))
            .uint_list("mp", 3, &self.list(|c| c.mix_position))
            .uint_list("ms", 4, &self.list(|c| c.mix_speed))
            .uint_list("mh", 4, &self.list(|c| c.mix_surface_follow))
            .uint_list("gi", 3, &self.list(|c| c.limit_curve_index))
            .flag("gj", self.tadm_algorithm)
            .flag("gk", self.tadm_recording)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `KY` — position one channel in Y.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionChannelY {
    channel_number: u32,
    y: u32,
}

impl PositionChannelY {
    /// `channel` is 0-based; the wire parameter `pn` is 1-based.
    pub fn new(channel: usize, y: u32) -> Result<PositionChannelY, CommandError> {
        check_range(
            "pn",
            "pipetting channel number",
            "",
            channel as f64 + 1.0,
            1.0,
            16.0,
        )?;
        check_range(
            "yj",
            "channel Y position",
            "0.1 mm",
            f64::from(y),
            0.0,
            6500.0,
        )?;
        Ok(PositionChannelY {
            channel_number: channel as u32 + 1,
            y,
        })
    }
}

impl Command for PositionChannelY {
    const CODE: &'static str = "KY";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("pn", 2, self.channel_number)
            .uint("yj", 4, self.y)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The channel Z ceiling in 0.1 mm. The documentation claims 3600 but the
/// firmware rejects anything above 3347.
pub const CHANNEL_Z_CEILING: u32 = 3347;

/// `KZ` — position one channel in Z. The position refers to the tip point
/// when a tip is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionChannelZ {
    channel_number: u32,
    z: u32,
}

impl PositionChannelZ {
    /// `channel` is 0-based; the wire parameter `pn` is 1-based.
    pub fn new(channel: usize, z: u32) -> Result<PositionChannelZ, CommandError> {
        check_range(
            "pn",
            "pipetting channel number",
            "",
            channel as f64 + 1.0,
            1.0,
            16.0,
        )?;
        check_range(
            "zj",
            "channel Z position",
            "0.1 mm",
            f64::from(z),
            0.0,
            f64::from(CHANNEL_Z_CEILING),
        )?;
        Ok(PositionChannelZ {
            channel_number: channel as u32 + 1,
            z,
        })
    }
}

impl Command for PositionChannelZ {
    const CODE: &'static str = "KZ";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("pn", 2, self.channel_number)
            .uint("zj", 4, self.z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `JE` — spread the channels evenly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpreadChannels;

impl Command for SpreadChannels {
    const CODE: &'static str = "JE";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `JM` — move all channels to one defined position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveAllChannels {
    tip_pattern: bool,
    x: u32,
    y: u32,
    traverse_height: u32,
    end_z: u32,
}

impl MoveAllChannels {
    pub fn new(
        tip_pattern: bool,
        x: u32,
        y: u32,
        traverse_height: u32,
        end_z: u32,
    ) -> Result<MoveAllChannels, CommandError> {
        check_range(
            "xp",
            "channel X position",
            "0.1 mm",
            f64::from(x),
            0.0,
            25000.0,
        )?;
        check_range(
            "yp",
            "channel Y position",
            "0.1 mm",
            f64::from(y),
            0.0,
            6500.0,
        )?;
        check_range(
            "th",
            "minimum traverse height",
            "0.1 mm",
            f64::from(traverse_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "zp",
            "Z at end of command",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        Ok(MoveAllChannels {
            tip_pattern,
            x,
            y,
            traverse_height,
            end_z,
        })
    }
}

impl Command for MoveAllChannels {
    const CODE: &'static str = "JM";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .flag("tm", self.tip_pattern)
            .uint("xp", 5, self.x)
            .uint("yp", 4, self.y)
            .uint("th", 4, self.traverse_height)
            .uint("zp", 4, self.end_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `JP` — position all channels for maximum free Y range around one
/// channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionMaxFreeY {
    channel_number: u32,
}

impl PositionMaxFreeY {
    /// `channel` is 0-based; the wire parameter `pn` is 1-based.
    pub fn new(channel: usize) -> Result<PositionMaxFreeY, CommandError> {
        check_range(
            "pn",
            "pipetting channel number",
            "",
            channel as f64 + 1.0,
            1.0,
            16.0,
        )?;
        Ok(PositionMaxFreeY {
            channel_number: channel as u32 + 1,
        })
    }
}

impl Command for PositionMaxFreeY {
    const CODE: &'static str = "JP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("pn", 2, self.channel_number)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `XL` — search for the Teach-In signal (capacitive LLD in X) with one
/// channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeachInSearchX {
    channel_number: u32,
    x: u32,
}

impl TeachInSearchX {
    /// `channel` is 0-based; the wire parameter `pn` is 1-based.
    pub fn new(channel: usize, x: u32) -> Result<TeachInSearchX, CommandError> {
        check_range(
            "pn",
            "pipetting channel number",
            "",
            channel as f64 + 1.0,
            1.0,
            16.0,
        )?;
        check_range(
            "xs",
            "X search position",
            "0.1 mm",
            f64::from(x),
            0.0,
            30000.0,
        )?;
        Ok(TeachInSearchX {
            channel_number: channel as u32 + 1,
            x,
        })
    }
}

impl Command for TeachInSearchX {
    const CODE: &'static str = "XL";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("pn", 2, self.channel_number)
            .uint("xs", 5, self.x)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `RB` — request one channel's Y position (fmt `rb####`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestChannelY {
    channel_number: u32,
}

impl RequestChannelY {
    /// `channel` is 0-based; the wire parameter `pn` is 1-based.
    pub fn new(channel: usize) -> Result<RequestChannelY, CommandError> {
        check_range(
            "pn",
            "pipetting channel number",
            "",
            channel as f64 + 1.0,
            1.0,
            16.0,
        )?;
        Ok(RequestChannelY {
            channel_number: channel as u32 + 1,
        })
    }
}

impl Command for RequestChannelY {
    const CODE: &'static str = "RB";
    /// The Y position in 0.1 mm.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("pn", 2, self.channel_number)
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("rb", 4)])?;
        Ok(fields.int("rb").unwrap_or(0))
    }
}

/// `RY` — request all channel Y positions (fmt `ry#### (n)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestAllChannelY;

impl Command for RequestAllChannelY {
    const CODE: &'static str = "RY";
    /// One Y position per channel, 0.1 mm.
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int_list("ry", 4)])?;
        Ok(fields
            .int_list("ry")
            .map(<[i64]>::to_vec)
            .unwrap_or_default())
    }
}

/// `RD` — request one channel's tip-bottom Z (fmt `rd####`; a tip must be
/// mounted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestChannelTipZ {
    channel_number: u32,
}

impl RequestChannelTipZ {
    /// `channel` is 0-based; the wire parameter `pn` is 1-based.
    pub fn new(channel: usize) -> Result<RequestChannelTipZ, CommandError> {
        check_range(
            "pn",
            "pipetting channel number",
            "",
            channel as f64 + 1.0,
            1.0,
            16.0,
        )?;
        Ok(RequestChannelTipZ {
            channel_number: channel as u32 + 1,
        })
    }
}

impl Command for RequestChannelTipZ {
    const CODE: &'static str = "RD";
    /// The tip-bottom Z in 0.1 mm.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("pn", 2, self.channel_number)
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("rd", 4)])?;
        Ok(fields.int("rd").unwrap_or(0))
    }
}

/// `RT` — request tip presence (fmt `rt# (n)`). The response length is the
/// machine's channel count, which is how the session discovers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestTipPresence;

impl Command for RequestTipPresence {
    const CODE: &'static str = "RT";
    /// One flag per channel: `true` when a tip is mounted.
    type Response = Vec<bool>;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Vec<bool>, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int_list("rt", 1)])?;
        Ok(fields
            .int_list("rt")
            .map(|values| values.iter().map(|&v| v == 1).collect())
            .unwrap_or_default())
    }
}

/// `RL` — request the last liquid level detection heights (fmt
/// `lh#### (n)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestLastLldHeights;

impl Command for RequestLastLldHeights {
    const CODE: &'static str = "RL";
    /// One height per channel, 0.1 mm.
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int_list("lh", 4)])?;
        Ok(fields
            .int_list("lh")
            .map(<[i64]>::to_vec)
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::CommandId;

    const EIGHT_CHANNELS: usize = 8;

    #[test]
    fn tip_pickup_on_the_first_two_channels_reproduces_the_golden_string() {
        let targets = [
            ChannelTarget {
                channel: 0,
                x: 1179,
                y: 2418,
            },
            ChannelTarget {
                channel: 1,
                x: 1179,
                y: 2328,
            },
        ];
        let command = TipPickup::new(
            &targets,
            EIGHT_CHANNELS,
            1,
            2244,
            2164,
            2450,
            TipPickupMethod::OutOfRack,
        )
        .expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(2)),
            "C0TPid0002xp01179 01179 00000&yp2418 2328 0000&tm1 1 0&tt01tp2244tz2164th2450td0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn tip_pickup_on_middle_channels_pads_the_leading_channels_one_hot() {
        let targets = [
            ChannelTarget {
                channel: 4,
                x: 1179,
                y: 2058,
            },
            ChannelTarget {
                channel: 5,
                x: 1179,
                y: 1968,
            },
        ];
        let command = TipPickup::new(
            &targets,
            EIGHT_CHANNELS,
            1,
            2244,
            2164,
            2450,
            TipPickupMethod::OutOfRack,
        )
        .expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(2)),
            "C0TPid0002xp00000 00000 00000 00000 01179 01179 00000&yp0000 0000 0000 0000 2058 1968 0000&tm0 0 0 0 1 1 0&tt01tp2244tz2164th2450td0",
            "unused leading channels carry syntactically valid zero placeholders"
        );
    }

    #[test]
    fn tip_discard_with_the_drop_method_reproduces_the_golden_string() {
        let targets = [
            ChannelTarget {
                channel: 4,
                x: 1179,
                y: 2058,
            },
            ChannelTarget {
                channel: 5,
                x: 1179,
                y: 1968,
            },
        ];
        let command = TipDiscard::new(
            &targets,
            EIGHT_CHANNELS,
            1314,
            1414,
            2450,
            2450,
            TipDiscardMethod::Drop,
        )
        .expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(3)),
            "C0TRid0003xp00000 00000 00000 00000 01179 01179 00000&yp0000 0000 0000 0000 2058 1968 0000&tm0 0 0 0 1 1 0&tp1314tz1414th2450te2450ti1",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn a_single_channel_aspirate_reproduces_the_golden_string() {
        // 100 µL of water through the standard-volume filter liquid class:
        // the correction curve commands 107.2 µL (av01072). The liquid
        // surface sits at 186.6 mm.
        let mut channel = AspirateChannel::at(0, 2983, 1457);
        channel.lld_search_height = 2000;
        channel.liquid_surface = 1866;
        channel.minimum_height = 1866;
        channel.volume = 1072;
        channel.swap_speed = 20;
        channel.settling_time = 10;
        let command = Aspirate::new(vec![channel], EIGHT_CHANNELS, 2450, 2450)
            .expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "C0ASid0001at0 0&tm1 0&xp02983 00000&yp1457 0000&th2450te2450lp2000 2000&ch000 000&zl1866 1866&po0100 0100&zu0032 0032&zr06180 06180&zx1866 1866&ip0000 0000&it0 0&fp0000 0000&av01072 01072&as1000 1000&ta000 000&ba0000 0000&oa000 000&lm0 0&ll1 1&lv1 1&zo000 000&ld00 00&de0020 0020&wt10 10&mv00000 00000&mc00 00&mp000 000&ms1000 1000&mh0000 0000&gi000 000&gj0gk0lk0 0&ik0000 0000&sd0500 0500&se0500 0500&sz0300 0300&io0000 0000&",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn a_blow_out_jet_dispense_reproduces_the_golden_string() {
        let mut channel = DispenseChannel::at(0, 2983, 1457);
        channel.mode = DispenseMode::BlowOutJet;
        channel.minimum_height = 1866;
        channel.lld_search_height = 2000;
        channel.liquid_surface = 1866;
        channel.volume = 1072;
        channel.speed = 1800;
        channel.transport_air = 50;
        channel.blow_out_air = 300;
        channel.swap_speed = 10;
        let command = Dispense::new(vec![channel], EIGHT_CHANNELS, 2450, 2450, 0)
            .expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "C0DSid0001dm1 1&tm1 0&xp02983 00000&yp1457 0000&zx1866 1866&lp2000 2000&zl1866 1866&po0100 0100&ip0000 0000&it0 0&fp0000 0000&zu0032 0032&zr06180 06180&th2450te2450dv01072 01072&ds1800 1800&ss0050 0050&rv000 000&ta050 050&ba0300 0300&lm0 0&dj00zo000 000&ll1 1&lv1 1&de0010 0010&wt00 00&mv00000 00000&mc00 00&mp000 000&ms0010 0010&mh0000 0000&gi000 000&gj0gk0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn the_channel_z_ceiling_is_the_empirical_3347() {
        let error = PositionChannelZ::new(0, 3400).expect_err("3400 exceeds the empirical ceiling");
        assert!(
            error.to_string().contains("3347"),
            "the error names the real ceiling, not the documented 3600: {error}"
        );
        assert!(
            PositionChannelZ::new(0, 3347).is_ok(),
            "3347 itself is accepted"
        );
    }

    #[test]
    fn aspirate_speed_zero_is_rejected() {
        let mut channel = AspirateChannel::at(0, 2983, 1457);
        channel.speed = 0;
        let error = Aspirate::new(vec![channel], EIGHT_CHANNELS, 2450, 2450)
            .expect_err("speed 0 is below the firmware's floor of 4");
        assert!(
            error.to_string().contains("as") && error.to_string().contains('4'),
            "the error names the parameter and floor: {error}"
        );
    }

    #[test]
    fn aspirate_volume_beyond_12500_is_rejected() {
        let mut channel = AspirateChannel::at(0, 2983, 1457);
        channel.volume = 12501;
        let error = Aspirate::new(vec![channel], EIGHT_CHANNELS, 2450, 2450)
            .expect_err("12501 tenth-µL exceeds the ceiling");
        assert!(
            error.to_string().contains("av") && error.to_string().contains("12500"),
            "the error names the parameter and ceiling: {error}"
        );
    }

    #[test]
    fn tip_presence_length_is_the_channel_count() {
        let presence = RequestTipPresence::parse_response("rt1 0 1 1 0 0 0 0")
            .expect("a well-formed RT reply parses");
        assert_eq!(presence.len(), 8, "the reply carries one flag per channel");
        assert_eq!(
            presence,
            vec![true, false, true, true, false, false, false, false],
            "each flag reports whether a tip is mounted"
        );
    }

    #[test]
    fn last_lld_heights_parse_per_channel() {
        let heights = RequestLastLldHeights::parse_response("lh1866 0000 1902 0000")
            .expect("a well-formed RL reply parses");
        assert_eq!(
            heights,
            vec![1866, 0, 1902, 0],
            "heights are 0.1 mm per channel"
        );
    }

    #[test]
    fn all_channel_y_positions_parse_per_channel() {
        let positions = RequestAllChannelY::parse_response("ry2418 2328 0060 0060")
            .expect("a well-formed RY reply parses");
        assert_eq!(
            positions,
            vec![2418, 2328, 60, 60],
            "one Y position per channel, in 0.1 mm"
        );
    }
}
