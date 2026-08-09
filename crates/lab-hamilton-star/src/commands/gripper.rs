//! CoRe gripper commands: the paired-channel plate gripper that mounts
//! tool paddles on two pipetting channels. Encode-only.
//!
//! The tools live on the waste-block mount and are addressed with the magic
//! tip-type index 14. Pickup heights are 235.0/225.0 mm plus any adjustment;
//! return heights are 215.0/205.0 mm. The grip convention matches the
//! iSWAP: open = plate width + 3 mm, grip = width − 3 mm.

use crate::commands::Command;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::ResponseParseError;

/// The tip-type index reserved for the CoRe gripper tools.
pub const CORE_TOOL_TIP_TYPE: u32 = 14;
/// `ZT` begin-Z: 235.0 mm plus adjustment, in 0.1 mm.
pub const CORE_GET_BEGIN_Z: u32 = 2350;
/// `ZT` end-Z: 225.0 mm plus adjustment, in 0.1 mm.
pub const CORE_GET_END_Z: u32 = 2250;
/// `ZS` begin-Z: 215.0 mm plus adjustment, in 0.1 mm.
pub const CORE_RETURN_BEGIN_Z: u32 = 2150;
/// `ZS` end-Z: 205.0 mm plus adjustment, in 0.1 mm.
pub const CORE_RETURN_END_Z: u32 = 2050;

/// `ZT` — pick the gripper tools off the waste-block mount with two
/// channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreGetTools {
    /// `xs`: mount X, 0.1 mm.
    pub x: u32,
    /// `ya`: back channel Y, 0.1 mm.
    pub back_channel_y: u32,
    /// `yb`: front channel Y, 0.1 mm.
    pub front_channel_y: u32,
    /// `pa`: back channel number, 1-based.
    pub back_channel: u32,
    /// `pb`: front channel number, 1-based.
    pub front_channel: u32,
    /// `tp`: begin pickup Z, 0.1 mm ([`CORE_GET_BEGIN_Z`] plus adjustment).
    pub begin_z: u32,
    /// `tz`: end pickup Z, 0.1 mm ([`CORE_GET_END_Z`] plus adjustment).
    pub end_z: u32,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
}

impl CoreGetTools {
    /// Validates the channel numbers.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "pa",
            "back channel number",
            "",
            f64::from(self.back_channel),
            1.0,
            16.0,
        )?;
        check_range(
            "pb",
            "front channel number",
            "",
            f64::from(self.front_channel),
            1.0,
            16.0,
        )?;
        Ok(())
    }
}

impl Command for CoreGetTools {
    const CODE: &'static str = "ZT";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("xs", 5, self.x)
            .flag("xd", false)
            .uint("ya", 4, self.back_channel_y)
            .uint("yb", 4, self.front_channel_y)
            .uint("pa", 2, self.back_channel)
            .uint("pb", 2, self.front_channel)
            .uint("tp", 4, self.begin_z)
            .uint("tz", 4, self.end_z)
            .uint("th", 4, self.traverse_height)
            .text("tt", "14")
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZS` — return the gripper tools to the waste-block mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreReturnTools {
    /// `xs`: mount X, 0.1 mm.
    pub x: u32,
    /// `ya`: back channel Y, 0.1 mm.
    pub back_channel_y: u32,
    /// `yb`: front channel Y, 0.1 mm.
    pub front_channel_y: u32,
    /// `tp`: begin deposit Z, 0.1 mm ([`CORE_RETURN_BEGIN_Z`] plus
    /// adjustment).
    pub begin_z: u32,
    /// `tz`: end deposit Z, 0.1 mm ([`CORE_RETURN_END_Z`] plus adjustment).
    pub end_z: u32,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `te`: Z at end of command, 0.1 mm.
    pub end_of_command_z: u32,
}

impl Command for CoreReturnTools {
    const CODE: &'static str = "ZS";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("xs", 5, self.x)
            .flag("xd", false)
            .uint("ya", 4, self.back_channel_y)
            .uint("yb", 4, self.front_channel_y)
            .uint("tp", 4, self.begin_z)
            .uint("tz", 4, self.end_z)
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_of_command_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZO` — open the gripper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoreOpenGripper;

impl Command for CoreOpenGripper {
    const CODE: &'static str = "ZO";
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

/// `ZP` — grip a plate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreGripPlate {
    /// `xs`: plate X, 0.1 mm, signed via `xd`.
    pub x: i32,
    /// `yj`: plate Y, 0.1 mm.
    pub y: u32,
    /// `yv`: gripping speed, 0.1 mm/s. Default 50.
    pub grip_speed: u32,
    /// `zj`: plate Z, 0.1 mm.
    pub z: u32,
    /// `zy`: Z speed, 0.1 mm/s. Default 500.
    pub z_speed: u32,
    /// `yo`: open position = plate width ×10 + 30, 0.1 mm.
    pub open_position: u32,
    /// `yg`: plate width, 0.1 mm.
    pub plate_width: u32,
    /// `yw`: grip strength 0–99. Default 15.
    pub grip_strength: u32,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `te`: minimum Z at command end, 0.1 mm.
    pub end_z: u32,
}

impl CoreGripPlate {
    /// Validates the documented ranges.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "yw",
            "grip strength",
            "",
            f64::from(self.grip_strength),
            0.0,
            99.0,
        )?;
        Ok(())
    }
}

