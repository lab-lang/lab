//! iSWAP plate-gripper commands: master-level (`C0`) plate handling and
//! `R0`-direct axis control. Encode-only: the structs produce verified wire
//! strings but the session offers no choreography for them.
//!
//! Grip geometry conventions: the open position is the plate width plus
//! 3 mm and the grip position the width minus 3 mm; the `gb` parameter is
//! the plate width ×10 − 33 in wire units. The `xe` acceleration indices
//! are only honored by `PM`/`PT` — a firmware quirk keeps them inert on
//! `PP`/`PR`.

use crate::commands::Command;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::{FieldSpec, ResponseParseError, parse_fields};

/// The default iSWAP traverse height for parking: 284.0 mm.
pub const ISWAP_PARK_TRAVERSE_HEIGHT: u32 = 2840;

/// The grip directions of the `gr` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GripDirection {
    /// Code 1: grip from the front (−Y).
    Front,
    /// Code 2: grip from the right (+X).
    Right,
    /// Code 3: grip from the back (+Y).
    Back,
    /// Code 4: grip from the left (−X).
    Left,
}

impl GripDirection {
    pub fn code(self) -> u32 {
        match self {
            GripDirection::Front => 1,
            GripDirection::Right => 2,
            GripDirection::Back => 3,
            GripDirection::Left => 4,
        }
    }
}

/// The `gb` grip width for a plate: width ×10 − 33 in 0.1 mm.
pub fn grip_width_for_plate(plate_width_mm: f64) -> u32 {
    (plate_width_mm * 10.0 - 33.0).round().max(0.0) as u32
}

/// The `go` open width for a plate: width + 3 mm, in 0.1 mm.
pub fn open_width_for_plate(plate_width_mm: f64) -> u32 {
    ((plate_width_mm + 3.0) * 10.0).round().max(0.0) as u32
}

/// A signed iSWAP coordinate: magnitude plus direction flag.
fn signed(value: i32) -> (u32, bool) {
    (value.unsigned_abs(), value < 0)
}

/// `FI` — initialize the iSWAP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IswapInitialize;

impl Command for IswapInitialize {
    const CODE: &'static str = "FI";
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

/// `FY` — position the iSWAP for the free Y range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IswapPositionForFreeY;

impl Command for IswapPositionForFreeY {
    const CODE: &'static str = "FY";
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

/// `GX`/`GY`/`GZ` — relative iSWAP moves: a step magnitude (0.1 mm, three
/// digits) plus a direction flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeAxis {
    X,
    Y,
    Z,
}

/// A relative iSWAP move along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapRelativeMove {
    axis: RelativeAxis,
    step: u32,
    negative: bool,
}

impl IswapRelativeMove {
    /// `step` in 0.1 mm, at most 999.
    pub fn new(
        axis: RelativeAxis,
        step: u32,
        negative: bool,
    ) -> Result<IswapRelativeMove, CommandError> {
        check_range(
            "gx",
            "relative step size",
            "0.1 mm",
            f64::from(step),
            0.0,
            999.0,
        )?;
        Ok(IswapRelativeMove {
            axis,
            step,
            negative,
        })
    }

    fn names(&self) -> (&'static str, &'static str, &'static str) {
        match self.axis {
            RelativeAxis::X => ("GX", "gx", "xd"),
            RelativeAxis::Y => ("GY", "gy", "yd"),
            RelativeAxis::Z => ("GZ", "gz", "zd"),
        }
    }

    /// The wire frame; relative moves pick their code from the axis, so
    /// they encode outside the [`Command`] trait's per-type code.
    pub fn to_wire(&self, id: crate::framing::CommandId) -> String {
        let (code, step_name, direction_name) = self.names();
        FrameBuilder::with_id(Module::Master, code, id)
            .uint(step_name, 3, self.step)
            .flag(direction_name, self.negative)
            .build()
    }
}

/// `PG` — park the iSWAP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapPark {
    /// `th`: minimum traverse height, 0.1 mm. Default 2840.
    pub traverse_height: u32,
}

impl Default for IswapPark {
    fn default() -> IswapPark {
        IswapPark {
            traverse_height: ISWAP_PARK_TRAVERSE_HEIGHT,
        }
    }
}

