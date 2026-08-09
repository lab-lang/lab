//! CoRe 96 head commands: master-level (`C0`) liquid handling and
//! head-direct (`H0`) drive control. Encode-only: the structs produce
//! verified wire strings but the session offers no choreography for them.
//!
//! Master-level X positions are magnitude plus direction (`xs`/`xd`, with
//! `xd1` marking negative X); `H0`-direct commands use motor increments.
//!
//! Known firmware behavior worth planning around:
//! - The master times out slave commands after about five minutes, so a
//!   slow 96-head liquid operation can answer `C0EAid####er99/00 H002/11`
//!   while the head is still working. The recovery is to poll `C0 EV` until
//!   it stops being rejected with `H0 01/40`.
//! - `H0 PI` drops tips in place and leaves the dispensing drive at
//!   215.92 µL; after a tip pickup it sits at 218.19 µL. Move the drive to
//!   218.19 µL before an out-of-rack pickup.
//! - `H0 DL` (dispensing-drive home) is broken firmware: it cannot reach
//!   0 µL and always errors. This crate deliberately does not provide it.

use crate::commands::Command;
use crate::commands::system::TipPickupMethod;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::{FieldSpec, ResponseParseError, parse_fields};

/// The legal window for the head's A1 position, in millimeters. The values
/// here are the measured STAR window; the STARlet window is unknown, so the
/// check stays parameterized instead of hard-coding one machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Head96PositionWindow {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub z_min: f64,
    pub z_max: f64,
}

/// The measured STAR window for the head's A1 position.
pub const STAR_HEAD96_WINDOW: Head96PositionWindow = Head96PositionWindow {
    x_min: -271.0,
    x_max: 974.0,
    y_min: 108.0,
    y_max: 560.0,
    z_min: 180.5,
    z_max: 342.5,
};

impl Head96PositionWindow {
    /// Checks an A1 target in millimeters against this window.
    pub fn check(&self, x: f64, y: f64, z: f64) -> Result<(), CommandError> {
        check_range(
            "xs",
            "96-head A1 X position",
            "mm",
            x,
            self.x_min,
            self.x_max,
        )?;
        check_range(
            "yh",
            "96-head A1 Y position",
            "mm",
            y,
            self.y_min,
            self.y_max,
        )?;
        check_range(
            "za",
            "96-head A1 Z position",
            "mm",
            z,
            self.z_min,
            self.z_max,
        )?;
        Ok(())
    }
}

/// The head footprint: 11 columns and 7 rows of 9 mm pitch. The A1 target
/// for centering the head on a resource is the resource position plus
/// (resource size − head size)/2 plus 4.5 mm on each axis.
pub fn head96_a1_target(
    resource_x: f64,
    resource_y: f64,
    resource_size_x: f64,
    resource_size_y: f64,
) -> (f64, f64) {
    const HEAD_SIZE_X: f64 = 11.0 * 9.0;
    const HEAD_SIZE_Y: f64 = 7.0 * 9.0;
    (
        resource_x + (resource_size_x - HEAD_SIZE_X) / 2.0 + 4.5,
        resource_y + (resource_size_y - HEAD_SIZE_Y) / 2.0 + 4.5,
    )
}

/// A master-level 96-head X coordinate: magnitude in 0.1 mm plus direction
/// (`xd1` = negative X).
fn split_x(x: i32) -> (u32, bool) {
    (x.unsigned_abs(), x < 0)
}

/// `EI` — initialize the 96 head over the trash (60 s timeout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96Initialize {
    /// A1 X, 0.1 mm, signed.
    pub x: i32,
    /// `yh`: A1 Y, 0.1 mm.
    pub y: u32,
    /// `za`: initialization Z, 0.1 mm.
    pub z: u32,
    /// `ze`: Z at end of command, 0.1 mm.
    pub end_z: u32,
}

