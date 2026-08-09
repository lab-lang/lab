//! Slave-direct commands to individual pipetting channels (`P1`–`PG`).
//! These address motors in increments, not 0.1 mm: the conversion constants
//! live in [`crate::units::Axis`]. A `Px`-direct Z position always refers to
//! the stop disk, never the tip point.

use crate::commands::Command;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::{FieldSpec, ResponseParseError, parse_fields};

fn channel_module(channel: usize) -> Result<Module, CommandError> {
    check_range(
        "pn",
        "pipetting channel number",
        "",
        channel as f64 + 1.0,
        1.0,
        16.0,
    )?;
    Ok(Module::PipettingChannel(channel as u8))
}

/// `VY` — request the channel's minimum Y spacing table (fmt `yc### (n)`).
/// Element 1 is the minimum spacing to the neighboring channel in Y-drive
/// increments; the firmware enforces at least 9 mm between adjacent
/// channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestMinimumYSpacing {
    module: Module,
}

impl RequestMinimumYSpacing {
    pub fn new(channel: usize) -> Result<RequestMinimumYSpacing, CommandError> {
        Ok(RequestMinimumYSpacing {
            module: channel_module(channel)?,
        })
    }
}

impl Command for RequestMinimumYSpacing {
    const CODE: &'static str = "VY";
    /// The `yc` values in Y-drive increments.
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int_list("yc", 3)])?;
        Ok(fields
            .int_list("yc")
            .map(<[i64]>::to_vec)
            .unwrap_or_default())
    }
}

/// `RV` — request the channel's cycle counters, as raw text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestCycleCounters {
    module: Module,
}

impl RequestCycleCounters {
    pub fn new(channel: usize) -> Result<RequestCycleCounters, CommandError> {
        Ok(RequestCycleCounters {
            module: channel_module(channel)?,
        })
    }
}

impl Command for RequestCycleCounters {
    const CODE: &'static str = "RV";
    type Response = String;

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.trim().to_string())
    }
}

/// `ZA` on a channel — absolute Z move in increments. The window
/// 9320–31200 increments spans 99.98–334.7 mm of stop-disk height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelZMove {
    module: Module,
    position: u32,
    speed: u32,
    acceleration: u32,
    current_limit: u32,
}

impl ChannelZMove {
    /// `position` in increments 9320–31200, `speed` in increments/s
    /// 20–15000, `acceleration` in thousands of increments/s² 5–150,
    /// `current_limit` 0–7.
    pub fn new(
        channel: usize,
        position: u32,
        speed: u32,
        acceleration: u32,
        current_limit: u32,
    ) -> Result<ChannelZMove, CommandError> {
        let module = channel_module(channel)?;
        check_range(
            "za",
            "absolute Z position",
            "increments",
            f64::from(position),
            9320.0,
            31200.0,
        )?;
        check_range(
            "zv",
            "Z speed",
            "increments/s",
            f64::from(speed),
            20.0,
            15000.0,
        )?;
        check_range(
            "zr",
            "Z acceleration",
            "1000 increments/s²",
            f64::from(acceleration),
            5.0,
            150.0,
        )?;
        check_range(
            "zw",
            "Z current limit",
            "",
            f64::from(current_limit),
            0.0,
            7.0,
        )?;
        Ok(ChannelZMove {
            module,
            position,
            speed,
            acceleration,
            current_limit,
        })
    }
}

impl Command for ChannelZMove {
    const CODE: &'static str = "ZA";
    type Response = ();

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("za", 5, self.position)
            .uint("zv", 5, self.speed)
            .uint("zr", 3, self.acceleration)
            .uint("zw", 1, self.current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `RZ` on a channel — request the stop-disk Z position (fmt `rz######`,
/// increments).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestStopDiskZ {
    module: Module,
}

impl RequestStopDiskZ {
    pub fn new(channel: usize) -> Result<RequestStopDiskZ, CommandError> {
        Ok(RequestStopDiskZ {
            module: channel_module(channel)?,
        })
    }
}

impl Command for RequestStopDiskZ {
    const CODE: &'static str = "RZ";
    /// The stop-disk Z in increments.
    type Response = i64;

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("rz", 6)])?;
        Ok(fields.int("rz").unwrap_or(0))
    }
}