impl Command for IswapPark {
    const CODE: &'static str = "PG";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("th", 4, self.traverse_height)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PP` — get a plate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapGetPlate {
    /// Plate X, 0.1 mm, signed.
    pub x: i32,
    /// Plate Y, 0.1 mm, signed.
    pub y: i32,
    /// Plate Z, 0.1 mm, signed.
    pub z: i32,
    /// `gr`: grip direction.
    pub grip_direction: GripDirection,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `te`: Z at end of command, 0.1 mm.
    pub end_z: u32,
    /// `gw`: grip strength 0–9. Default 4.
    pub grip_strength: u32,
    /// `go`: open position, 0.1 mm ([`open_width_for_plate`]).
    pub open_position: u32,
    /// `gb`: grip width, 0.1 mm ([`grip_width_for_plate`]).
    pub grip_width: u32,
    /// `gt`: width tolerance, 0.1 mm.
    pub width_tolerance: u32,
    /// `ga`: collision control level.
    pub collision_control: bool,
    /// `gc`: fold up at the end of the process.
    pub fold_up: bool,
}

impl IswapGetPlate {
    /// Validates the documented ranges.
    pub fn validate(&self) -> Result<(), CommandError> {
        check_range(
            "gw",
            "grip strength",
            "",
            f64::from(self.grip_strength),
            0.0,
            9.0,
        )?;
        check_range(
            "th",
            "minimum traverse height",
            "0.1 mm",
            f64::from(self.traverse_height),
            0.0,
            3600.0,
        )?;
        check_range(
            "te",
            "Z at end of command",
            "0.1 mm",
            f64::from(self.end_z),
            0.0,
            3600.0,
        )?;
        Ok(())
    }
}

impl Command for IswapGetPlate {
    const CODE: &'static str = "PP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = signed(self.x);
        let (yj, yd) = signed(self.y);
        let (zj, zd) = signed(self.z);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yj", 4, yj)
            .flag("yd", yd)
            .uint("zj", 4, zj)
            .flag("zd", zd)
            .uint("gr", 1, self.grip_direction.code())
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
            .uint("gw", 1, self.grip_strength)
            .uint("go", 4, self.open_position)
            .uint("gb", 4, self.grip_width)
            .uint("gt", 2, self.width_tolerance)
            .flag("ga", self.collision_control)
            .flag("gc", self.fold_up)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PR` — put a plate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapPutPlate {
    /// Plate X, 0.1 mm, signed.
    pub x: i32,
    /// Plate Y, 0.1 mm, signed.
    pub y: i32,
    /// Plate Z, 0.1 mm, signed.
    pub z: i32,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `te`: Z at end of command, 0.1 mm.
    pub end_z: u32,
    /// `gr`: grip direction.
    pub grip_direction: GripDirection,
    /// `go`: open position, 0.1 mm.
    pub open_position: u32,
    /// `ga`: collision control level.
    pub collision_control: bool,
    /// `gc`: fold up at the end of the process.
    pub fold_up: bool,
}

impl Command for IswapPutPlate {
    const CODE: &'static str = "PR";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = signed(self.x);
        let (yj, yd) = signed(self.y);
        let (zj, zd) = signed(self.z);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yj", 4, yj)
            .flag("yd", yd)
            .uint("zj", 4, zj)
            .flag("zd", zd)
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
            .uint("gr", 1, self.grip_direction.code())
            .uint("go", 4, self.open_position)
            .flag("ga", self.collision_control)
            .flag("gc", self.fold_up)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PM` — move a gripped plate. The `xe` acceleration indices are active
/// here (and on `PT`), unlike on `PP`/`PR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapMovePlate {
    /// Target X, 0.1 mm, signed.
    pub x: i32,
    /// Target Y, 0.1 mm, signed.
    pub y: i32,
    /// Target Z, 0.1 mm, signed.
    pub z: i32,
    /// `gr`: grip direction.
    pub grip_direction: GripDirection,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `ga`: collision control level.
    pub collision_control: bool,
    /// `xe`: high acceleration index, 0–4.
    pub acceleration_index_high: u32,
    /// `xe`: low acceleration index, 0–4.
    pub acceleration_index_low: u32,
}