impl Command for Head96Initialize {
    const CODE: &'static str = "EI";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = split_x(self.x);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yh", 4, self.y)
            .uint("za", 4, self.z)
            .uint("ze", 4, self.end_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `EP` — 96-head tip pickup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96TipPickup {
    x: i32,
    y: u32,
    tip_type: u32,
    method: TipPickupMethod,
    begin_z: u32,
    traverse_height: u32,
    end_z: u32,
}

impl Head96TipPickup {
    /// `begin_z` (`za`) is where the pickup starts, at most 342.5 mm.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        x: i32,
        y: u32,
        tip_type: u32,
        method: TipPickupMethod,
        begin_z: u32,
        traverse_height: u32,
        end_z: u32,
    ) -> Result<Head96TipPickup, CommandError> {
        check_range(
            "yh",
            "96-head A1 Y position",
            "0.1 mm",
            f64::from(y),
            1080.0,
            5600.0,
        )?;
        check_range("tt", "tip type index", "", f64::from(tip_type), 0.0, 99.0)?;
        check_range(
            "za",
            "begin of tip pickup Z",
            "0.1 mm",
            f64::from(begin_z),
            0.0,
            3425.0,
        )?;
        check_range(
            "zh",
            "minimum traverse height",
            "0.1 mm",
            f64::from(traverse_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "ze",
            "Z at end of command",
            "0.1 mm",
            f64::from(end_z),
            0.0,
            3600.0,
        )?;
        Ok(Head96TipPickup {
            x,
            y,
            tip_type,
            method,
            begin_z,
            traverse_height,
            end_z,
        })
    }
}

impl Command for Head96TipPickup {
    const CODE: &'static str = "EP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = split_x(self.x);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yh", 4, self.y)
            .uint("tt", 2, self.tip_type)
            .uint("wu", 1, self.method.code())
            .uint("za", 4, self.begin_z)
            .uint("zh", 4, self.traverse_height)
            .uint("ze", 4, self.end_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ER` — 96-head tip discard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96TipDiscard {
    /// A1 X, 0.1 mm, signed.
    pub x: i32,
    /// `yh`: A1 Y, 0.1 mm.
    pub y: u32,
    /// `za`: deposit Z, 0.1 mm.
    pub begin_z: u32,
    /// `zh`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `ze`: Z at end of command, 0.1 mm.
    pub end_z: u32,
}

impl Command for Head96TipDiscard {
    const CODE: &'static str = "ER";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = split_x(self.x);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yh", 4, self.y)
            .uint("za", 4, self.begin_z)
            .uint("zh", 4, self.traverse_height)
            .uint("ze", 4, self.end_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The 96-well pattern (`cw`): 24 uppercase hex characters, bit 0 = well A1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WellPattern {
    bits: Vec<bool>,
}

impl WellPattern {
    /// All 96 wells.
    pub fn all() -> WellPattern {
        WellPattern {
            bits: vec![true; 96],
        }
    }

    /// A pattern from 96 flags, well A1 first.
    pub fn from_wells(bits: Vec<bool>) -> Result<WellPattern, CommandError> {
        if bits.len() != 96 {
            return Err(CommandError::InconsistentChannels {
                message: format!(
                    "a 96-head well pattern needs exactly 96 flags, got {}",
                    bits.len()
                ),
            });
        }
        Ok(WellPattern { bits })
    }

    pub fn bits(&self) -> &[bool] {
        &self.bits
    }
}

/// `EA` — 96-head aspirate (300 s timeout). All values in wire units:
/// 0.1 mm, 0.1 µL, 0.1 µL/s, 0.1 s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head96Aspirate {
    /// `aa`: aspiration type (0 simple).
    pub aspiration_type: u32,
    /// A1 X, 0.1 mm, signed.
    pub x: i32,
    /// `yh`: A1 Y, 0.1 mm.
    pub y: u32,
    /// `zh`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `ze`: Z at end of command, 0.1 mm.
    pub end_z: u32,
    /// `lz`: LLD search height, 0.1 mm.
    pub lld_search_height: u32,
    /// `zt`: liquid surface when LLD is off, 0.1 mm.
    pub liquid_surface: u32,
    /// `pp`: transport-air pull-out distance, 0.1 mm.
    pub pull_out_distance: u32,
    /// `zm`: minimum height (maximum immersion), 0.1 mm.
    pub minimum_height: u32,
    /// `zv`: second-section height, 0.1 mm.
    pub second_section_height: u32,
    /// `zq`: second-section ratio.
    pub second_section_ratio: u32,
    /// `iw`: immersion depth, 0.1 mm.
    pub immersion_depth: u32,
    /// `ix`: immersion direction, `false` = down.
    pub immersion_direction_up: bool,
    /// `fh`: surface-following distance, 0.1 mm.
    pub surface_following_distance: u32,
    /// `af`: aspirate volume, 0.1 µL, 0–11500.
    pub volume: u32,
    /// `ag`: aspirate speed, 0.1 µL/s, 3–5000.
    pub speed: u32,
    /// `vt`: transport air volume, 0.1 µL.
    pub transport_air: u32,
    /// `bv`: blow-out air volume, 0.1 µL.
    pub blow_out_air: u32,
    /// `wv`: pre-wetting volume, 0.1 µL.
    pub pre_wetting: u32,
    /// `cm`: LLD mode.
    pub lld_mode: u32,
    /// `cs`: gamma LLD sensitivity, 1–4.
    pub gamma_lld_sensitivity: u32,
    /// `bs`: swap speed, 0.1 mm/s.
    pub swap_speed: u32,
    /// `wh`: settling time, 0.1 s.
    pub settling_time: u32,
    /// `hv`: mix volume, 0.1 µL.
    pub mix_volume: u32,
    /// `hc`: mix cycles.
    pub mix_cycles: u32,
    /// `hp`: mix position from surface, 0.1 mm.
    pub mix_position: u32,
    /// `mj`: mix surface-following distance, 0.1 mm.
    pub mix_surface_follow: u32,
    /// `hs`: mix speed, 0.1 µL/s.
    pub mix_speed: u32,
    /// `cw`: the 96-well pattern.
    pub pattern: WellPattern,
    /// `cr`: TADM limit-curve index.
    pub limit_curve_index: u32,
    /// `cj`: TADM algorithm flag.
    pub tadm_algorithm: bool,
    /// `cx`: TADM recording mode flag.
    pub tadm_recording: bool,
}

impl Head96Aspirate {
    /// An aspirate with every documented default; set the target, surface,
    /// and volume before use.
    pub fn at(x: i32, y: u32) -> Head96Aspirate {
        Head96Aspirate {
            aspiration_type: 0,
            x,
            y,
            traverse_height: 2450,
            end_z: 2450,
            lld_search_height: 0,
            liquid_surface: 0,
            pull_out_distance: 100,
            minimum_height: 0,
            second_section_height: 32,
            second_section_ratio: 6180,
            immersion_depth: 0,
            immersion_direction_up: false,
            surface_following_distance: 0,
            volume: 0,
            speed: 2500,
            transport_air: 50,
            blow_out_air: 0,
            pre_wetting: 50,
            lld_mode: 0,
            gamma_lld_sensitivity: 1,
            swap_speed: 20,
            settling_time: 10,
            mix_volume: 0,
            mix_cycles: 0,
            mix_position: 0,
            mix_surface_follow: 0,
            mix_speed: 1200,
            pattern: WellPattern::all(),
            limit_curve_index: 0,
            tadm_algorithm: false,
            tadm_recording: false,
        }
    }

    /// Validates the documented ranges.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "af",
            "96-head aspirate volume",
            "0.1 µL",
            f64::from(self.volume),
            0.0,
            11500.0,
        )?;
        check_range(
            "ag",
            "96-head aspirate speed",
            "0.1 µL/s",
            f64::from(self.speed),
            3.0,
            5000.0,
        )?;
        Ok(())
    }
}

impl Command for Head96Aspirate {
    const CODE: &'static str = "EA";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = split_x(self.x);
        builder
            .uint("aa", 1, self.aspiration_type)
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yh", 4, self.y)
            .uint("zh", 4, self.traverse_height)
            .uint("ze", 4, self.end_z)
            .uint("lz", 4, self.lld_search_height)
            .uint("zt", 4, self.liquid_surface)
            .uint("pp", 4, self.pull_out_distance)
            .uint("zm", 4, self.minimum_height)
            .uint("zv", 4, self.second_section_height)
            .uint("zq", 5, self.second_section_ratio)
            .uint("iw", 3, self.immersion_depth)
            .flag("ix", self.immersion_direction_up)
            .uint("fh", 3, self.surface_following_distance)
            .uint("af", 5, self.volume)
            .uint("ag", 4, self.speed)
            .uint("vt", 3, self.transport_air)
            .uint("bv", 5, self.blow_out_air)
            .uint("wv", 5, self.pre_wetting)
            .uint("cm", 1, self.lld_mode)
            .uint("cs", 1, self.gamma_lld_sensitivity)
            .uint("bs", 4, self.swap_speed)
            .uint("wh", 2, self.settling_time)
            .uint("hv", 5, self.mix_volume)
            .uint("hc", 2, self.mix_cycles)
            .uint("hp", 3, self.mix_position)
            .uint("mj", 3, self.mix_surface_follow)
            .uint("hs", 4, self.mix_speed)
            .hex_mask("cw", 24, self.pattern.bits())
            .uint("cr", 3, self.limit_curve_index)
            .flag("cj", self.tadm_algorithm)
            .flag("cx", self.tadm_recording)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ED` — 96-head dispense (300 s timeout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head96Dispense {
    /// `da`: dispense mode (0 partial jet, 1 blow-out jet, 2 partial
    /// surface, 3 blow-out surface).
    pub mode: u32,
    /// A1 X, 0.1 mm, signed.
    pub x: i32,
    /// `yh`: A1 Y, 0.1 mm.
    pub y: u32,
    /// `zm`: minimum height, 0.1 mm.
    pub minimum_height: u32,
    /// `zv`: second-section height, 0.1 mm.
    pub second_section_height: u32,
    /// `zq`: second-section ratio.
    pub second_section_ratio: u32,
    /// `lz`: LLD search height, 0.1 mm.
    pub lld_search_height: u32,
    /// `zt`: liquid surface when LLD is off, 0.1 mm.
    pub liquid_surface: u32,
    /// `pp`: transport-air pull-out distance, 0.1 mm.
    pub pull_out_distance: u32,
    /// `iw`: immersion depth, 0.1 mm.
    pub immersion_depth: u32,
    /// `ix`: immersion direction, `false` = down.
    pub immersion_direction_up: bool,
    /// `fh`: surface-following distance, 0.1 mm.
    pub surface_following_distance: u32,
    /// `zh`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `ze`: Z at end of command, 0.1 mm.
    pub end_z: u32,
    /// `df`: dispense volume, 0.1 µL, 0–11500.
    pub volume: u32,
    /// `dg`: dispense speed, 0.1 µL/s, 3–5000.
    pub speed: u32,
    /// `es`: cut-off speed, 0.1 µL/s.
    pub cut_off_speed: u32,
    /// `ev`: stop-back volume, 0.1 µL.
    pub stop_back_volume: u32,
    /// `vt`: transport air volume, 0.1 µL.
    pub transport_air: u32,
    /// `bv`: blow-out air volume, 0.1 µL.
    pub blow_out_air: u32,
    /// `cm`: LLD mode.
    pub lld_mode: u32,
    /// `cs`: gamma LLD sensitivity, 1–4.
    pub gamma_lld_sensitivity: u32,
    /// `ej`: side touch-off distance, 0.1 mm.
    pub side_touch_off_distance: u32,
    /// `bs`: swap speed, 0.1 mm/s.
    pub swap_speed: u32,
    /// `wh`: settling time, 0.1 s.
    pub settling_time: u32,
    /// `hv`: mix volume, 0.1 µL.
    pub mix_volume: u32,
    /// `hc`: mix cycles.
    pub mix_cycles: u32,
    /// `hp`: mix position from surface, 0.1 mm.
    pub mix_position: u32,
    /// `mj`: mix surface-following distance, 0.1 mm.
    pub mix_surface_follow: u32,
    /// `hs`: mix speed, 0.1 µL/s.
    pub mix_speed: u32,
    /// `cw`: the 96-well pattern.
    pub pattern: WellPattern,
    /// `cr`: TADM limit-curve index.
    pub limit_curve_index: u32,
    /// `cj`: TADM algorithm flag.
    pub tadm_algorithm: bool,
    /// `cx`: TADM recording mode flag.
    pub tadm_recording: bool,
}

impl Head96Dispense {
    /// A dispense with every documented default; set the target, surface,
    /// mode, and volume before use.
    pub fn at(x: i32, y: u32) -> Head96Dispense {
        Head96Dispense {
            mode: 3,
            x,
            y,
            minimum_height: 0,
            second_section_height: 32,
            second_section_ratio: 6180,
            lld_search_height: 0,
            liquid_surface: 0,
            pull_out_distance: 100,
            immersion_depth: 0,
            immersion_direction_up: false,
            surface_following_distance: 0,
            traverse_height: 2450,
            end_z: 2450,
            volume: 0,
            speed: 1200,
            cut_off_speed: 50,
            stop_back_volume: 0,
            transport_air: 50,
            blow_out_air: 0,
            lld_mode: 0,
            gamma_lld_sensitivity: 1,
            side_touch_off_distance: 0,
            swap_speed: 20,
            settling_time: 0,
            mix_volume: 0,
            mix_cycles: 0,
            mix_position: 0,
            mix_surface_follow: 0,
            mix_speed: 1200,
            pattern: WellPattern::all(),
            limit_curve_index: 0,
            tadm_algorithm: false,
            tadm_recording: false,
        }
    }

    /// Validates the documented ranges.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "df",
            "96-head dispense volume",
            "0.1 µL",
            f64::from(self.volume),
            0.0,
            11500.0,
        )?;
        check_range(
            "dg",
            "96-head dispense speed",
            "0.1 µL/s",
            f64::from(self.speed),
            3.0,
            5000.0,
        )?;
        Ok(())
    }
}

impl Command for Head96Dispense {
    const CODE: &'static str = "ED";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = split_x(self.x);
        builder
            .uint("da", 1, self.mode)
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yh", 4, self.y)
            .uint("zm", 4, self.minimum_height)
            .uint("zv", 4, self.second_section_height)
            .uint("zq", 5, self.second_section_ratio)
            .uint("lz", 4, self.lld_search_height)
            .uint("zt", 4, self.liquid_surface)
            .uint("pp", 4, self.pull_out_distance)
            .uint("iw", 3, self.immersion_depth)
            .flag("ix", self.immersion_direction_up)
            .uint("fh", 3, self.surface_following_distance)
            .uint("zh", 4, self.traverse_height)
            .uint("ze", 4, self.end_z)
            .uint("df", 5, self.volume)
            .uint("dg", 4, self.speed)
            .uint("es", 4, self.cut_off_speed)
            .uint("ev", 3, self.stop_back_volume)
            .uint("vt", 3, self.transport_air)
            .uint("bv", 5, self.blow_out_air)
            .uint("cm", 1, self.lld_mode)
            .uint("cs", 1, self.gamma_lld_sensitivity)
            .uint("ej", 2, self.side_touch_off_distance)
            .uint("bs", 4, self.swap_speed)
            .uint("wh", 2, self.settling_time)
            .uint("hv", 5, self.mix_volume)
            .uint("hc", 2, self.mix_cycles)
            .uint("hp", 3, self.mix_position)
            .uint("mj", 3, self.mix_surface_follow)
            .uint("hs", 4, self.mix_speed)
            .hex_mask("cw", 24, self.pattern.bits())
            .uint("cr", 3, self.limit_curve_index)
            .flag("cj", self.tadm_algorithm)
            .flag("cx", self.tadm_recording)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `EM` — move the 96 head to an A1 coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96Move {
    /// A1 X, 0.1 mm, signed.
    pub x: i32,
    /// `yh`: A1 Y, 0.1 mm.
    pub y: u32,
    /// `za`: A1 Z, 0.1 mm.
    pub z: u32,
    /// `zh`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
}

impl Command for Head96Move {
    const CODE: &'static str = "EM";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = split_x(self.x);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yh", 4, self.y)
            .uint("za", 4, self.z)
            .uint("zh", 4, self.traverse_height)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `EV` — move the 96 head to Z-safety (20 s timeout). Also the polling
/// command for recovering from the five-minute master timeout during a slow
/// head liquid operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96MoveToZSafety;

impl Command for Head96MoveToZSafety {
    const CODE: &'static str = "EV";
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

/// `QH` — query 96-head tip presence (fmt `qh#`). This is the firmware's
/// belief, not a sensor reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96QueryTipPresence;

impl Command for Head96QueryTipPresence {
    const CODE: &'static str = "QH";
    type Response = bool;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qh", 1)])?;
        Ok(fields.int("qh") == Some(1))
    }
}

/// The 96 head's position (`QI` reply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96Position {
    /// A1 X, 0.1 mm, signed.
    pub x: i64,
    /// A1 Y, 0.1 mm.
    pub y: i64,
    /// A1 Z, 0.1 mm.
    pub z: i64,
}

/// `QI` — query the 96 head's position (fmt `xs#####xd#yh####za####`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96QueryPosition;

impl Command for Head96QueryPosition {
    const CODE: &'static str = "QI";
    type Response = Head96Position;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Head96Position, ResponseParseError> {
        let fields = parse_fields(
            payload,
            &[
                FieldSpec::int("xs", 5),
                FieldSpec::int("xd", 1),
                FieldSpec::int("yh", 4),
                FieldSpec::int("za", 4),
            ],
        )?;
        let magnitude = fields.int("xs").unwrap_or(0);
        let x = if fields.int("xd") == Some(1) {
            -magnitude
        } else {
            magnitude
        };
        Ok(Head96Position {
            x,
            y: fields.int("yh").unwrap_or(0),
            z: fields.int("za").unwrap_or(0),
        })
    }
}

/// The 96 head types of the `H0 QG` reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head96Type {
    /// Code 0.
    LowVolume,
    /// Code 1.
    HighVolume,
    /// Code 2.
    HeadII,
    /// Code 3.
    Tadm,
    Unknown(u8),
}

/// `QG` on `H0` — query the head type (fmt `qg#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96QueryType;

impl Command for Head96QueryType {
    const CODE: &'static str = "QG";
    type Response = Head96Type;

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Head96Type, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qg", 1)])?;
        Ok(match fields.int("qg").unwrap_or(-1) {
            0 => Head96Type::LowVolume,
            1 => Head96Type::HighVolume,
            2 => Head96Type::HeadII,
            3 => Head96Type::Tadm,
            other => Head96Type::Unknown(other.clamp(0, 255) as u8),
        })
    }
}

/// `QU` on `H0` — query the head's device information, as raw text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96QueryInfo;

impl Command for Head96QueryInfo {
    const CODE: &'static str = "QU";
    type Response = String;

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.trim().to_string())
    }
}