/// `DS` on a channel — relative dispensing-drive move in increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispensingDriveRelativeMove {
    module: Module,
    distance: u32,
    direction_up: bool,
    speed: u32,
    acceleration: u32,
    current_limit: u32,
}

impl DispensingDriveRelativeMove {
    /// `distance` and `speed` in dispensing-drive increments,
    /// `acceleration` in thousands of increments/s², `current_limit` 0–7.
    pub fn new(
        channel: usize,
        distance: u32,
        direction_up: bool,
        speed: u32,
        acceleration: u32,
        current_limit: u32,
    ) -> Result<DispensingDriveRelativeMove, CommandError> {
        let module = channel_module(channel)?;
        check_range(
            "ds",
            "relative dispensing-drive distance",
            "increments",
            f64::from(distance),
            0.0,
            99999.0,
        )?;
        check_range(
            "dv",
            "dispensing-drive speed",
            "increments/s",
            f64::from(speed),
            0.0,
            99999.0,
        )?;
        check_range(
            "dr",
            "dispensing-drive acceleration",
            "1000 increments/s²",
            f64::from(acceleration),
            0.0,
            999.0,
        )?;
        check_range(
            "dw",
            "dispensing-drive current limit",
            "",
            f64::from(current_limit),
            0.0,
            7.0,
        )?;
        Ok(DispensingDriveRelativeMove {
            module,
            distance,
            direction_up,
            speed,
            acceleration,
            current_limit,
        })
    }
}