impl Command for IswapMovePlate {
    const CODE: &'static str = "PM";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = signed(self.x);
        let (yj, yd) = signed(self.y);
        let (zj, zd) = signed(self.z);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yj", 4, yj)
            .flag("yd", yd)
            .uint("zj", 4, zj)
            .flag("zd", zd)
            .uint("gr", 1, self.grip_direction.code())
            .uint("th", 4, self.traverse_height)
            .flag("ga", self.collision_control)
            .text(
                "xe",
                &format!(
                    "{} {}",
                    self.acceleration_index_high, self.acceleration_index_low
                ),
            )
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PN` — collapse the gripper arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapCollapseArm {
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `gc`: fold up at the end of the process.
    pub fold_up: bool,
}

impl Command for IswapCollapseArm {
    const CODE: &'static str = "PN";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("th", 4, self.traverse_height)
            .flag("gc", self.fold_up)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The default hotel depth for teach and hotel commands: 130.0 mm.
pub const DEFAULT_HOTEL_DEPTH: u32 = 1300;

/// `PT` — teach a position. The `xe` acceleration indices are active here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapTeachPosition {
    /// Target X, 0.1 mm, signed.
    pub x: i32,
    /// Target Y, 0.1 mm, signed.
    pub y: i32,
    /// Target Z, 0.1 mm, signed.
    pub z: i32,
    /// `hh`: the location is a stack/hotel.
    pub hotel: bool,
    /// `hd`: hotel depth, 0.1 mm. Default 1300.
    pub hotel_depth: u32,
    /// `gr`: grip direction.
    pub grip_direction: GripDirection,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `ga`: collision control level.
    pub collision_control: bool,
    /// `xe`: high acceleration index, 0–4.
    pub acceleration_index_high: u32,
    /// `xe`: low acceleration index, 0–4.
    pub acceleration_index_low: u32,
}

impl Command for IswapTeachPosition {
    const CODE: &'static str = "PT";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = signed(self.x);
        let (yj, yd) = signed(self.y);
        let (zj, zd) = signed(self.z);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yj", 4, yj)
            .flag("yd", yd)
            .uint("zj", 4, zj)
            .flag("zd", zd)
            .flag("hh", self.hotel)
            .uint("hd", 4, self.hotel_depth)
            .uint("gr", 1, self.grip_direction.code())
            .uint("th", 4, self.traverse_height)
            .flag("ga", self.collision_control)
            .text(
                "xe",
                &format!(
                    "{} {}",
                    self.acceleration_index_high, self.acceleration_index_low
                ),
            )
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The hotel geometry shared by the unsafe `PI` (put) and `PO` (get)
/// commands. These reach outside the deck: nothing guards the space the
/// arm swings through, so the caller carries full responsibility for the
/// clearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotelAccess {
    /// Hotel center X, 0.1 mm, signed.
    pub x: i32,
    /// Hotel center Y, 0.1 mm, signed.
    pub y: i32,
    /// Hotel center Z, 0.1 mm, signed.
    pub z: i32,
    /// `zc`: clearance height, 0.1 mm, 0–999.
    pub clearance_height: u32,
    /// `hd`: hotel depth, 0.1 mm, 0–3000.
    pub hotel_depth: u32,
    /// `gr`: grip direction.
    pub grip_direction: GripDirection,
    /// `th`: minimum traverse height, 0.1 mm.
    pub traverse_height: u32,
    /// `te`: Z at end of command, 0.1 mm.
    pub end_z: u32,
    /// `gw`: grip strength 0–9.
    pub grip_strength: u32,
    /// `go`: open position, 0.1 mm.
    pub open_position: u32,
    /// `gb`: grip width, 0.1 mm.
    pub grip_width: u32,
    /// `gt`: width tolerance, 0.1 mm.
    pub width_tolerance: u32,
    /// `ga`: collision control level.
    pub collision_control: bool,
}

impl HotelAccess {
    fn encode(&self, builder: FrameBuilder) -> FrameBuilder {
        let (xs, xd) = signed(self.x);
        let (yj, yd) = signed(self.y);
        let (zj, zd) = signed(self.z);
        builder
            .uint("xs", 5, xs)
            .flag("xd", xd)
            .uint("yj", 4, yj)
            .flag("yd", yd)
            .uint("zj", 4, zj)
            .flag("zd", zd)
            .uint("zc", 3, self.clearance_height)
            .uint("hd", 4, self.hotel_depth)
            .uint("gr", 1, self.grip_direction.code())
            .uint("th", 4, self.traverse_height)
            .uint("te", 4, self.end_z)
            .uint("gw", 1, self.grip_strength)
            .uint("go", 4, self.open_position)
            .uint("gb", 4, self.grip_width)
            .uint("gt", 2, self.width_tolerance)
            .flag("ga", self.collision_control)
    }
}

/// `PI` — put a plate into a hotel. Unsafe: reaches outside the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapHotelPut(pub HotelAccess);

impl Command for IswapHotelPut {
    const CODE: &'static str = "PI";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        self.0.encode(builder)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PO` — get a plate from a hotel. Unsafe: reaches outside the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapHotelGet(pub HotelAccess);

impl Command for IswapHotelGet {
    const CODE: &'static str = "PO";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        self.0.encode(builder)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `QP` — query plate presence in the gripper (fmt `ph#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IswapQueryPlatePresence;

impl Command for IswapQueryPlatePresence {
    const CODE: &'static str = "QP";
    type Response = bool;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("ph", 1)])?;
        Ok(fields.int("ph") == Some(1))
    }
}