impl Command for CoreGripPlate {
    const CODE: &'static str = "ZP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("xs", 5, self.x.unsigned_abs())
            .flag("xd", self.x < 0)
            .uint("yj", 4, self.y)
            .uint("yv", 4, self.grip_speed)
            .uint("zj", 4, self.z)
            .uint("zy", 4, self.z_speed)
            .uint("yo", 4, self.open_position)
            .uint("yg", 4, self.plate_width)
            .uint("yw", 2, self.grip_strength)
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZR` — put a gripped plate down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorePutPlate {
    /// `xs`: target X, 0.1 mm, signed via `xd`.
    pub x: i32,
    /// `yj`: target Y, 0.1 mm.
    pub y: u32,
    /// `zj`: target Z, 0.1 mm.
    pub z: u32,
    /// `zi`: press-on distance, 0.1 mm, 0–50.
    pub press_on_distance: u32,
    /// `zy`: Z speed, 0.1 mm/s.
    pub z_speed: u32,
    /// `yo`: open position, 0.1 mm.
    pub open_position: u32,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `te`: Z at command end, 0.1 mm.
    pub end_z: u32,
}

impl CorePutPlate {
    /// Validates the documented ranges.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "zi",
            "press-on distance",
            "0.1 mm",
            f64::from(self.press_on_distance),
            0.0,
            50.0,
        )?;
        Ok(())
    }
}

impl Command for CorePutPlate {
    const CODE: &'static str = "ZR";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("xs", 5, self.x.unsigned_abs())
            .flag("xd", self.x < 0)
            .uint("yj", 4, self.y)
            .uint("zj", 4, self.z)
            .uint("zi", 3, self.press_on_distance)
            .uint("zy", 4, self.z_speed)
            .uint("yo", 4, self.open_position)
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZM` — move a gripped plate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreMovePlate {
    /// `xs`: target X, 0.1 mm, signed via `xd`.
    pub x: i32,
    /// `xg`: X acceleration index 0–7.
    pub x_acceleration_index: u32,
    /// `yj`: target Y, 0.1 mm.
    pub y: u32,
    /// `zj`: target Z, 0.1 mm.
    pub z: u32,
    /// `zy`: Z speed, 0.1 mm/s.
    pub z_speed: u32,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
}

impl CoreMovePlate {
    /// Validates the documented ranges.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "xg",
            "X acceleration index",
            "",
            f64::from(self.x_acceleration_index),
            0.0,
            7.0,
        )?;
        Ok(())
    }
}

impl Command for CoreMovePlate {
    const CODE: &'static str = "ZM";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("xs", 5, self.x.unsigned_abs())
            .flag("xd", self.x < 0)
            .uint("xg", 1, self.x_acceleration_index)
            .uint("yj", 4, self.y)
            .uint("zj", 4, self.z)
            .uint("zy", 4, self.z_speed)
            .uint("th", 4, self.traverse_height)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `ZB` — read the barcode of a gripped resource. The `ma`/`mr`/`mo`
/// values are opaque scanner tuning constants; the firmware requires them
/// verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreReadBarcode {
    /// `cp`: rail position, 1-based.
    pub rail: u32,
    /// `zb`: minimal Z position, 0.1 mm.
    pub minimal_z: u32,
    /// `th`: traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `zy`: Z speed, 0.1 mm/s.
    pub z_speed: u32,
    /// `bd`: reading direction code.
    pub reading_direction: u32,
}

impl Command for CoreReadBarcode {
    const CODE: &'static str = "ZB";
    /// The raw reply text carrying the barcode.
    type Response = String;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("cp", 2, self.rail)
            .uint("zb", 4, self.minimal_z)
            .uint("th", 4, self.traverse_height)
            .uint("zy", 4, self.z_speed)
            .uint("bd", 1, self.reading_direction)
            .text("ma", "0250 2100 0860 0200")
            .uint("mr", 1, 0)
            .text("mo", "000 000 000 000 000 000 000")
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::CommandId;

    #[test]
    fn tool_pickup_uses_the_magic_tip_type_and_documented_heights() {
        let command = CoreGetTools {
            x: 4350,
            back_channel_y: 1000,
            front_channel_y: 910,
            back_channel: 7,
            front_channel: 8,
            begin_z: CORE_GET_BEGIN_Z,
            end_z: CORE_GET_END_Z,
            traverse_height: 2800,
        };
        command.validate().expect("channels 7 and 8 exist");
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "C0ZTid0001xs04350xd0ya1000yb0910pa07pb08tp2350tz2250th2800tt14",
            "tool pickup addresses tip type 14 at 235.0/225.0 mm"
        );
    }

    #[test]
    fn grip_strength_beyond_99_is_rejected() {
        let mut command = CoreGripPlate {
            x: 0,
            y: 0,
            grip_speed: 50,
            z: 0,
            z_speed: 500,
            open_position: 1307,
            plate_width: 1277,
            grip_strength: 15,
            traverse_height: 2800,
            end_z: 2800,
        };
        command
            .validate()
            .expect("the default strength 15 is legal");
        command.grip_strength = 100;
        let error = command.validate().expect_err("100 exceeds the 0–99 range");
        assert!(
            error.to_string().contains("yw"),
            "the error names the parameter: {error}"
        );
    }
}