impl Command for DispensingDriveRelativeMove {
    const CODE: &'static str = "DS";
    type Response = ();

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("ds", 5, self.distance)
            .flag("dt", self.direction_up)
            .uint("dv", 5, self.speed)
            .uint("dr", 3, self.acceleration)
            .uint("dw", 1, self.current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZL` on a channel — capacitive LLD Z search. The detected height is read
/// afterwards with the master-level `RL` query. Runs up to four minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClldZSearch {
    module: Module,
    /// `zh`: lowest immersion position, increments.
    pub lowest_immersion: u32,
    /// `zc`: search start position, increments.
    pub search_start: u32,
    /// `zl`: search speed, increments/s.
    pub speed: u32,
    /// `zr`: acceleration, thousands of increments/s².
    pub acceleration: u32,
    /// `gt`: detection edge steepness, 0–1023. Default 10.
    pub detection_edge: u32,
    /// `gl`: offset after edge detection, 0–1023. Default 2.
    pub detection_drop: u32,
    /// `zj`: post-detection trajectory, 0 or 1. Default 1.
    pub post_detection_trajectory: u32,
    /// `zi`: post-detection retract distance, increments. Default 186
    /// (about 2 mm).
    pub post_detection_distance: u32,
}

/// The default post-detection retract for LLD searches: 186 increments,
/// about 2 mm of Z travel.
pub const DEFAULT_POST_DETECTION_DISTANCE: u32 = 186;

impl ClldZSearch {
    pub fn new(
        channel: usize,
        lowest_immersion: u32,
        search_start: u32,
        speed: u32,
        acceleration: u32,
    ) -> Result<ClldZSearch, CommandError> {
        let module = channel_module(channel)?;
        check_range(
            "zh",
            "lowest immersion position",
            "increments",
            f64::from(lowest_immersion),
            9320.0,
            31200.0,
        )?;
        check_range(
            "zc",
            "search start position",
            "increments",
            f64::from(search_start),
            9320.0,
            31200.0,
        )?;
        check_range(
            "zl",
            "search speed",
            "increments/s",
            f64::from(speed),
            20.0,
            15000.0,
        )?;
        check_range(
            "zr",
            "acceleration",
            "1000 increments/s²",
            f64::from(acceleration),
            5.0,
            150.0,
        )?;
        Ok(ClldZSearch {
            module,
            lowest_immersion,
            search_start,
            speed,
            acceleration,
            detection_edge: 10,
            detection_drop: 2,
            post_detection_trajectory: 1,
            post_detection_distance: DEFAULT_POST_DETECTION_DISTANCE,
        })
    }
}

impl Command for ClldZSearch {
    const CODE: &'static str = "ZL";
    type Response = ();

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("zh", 5, self.lowest_immersion)
            .uint("zc", 5, self.search_start)
            .uint("zl", 5, self.speed)
            .uint("zr", 3, self.acceleration)
            .uint("gt", 4, self.detection_edge)
            .uint("gl", 4, self.detection_drop)
            .uint("zj", 1, self.post_detection_trajectory)
            .uint("zi", 4, self.post_detection_distance)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The foam sub-modes of the pressure LLD search (`ZE` `cj`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PressureLldMode {
    /// Code 0: detect the liquid surface.
    #[default]
    Liquid,
    /// Code 1: detect foam.
    Foam,
}

impl PressureLldMode {
    pub fn code(self) -> u32 {
        match self {
            PressureLldMode::Liquid => 0,
            PressureLldMode::Foam => 1,
        }
    }
}

/// `ZE` on a channel — pressure LLD Z search, optionally with capacitive
/// verification and foam detection. The reply carries the detected
/// positions after `if` in increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlldZSearch {
    module: Module,
    /// `zh`: lowest immersion position, increments.
    pub lowest_immersion: u32,
    /// `zc`: search start position, increments.
    pub search_start: u32,
    /// `zi`: post-detection retract distance, increments.
    pub post_detection_distance: u32,
    /// `zj`: post-detection trajectory, 0 or 1.
    pub post_detection_trajectory: u32,
    /// `gf`: the mounted tip has a filter.
    pub tip_has_filter: bool,
    /// `gt`: capacitive detection edge (verification feature).
    pub clld_detection_edge: u32,
    /// `gl`: capacitive detection drop (verification feature).
    pub clld_detection_drop: u32,
    /// `gu`: pressure detection edge.
    pub plld_detection_edge: u32,
    /// `gn`: pressure detection drop.
    pub plld_detection_drop: u32,
    /// `gm`: verify the pressure result with capacitive detection.
    pub clld_verification: bool,
    /// `gz`: maximum allowed delta between the pressure and capacitive
    /// results, increments.
    pub max_delta_plld_clld: u32,
    /// `cj`: foam sub-mode.
    pub mode: PressureLldMode,
    /// `co`: foam detection drop.
    pub foam_detection_drop: u32,
    /// `cp`: foam detection edge tolerance.
    pub foam_edge_tolerance: u32,
    /// `cq`: foam AD values, 0–4999.
    pub foam_ad_values: u32,
    /// `cl`: foam search speed, increments/s, 20–13500.
    pub foam_search_speed: u32,
    /// `cc`: dispense back the detection volume.
    pub dispense_back: bool,
    /// `cd`: dispense-back volume, dispensing-drive increments, 0–26666.
    pub dispense_back_volume: u32,
    /// `zv`: channel speed above the search start, increments/s.
    pub speed_above_start: u32,
    /// `zl`: search speed, increments/s.
    pub speed: u32,
    /// `zr`: acceleration, thousands of increments/s².
    pub acceleration: u32,
    /// `zw`: Z current limit, 0–7.
    pub z_current_limit: u32,
    /// `dl`: dispensing-drive speed, increments/s.
    pub dispense_drive_speed: u32,
    /// `dr`: dispensing-drive acceleration, thousands of increments/s².
    pub dispense_drive_acceleration: u32,
    /// `dv`: dispensing-drive maximum speed, increments/s.
    pub dispense_drive_max_speed: u32,
    /// `dw`: dispensing-drive current limit, 0–7.
    pub dispense_drive_current_limit: u32,
}

impl PlldZSearch {
    /// Builds a search with the documented defaults for every tuning
    /// parameter; the caller sets the geometry and speeds.
    pub fn new(
        channel: usize,
        lowest_immersion: u32,
        search_start: u32,
        speed: u32,
        acceleration: u32,
    ) -> Result<PlldZSearch, CommandError> {
        let module = channel_module(channel)?;
        check_range(
            "zh",
            "lowest immersion position",
            "increments",
            f64::from(lowest_immersion),
            9320.0,
            31200.0,
        )?;
        check_range(
            "zc",
            "search start position",
            "increments",
            f64::from(search_start),
            9320.0,
            31200.0,
        )?;
        check_range(
            "zl",
            "search speed",
            "increments/s",
            f64::from(speed),
            20.0,
            15000.0,
        )?;
        check_range(
            "zr",
            "acceleration",
            "1000 increments/s²",
            f64::from(acceleration),
            5.0,
            150.0,
        )?;
        Ok(PlldZSearch {
            module,
            lowest_immersion,
            search_start,
            post_detection_distance: DEFAULT_POST_DETECTION_DISTANCE,
            post_detection_trajectory: 1,
            tip_has_filter: false,
            clld_detection_edge: 10,
            clld_detection_drop: 2,
            plld_detection_edge: 30,
            plld_detection_drop: 10,
            clld_verification: false,
            max_delta_plld_clld: 466,
            mode: PressureLldMode::Liquid,
            foam_detection_drop: 30,
            foam_edge_tolerance: 30,
            foam_ad_values: 30,
            foam_search_speed: 932,
            dispense_back: false,
            dispense_back_volume: 0,
            speed_above_start: 11186,
            speed,
            acceleration,
            z_current_limit: 3,
            dispense_drive_speed: 1829,
            dispense_drive_acceleration: 73,
            dispense_drive_max_speed: 5303,
            dispense_drive_current_limit: 3,
        })
    }
}

impl Command for PlldZSearch {
    const CODE: &'static str = "ZE";
    /// The detected positions after `if`, in increments: the liquid surface
    /// and, in foam mode, the foam surface.
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("zh", 5, self.lowest_immersion)
            .uint("zc", 5, self.search_start)
            .uint("zi", 4, self.post_detection_distance)
            .uint("zj", 1, self.post_detection_trajectory)
            .flag("gf", self.tip_has_filter)
            .uint("gt", 4, self.clld_detection_edge)
            .uint("gl", 4, self.clld_detection_drop)
            .uint("gu", 4, self.plld_detection_edge)
            .uint("gn", 4, self.plld_detection_drop)
            .flag("gm", self.clld_verification)
            .uint("gz", 4, self.max_delta_plld_clld)
            .uint("cj", 1, self.mode.code())
            .uint("co", 4, self.foam_detection_drop)
            .uint("cp", 4, self.foam_edge_tolerance)
            .uint("cq", 4, self.foam_ad_values)
            .uint("cl", 5, self.foam_search_speed)
            .flag("cc", self.dispense_back)
            .uint("cd", 5, self.dispense_back_volume)
            .uint("zv", 5, self.speed_above_start)
            .uint("zl", 5, self.speed)
            .uint("zr", 3, self.acceleration)
            .uint("zw", 1, self.z_current_limit)
            .uint("dl", 5, self.dispense_drive_speed)
            .uint("dr", 3, self.dispense_drive_acceleration)
            .uint("dv", 5, self.dispense_drive_max_speed)
            .uint("dw", 1, self.dispense_drive_current_limit)
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int_list("if", 5)])?;
        Ok(fields
            .int_list("if")
            .map(<[i64]>::to_vec)
            .unwrap_or_default())
    }
}

/// `YL` on a channel — capacitive LLD Y search: move in Y until an object
/// is detected. The detected position is read afterwards with the
/// master-level `RB` query. Runs up to two minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClldYSearch {
    module: Module,
    /// `ya`: maximum search position, Y-drive increments, 0–13714.
    pub max_search_position: u32,
    /// `gt`: detection edge steepness, 0–1023.
    pub detection_edge: u32,
    /// `gl`: offset after edge detection; zero measures the contact
    /// position itself.
    pub detection_drop: u32,
    /// `yv`: maximum speed, increments/s, 20–8000.
    pub speed: u32,
    /// `yr`: acceleration ramp index 1–4 (times 5000 increments/s²).
    pub acceleration_index: u32,
    /// `yw`: current limit, 0–7.
    pub current_limit: u32,
}

impl ClldYSearch {
    pub fn new(
        channel: usize,
        max_search_position: u32,
        speed: u32,
    ) -> Result<ClldYSearch, CommandError> {
        let module = channel_module(channel)?;
        check_range(
            "ya",
            "maximum Y search position",
            "increments",
            f64::from(max_search_position),
            0.0,
            13714.0,
        )?;
        check_range(
            "yv",
            "Y search speed",
            "increments/s",
            f64::from(speed),
            20.0,
            8000.0,
        )?;
        Ok(ClldYSearch {
            module,
            max_search_position,
            detection_edge: 10,
            detection_drop: 0,
            speed,
            acceleration_index: 2,
            current_limit: 3,
        })
    }
}

impl Command for ClldYSearch {
    const CODE: &'static str = "YL";
    type Response = ();

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("ya", 5, self.max_search_position)
            .uint("gt", 4, self.detection_edge)
            .uint("gl", 4, self.detection_drop)
            .uint("yv", 4, self.speed)
            .uint("yr", 1, self.acceleration_index)
            .uint("yw", 1, self.current_limit)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `SI` on a channel — initialize the squeezer drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitializeSqueezerDrive {
    module: Module,
}

impl InitializeSqueezerDrive {
    pub fn new(channel: usize) -> Result<InitializeSqueezerDrive, CommandError> {
        Ok(InitializeSqueezerDrive {
            module: channel_module(channel)?,
        })
    }
}

impl Command for InitializeSqueezerDrive {
    const CODE: &'static str = "SI";
    type Response = ();

    fn module(&self) -> Module {
        self.module
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
    fn a_channel_z_move_addresses_the_channels_own_module() {
        let command = ChannelZMove::new(2, 20000, 1000, 50, 3).expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(7)),
            "P3ZAid0007za20000zv01000zr050zw3",
            "channel index 2 is module P3 and positions are increments"
        );
    }

    #[test]
    fn z_moves_below_the_9320_increment_floor_are_rejected() {
        let error = ChannelZMove::new(0, 9000, 1000, 50, 3)
            .expect_err("9000 increments is below the drive's physical floor");
        assert!(
            error.to_string().contains("za") && error.to_string().contains("9320"),
            "the error names the parameter and floor: {error}"
        );
    }

    #[test]
    fn a_clld_search_encodes_the_documented_defaults() {
        let command = ClldZSearch::new(0, 9320, 25000, 932, 74).expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "P1ZLid0001zh09320zc25000zl00932zr074gt0010gl0002zj1zi0186",
            "detection edge 10, drop 2, trajectory 1, and the 2 mm retract are the defaults"
        );
    }

    #[test]
    fn a_plld_reply_carries_the_detected_positions_after_if() {
        let positions =
            PlldZSearch::parse_response("if23120 00000").expect("a well-formed ZE reply parses");
        assert_eq!(
            positions,
            vec![23120, 0],
            "positions are increments, liquid first"
        );
    }

    #[test]
    fn stop_disk_z_parses_six_digits() {
        let z =
            RequestStopDiskZ::parse_response("rz031200").expect("a well-formed RZ reply parses");
        assert_eq!(
            z, 31200,
            "the reply is the stop-disk position in increments"
        );
    }
}