/// `RG` on the master — query whether the iSWAP is parked (fmt `rg#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IswapQueryParked;

impl Command for IswapQueryParked {
    const CODE: &'static str = "RG";
    type Response = bool;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("rg", 1)])?;
        Ok(fields.int("rg") == Some(1))
    }
}

/// The iSWAP position (`QG` reply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapPosition {
    /// X in 0.1 mm, signed.
    pub x: i64,
    /// Y in 0.1 mm, signed.
    pub y: i64,
    /// Z in 0.1 mm, signed.
    pub z: i64,
}

/// `QG` on the master — query the iSWAP position (fmt
/// `xs#####xd#yj####yd#zj####zd#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IswapQueryPosition;

impl Command for IswapQueryPosition {
    const CODE: &'static str = "QG";
    type Response = IswapPosition;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<IswapPosition, ResponseParseError> {
        let fields = parse_fields(
            payload,
            &[
                FieldSpec::int("xs", 5),
                FieldSpec::int("xd", 1),
                FieldSpec::int("yj", 4),
                FieldSpec::int("yd", 1),
                FieldSpec::int("zj", 4),
                FieldSpec::int("zd", 1),
            ],
        )?;
        let apply = |magnitude: &str, direction: &str| {
            let value = fields.int(magnitude).unwrap_or(0);
            if fields.int(direction) == Some(1) {
                -value
            } else {
                value
            }
        };
        Ok(IswapPosition {
            x: apply("xs", "xd"),
            y: apply("yj", "yd"),
            z: apply("zj", "zd"),
        })
    }
}

/// The wrist orientations of `R0 WP`. The command must be sent without an
/// id: the firmware rejects an id-bearing `WP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WristOrientation {
    /// Orientation code 1.
    Right,
    /// Orientation code 2.
    Front,
    /// Orientation code 3.
    Left,
}

impl WristOrientation {
    pub fn code(self) -> u32 {
        match self {
            WristOrientation::Right => 1,
            WristOrientation::Front => 2,
            WristOrientation::Left => 3,
        }
    }
}

/// `WP` on `R0` — rotate the wrist. Sent without an auto-id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapWristRotate {
    pub orientation: WristOrientation,
}

impl Command for IswapWristRotate {
    const CODE: &'static str = "WP";
    const EXPECTS_REPLY: bool = false;
    type Response = ();