/// `PI` on `H0` — initialize the head's drives. Drops any tips in place and
/// leaves the dispensing drive at 215.92 µL; move the drive to 218.19 µL
/// before an out-of-rack tip pickup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96DriveInitialize {
    /// `sv`: squeezer speed, increments/s.
    pub squeezer_speed: u32,
    /// `sr`: squeezer acceleration, increments/s².
    pub squeezer_acceleration: u32,
    /// `sw`: squeezer current limit, 0–15.
    pub squeezer_current_limit: u32,
    /// `dw`: dispensing-drive current limit, 0–15.
    pub dispensing_drive_current_limit: u32,
}

impl Command for Head96DriveInitialize {
    const CODE: &'static str = "PI";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("sv", 5, self.squeezer_speed)
            .uint("sr", 6, self.squeezer_acceleration)
            .uint("sw", 2, self.squeezer_current_limit)
            .uint("dw", 2, self.dispensing_drive_current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PA` on `H0` — direct aspirate in motor increments (dispensing drive:
/// 0.019340933 µL per increment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head96DirectAspirate {
    /// `pm`: the 96-well pattern.
    pub pattern: WellPattern,
    /// `dj`: enforce the minimum height.
    pub enforce_minimum_height: bool,
    /// `da`: aspirate volume, dispensing-drive increments.
    pub volume: u32,
    /// `dv`: flow rate, dispensing-drive increments/s.
    pub flow_rate: u32,
    /// `dc`: pre-wetting volume, dispensing-drive increments.
    pub pre_wetting: u32,
    /// `zd`: surface-following distance, Z-drive increments.
    pub surface_following_distance: u32,
    /// `zh`: minimum stop-disk height, Z-drive increments.
    pub minimum_height: u32,
    /// `to`: settling time, 0.1 s.
    pub settling_time: u32,
}

impl Command for Head96DirectAspirate {
    const CODE: &'static str = "PA";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .hex_mask("pm", 24, self.pattern.bits())
            .flag("dj", self.enforce_minimum_height)
            .uint("da", 5, self.volume)
            .uint("dv", 5, self.flow_rate)
            .uint("dc", 5, self.pre_wetting)
            .uint("zd", 4, self.surface_following_distance)
            .uint("zh", 5, self.minimum_height)
            .uint("to", 3, self.settling_time)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PB` on `H0` — direct dispense in motor increments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head96DirectDispense {
    /// `pm`: the 96-well pattern.
    pub pattern: WellPattern,
    /// `db`: dispense volume, dispensing-drive increments.
    pub volume: u32,
    /// `dv`: flow rate, dispensing-drive increments/s.
    pub flow_rate: u32,
    /// `dd`: stop-back volume, dispensing-drive increments.
    pub stop_back_volume: u32,
    /// `ze`: surface-following distance, Z-drive increments.
    pub surface_following_distance: u32,
    /// `zh`: minimum stop-disk height, Z-drive increments.
    pub minimum_height: u32,
    /// `du`: stop flow rate, dispensing-drive increments/s.
    pub stop_flow_rate: u32,
}

impl Command for Head96DirectDispense {
    const CODE: &'static str = "PB";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .hex_mask("pm", 24, self.pattern.bits())
            .uint("db", 5, self.volume)
            .uint("dv", 5, self.flow_rate)
            .uint("dd", 4, self.stop_back_volume)
            .uint("ze", 4, self.surface_following_distance)
            .uint("zh", 5, self.minimum_height)
            .uint("du", 5, self.stop_flow_rate)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `DQ` on `H0` — move the dispensing drive to an absolute position in
/// increments. The route to the 218.19 µL pre-pickup position `PI` leaves
/// unreached (the firmware's own `DL` home is broken and must never be
/// sent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96DispensingDriveMove {
    /// `dq`: target position, increments.
    pub position: u32,
    /// `dv`: speed, increments/s.
    pub speed: u32,
    /// `du`: stop speed, increments/s.
    pub stop_speed: u32,
    /// `dr`: acceleration, increments/s².
    pub acceleration: u32,
    /// `dw`: current protection limiter, 0–15.
    pub current_limit: u32,
}

impl Command for Head96DispensingDriveMove {
    const CODE: &'static str = "DQ";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("dq", 5, self.position)
            .uint("dv", 5, self.speed)
            .uint("du", 5, self.stop_speed)
            .uint("dr", 6, self.acceleration)
            .uint("dw", 2, self.current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `YA` on `H0` — absolute Y move in increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96YMove {
    /// `ya`: target position, increments.
    pub position: u32,
    /// `yv`: speed, increments/s.
    pub speed: u32,
    /// `yr`: acceleration, increments/s².
    pub acceleration: u32,
    /// `yw`: current protection limiter, 0–15.
    pub current_limit: u32,
}

impl Command for Head96YMove {
    const CODE: &'static str = "YA";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("ya", 5, self.position)
            .uint("yv", 5, self.speed)
            .uint("yr", 6, self.acceleration)
            .uint("yw", 2, self.current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZA` on `H0` — absolute Z move in increments (legacy window 36100–68500,
/// FM-STAR 24200–76200).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head96ZMove {
    /// `za`: target position, increments.
    pub position: u32,
    /// `zv`: speed, increments/s.
    pub speed: u32,
    /// `zr`: acceleration, increments/s².
    pub acceleration: u32,
    /// `zw`: current protection limiter, 0–15.
    pub current_limit: u32,
}

impl Command for Head96ZMove {
    const CODE: &'static str = "ZA";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("za", 5, self.position)
            .uint("zv", 5, self.speed)
            .uint("zr", 6, self.acceleration)
            .uint("zw", 2, self.current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `RZ` on `H0` — request the Z drive counters (fmt `rz##### (n)`).
/// Element 0 is the firmware counter and element 1 the hardware counter;
/// use the hardware counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96RequestZ;

impl Command for Head96RequestZ {
    const CODE: &'static str = "RZ";
    /// The hardware Z counter in increments.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int_list("rz", 5)])?;
        let values = fields.int_list("rz").unwrap_or(&[]);
        // The hardware counter is authoritative; the firmware counter
        // drifts.
        Ok(values
            .get(1)
            .or_else(|| values.first())
            .copied()
            .unwrap_or(0))
    }
}

/// `RH` on `H0` — request the last capacitive LLD height (fmt `rh#####`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96RequestLastLldHeight;

impl Command for Head96RequestLastLldHeight {
    const CODE: &'static str = "RH";
    /// The height in increments.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("rh", 5)])?;
        Ok(fields.int("rh").unwrap_or(0))
    }
}

/// `RD` on `H0` — request the dispensing drive position (fmt `rd#####`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96RequestDrivePosition;

impl Command for Head96RequestDrivePosition {
    const CODE: &'static str = "RD";
    /// The drive position in increments.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("rd", 5)])?;
        Ok(fields.int("rd").unwrap_or(0))
    }
}

/// `MO` on `H0` — park the head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Head96Park;

impl Command for Head96Park {
    const CODE: &'static str = "MO";
    type Response = ();

    fn module(&self) -> Module {
        Module::Head96
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::CommandId;

    #[test]
    fn head_tip_pickup_reproduces_the_golden_string() {
        let command =
            Head96TipPickup::new(1179, 2418, 1, TipPickupMethod::OutOfRack, 2164, 2450, 2450)
                .expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(3)),
            "C0EPid0003xs01179xd0yh2418tt01wu0za2164zh2450ze2450",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn head_tip_discard_over_waste_encodes_the_negative_x_direction() {
        let command = Head96TipDiscard {
            x: -420,
            y: 1203,
            begin_z: 2164,
            traverse_height: 2450,
            end_z: 2450,
        };
        assert_eq!(
            command.to_wire(CommandId::new(4)),
            "C0ERid0004xs00420xd1yh1203za2164zh2450ze2450",
            "xd1 marks a negative X; the magnitude stays five digits"
        );
    }

    #[test]
    fn a_head_aspirate_reproduces_the_golden_string() {
        let mut command = Head96Aspirate::at(2983, 1457);
        command.lld_search_height = 1999;
        command.liquid_surface = 1866;
        command.minimum_height = 1866;
        command.volume = 1083;
        command.settling_time = 10;
        command.validate().expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(4)),
            "C0EAid0004aa0xs02983xd0yh1457zh2450ze2450lz1999zt1866pp0100zm1866zv0032zq06180iw000ix0fh000af01083ag2500vt050bv00000wv00050cm0cs1bs0020wh10hv00000hc00hp000mj000hs1200cwFFFFFFFFFFFFFFFFFFFFFFFFcr000cj0cx0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn a_head_dispense_reproduces_the_golden_string() {
        let mut command = Head96Dispense::at(2983, 1457);
        command.mode = 3;
        command.minimum_height = 1866;
        command.lld_search_height = 1999;
        command.liquid_surface = 1866;
        command.volume = 1083;
        command.validate().expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(5)),
            "C0EDid0005da3xs02983xd0yh1457zm1866zv0032zq06180lz1999zt1866pp0100iw000ix0fh000zh2450ze2450df01083dg1200es0050ev000vt050bv00000cm0cs1ej00bs0020wh00hv00000hc00hp000mj000hs1200cwFFFFFFFFFFFFFFFFFFFFFFFFcr000cj0cx0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn a_direct_aspirate_reproduces_the_golden_string() {
        // 100 µL is 5170 dispensing-drive increments (0.019340933 µL each).
        let command = Head96DirectAspirate {
            pattern: WellPattern::all(),
            enforce_minimum_height: true,
            volume: 5170,
            flow_rate: 2585,
            pre_wetting: 0,
            surface_following_distance: 400,
            minimum_height: 46000,
            settling_time: 0,
        };
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "H0PAid0001pmFFFFFFFFFFFFFFFFFFFFFFFFdj1da05170dv02585dc00000zd0400zh46000to000",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn the_head_position_reply_decodes_the_x_direction_flag() {
        let position = Head96QueryPosition::parse_response("xs00420xd1yh1457za2450")
            .expect("a well-formed QI reply parses");
        assert_eq!(
            position,
            Head96Position {
                x: -420,
                y: 1457,
                z: 2450
            },
            "xd1 negates the five-digit X magnitude"
        );
    }

    #[test]
    fn the_z_counter_reply_prefers_the_hardware_counter() {
        let z =
            Head96RequestZ::parse_response("rz46000 46012").expect("a two-counter reply parses");
        assert_eq!(
            z, 46012,
            "element 1 is the hardware counter, the authoritative one"
        );
    }

    #[test]
    fn head_a1_centering_follows_the_footprint_formula() {
        let (x, y) = head96_a1_target(100.0, 200.0, 127.76, 85.48);
        assert!(
            (x - 118.88).abs() < 1e-9,
            "x = 100 + (127.76 − 99)/2 + 4.5 = 118.88, got {x}"
        );
        assert!(
            (y - 215.74).abs() < 1e-9,
            "y = 200 + (85.48 − 63)/2 + 4.5 = 215.74, got {y}"
        );
    }
}