    fn module(&self) -> Module {
        Module::Iswap
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("wp", 1, self.orientation.code())
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `PD` on `R0` — combined rotation: the position code is rotation × 10 +
/// grip direction (11–34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapCombinedRotate {
    position_code: u32,
    /// `wv`: gripper velocity, increments/s.
    pub gripper_velocity: u32,
    /// `wr`: gripper acceleration.
    pub gripper_acceleration: u32,
    /// `ww`: gripper protection level.
    pub gripper_protection: u32,
    /// `tv`: wrist velocity, increments/s.
    pub wrist_velocity: u32,
    /// `tr`: wrist acceleration.
    pub wrist_acceleration: u32,
    /// `tw`: wrist protection level.
    pub wrist_protection: u32,
}

impl IswapCombinedRotate {
    /// `rotation` 1–3 and a grip direction combine into the 11–34 code.
    pub fn new(rotation: u32, grip: GripDirection) -> Result<IswapCombinedRotate, CommandError> {
        check_range("pd", "rotation position", "", f64::from(rotation), 1.0, 3.0)?;
        Ok(IswapCombinedRotate {
            position_code: rotation * 10 + grip.code(),
            gripper_velocity: 12000,
            gripper_acceleration: 100,
            gripper_protection: 5,
            wrist_velocity: 25000,
            wrist_acceleration: 170,
            wrist_protection: 5,
        })
    }
}

impl Command for IswapCombinedRotate {
    const CODE: &'static str = "PD";
    type Response = ();

    fn module(&self) -> Module {
        Module::Iswap
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("pd", 2, self.position_code)
            .uint("wv", 5, self.gripper_velocity)
            .uint("wr", 3, self.gripper_acceleration)
            .uint("ww", 1, self.gripper_protection)
            .uint("tv", 5, self.wrist_velocity)
            .uint("tr", 3, self.wrist_acceleration)
            .uint("tw", 1, self.wrist_protection)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// The iSWAP axis queries on `R0`: each returns increment counters (fmt
/// `<name>##### (n)`).
macro_rules! iswap_axis_query {
    ($(#[$doc:meta])* $name:ident, $code:literal, $field:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl Command for $name {
            const CODE: &'static str = $code;
            /// The axis counters in increments.
            type Response = Vec<i64>;

            fn module(&self) -> Module {
                Module::Iswap
            }
            fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
                builder
            }
            fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
                let fields = parse_fields(payload, &[FieldSpec::int_list($field, 5)])?;
                Ok(fields.int_list($field).map(<[i64]>::to_vec).unwrap_or_default())
            }
        }
    };
}

iswap_axis_query!(
    /// `RY` on `R0` — Y axis counters.
    IswapRequestY, "RY", "ry"
);
iswap_axis_query!(
    /// `RZ` on `R0` — Z axis counters.
    IswapRequestZ, "RZ", "rz"
);
iswap_axis_query!(
    /// `RW` on `R0` — rotation axis counters.
    IswapRequestRotation, "RW", "rw"
);
iswap_axis_query!(
    /// `RT` on `R0` — wrist axis counters.
    IswapRequestWrist, "RT", "rt"
);
iswap_axis_query!(
    /// `RG` on `R0` — gripper jaw counters.
    IswapRequestGripper, "RG", "rg"
);

/// The EEPROM tables readable through `R0 RA`: rotation stops (`pw`), wrist
/// stops (`pt`), and the `py`/`pz`/`pg` tables. The arm link lengths live
/// in slot 9 of the `pw` and `pt` tables (default 138.0 mm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IswapEepromTable {
    RotationStops,
    WristStops,
    Py,
    Pz,
    Pg,
}

impl IswapEepromTable {
    fn name(self) -> &'static str {
        match self {
            IswapEepromTable::RotationStops => "pw",
            IswapEepromTable::WristStops => "pt",
            IswapEepromTable::Py => "py",
            IswapEepromTable::Pz => "pz",
            IswapEepromTable::Pg => "pg",
        }
    }
}

/// `RA` on `R0` — read an EEPROM table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IswapReadEepromTable {
    pub table: IswapEepromTable,
}

impl Command for IswapReadEepromTable {
    const CODE: &'static str = "RA";
    /// The table values.
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        Module::Iswap
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.text("ra", self.table.name())
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        // The reply echoes the table name (`pw##### (n)` and siblings);
        // strip the two-character name and take the bare numbers.
        Ok(crate::response::parse_bare_ints(
            payload.get(2..).unwrap_or(payload),
        ))
    }
}

/// The master-level EEPROM offsets read at setup through `C0 RA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterEepromOffset {
    /// `kg`: the iSWAP X offset, default 34.0 mm, read three digits wide.
    IswapXOffset,
    /// `kf`: the 96-head X offset, about 368 mm. It must be read four
    /// digits wide — a three-digit read silently truncates the value.
    Head96XOffset,
}

impl MasterEepromOffset {
    fn spec(self) -> FieldSpec {
        match self {
            MasterEepromOffset::IswapXOffset => FieldSpec::int("kg", 3),
            MasterEepromOffset::Head96XOffset => FieldSpec::int("kf", 4),
        }
    }
}

/// `RA` on the master — read one EEPROM offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadMasterEepromOffset {
    pub offset: MasterEepromOffset,
}

impl ReadMasterEepromOffset {
    /// Parses the offset value out of a reply payload; the width is chosen
    /// by the offset (four digits for `kf`, three for `kg`).
    pub fn parse_offset(&self, payload: &str) -> Result<i64, ResponseParseError> {
        let spec = self.offset.spec();
        let fields = parse_fields(payload, &[spec])?;
        Ok(fields.int(spec.name).unwrap_or(0))
    }
}

impl Command for ReadMasterEepromOffset {
    const CODE: &'static str = "RA";
    /// The raw reply payload; decode with [`ReadMasterEepromOffset::parse_offset`],
    /// which knows the per-offset field width.
    type Response = String;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        let name = match self.offset {
            MasterEepromOffset::IswapXOffset => "kg",
            MasterEepromOffset::Head96XOffset => "kf",
        };
        builder.text("ra", name)
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::CommandId;

    #[test]
    fn get_plate_reproduces_the_golden_string() {
        // Plate width 127.76 mm: gb = 1277.6 − 33 ≈ 1245, go = width + 3 mm
        // ≈ 1308.
        let command = IswapGetPlate {
            x: 3479,
            y: 1142,
            z: 1874,
            grip_direction: GripDirection::Front,
            traverse_height: 2800,
            end_z: 2800,
            grip_strength: 4,
            open_position: open_width_for_plate(127.76),
            grip_width: grip_width_for_plate(127.76),
            width_tolerance: 20,
            collision_control: false,
            fold_up: false,
        };
        command.validate().expect("all values are in range");
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "C0PPid0001xs03479xd0yj1142yd0zj1874zd0gr1th2800te2800gw4go1308gb1245gt20ga0gc0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn put_plate_reproduces_the_golden_string() {
        let command = IswapPutPlate {
            x: 3479,
            y: 3062,
            z: 1874,
            traverse_height: 2800,
            end_z: 2800,
            grip_direction: GripDirection::Front,
            open_position: 1308,
            collision_control: false,
            fold_up: false,
        };
        assert_eq!(
            command.to_wire(CommandId::new(2)),
            "C0PRid0002xs03479xd0yj3062yd0zj1874zd0th2800te2800gr1go1308ga0gc0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    fn expects_reply<C: Command>(_: &C) -> bool {
        C::EXPECTS_REPLY
    }

    #[test]
    fn the_wrist_rotate_carries_no_id() {
        let command = IswapWristRotate {
            orientation: WristOrientation::Front,
        };
        assert!(
            !expects_reply(&command),
            "WP replies are not correlated by id"
        );
        assert_eq!(
            command.to_wire(None),
            "R0WPwp2",
            "the frame carries no id parameter"
        );
    }

    #[test]
    fn grip_geometry_follows_the_plate_width() {
        assert_eq!(
            grip_width_for_plate(127.76),
            1245,
            "gb is width ×10 − 33, rounded"
        );
        assert_eq!(
            open_width_for_plate(127.76),
            1308,
            "go is width + 3 mm in 0.1 mm"
        );
    }

    #[test]
    fn the_head_offset_must_be_read_four_digits_wide() {
        let read = ReadMasterEepromOffset {
            offset: MasterEepromOffset::Head96XOffset,
        };
        let value = read.parse_offset("kf3680").expect("a four-digit kf parses");
        assert_eq!(
            value, 3680,
            "a three-digit read would silently truncate 368.0 mm"
        );
    }
}
