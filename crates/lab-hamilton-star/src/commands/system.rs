//! Instrument and configuration commands on the master controller (§ system
//! commands): initialization, configuration discovery, tip-type definition,
//! cover control, and deck-area reservation.

use crate::commands::Command;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::{FieldSpec, Fields, ResponseParseError, parse_bare_ints, parse_fields};
use crate::units::TenthMm;

/// `VI` — pre-initialize the instrument. The firmware homes every drive; the
/// reply arrives only when initialization finishes, up to five minutes later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PreInitialize;

impl Command for PreInitialize {
    const CODE: &'static str = "VI";
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

/// `QW` — query initialization status. Exists on the master, the autoload,
/// the 96 head, and the iSWAP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryInitializationStatus {
    pub module: Module,
}

impl QueryInitializationStatus {
    pub fn master() -> QueryInitializationStatus {
        QueryInitializationStatus {
            module: Module::Master,
        }
    }
}

impl Command for QueryInitializationStatus {
    const CODE: &'static str = "QW";
    /// `true` when the module reports itself initialized.
    type Response = bool;

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qw", 1)])?;
        Ok(fields.int("qw") == Some(1))
    }
}

/// `RF` — request the firmware version string, from the master or any slave
/// module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestFirmwareVersion {
    pub module: Module,
}

impl Command for RequestFirmwareVersion {
    const CODE: &'static str = "RF";
    /// The version text, for example `1.0S 2009-06-24 A`.
    type Response = String;

    fn module(&self) -> Module {
        self.module
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload
            .strip_prefix("rf")
            .unwrap_or(payload)
            .trim()
            .to_string())
    }
}

/// The decoded `RM` machine configuration: the `kb` feature bits and the
/// `kp` channel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineConfiguration {
    /// The raw `kb` feature bits.
    pub feature_bits: u8,
    /// Number of pipetting channels (0–16).
    pub channel_count: usize,
}

impl MachineConfiguration {
    /// Bit 0: channels are the 1000 µL type.
    pub fn has_1000ul_channels(&self) -> bool {
        self.feature_bits & 0x01 != 0
    }
    /// Bit 1: an iSWAP is installed.
    pub fn has_iswap(&self) -> bool {
        self.feature_bits & 0x02 != 0
    }
    /// Bit 2: front cover monitoring is installed.
    pub fn has_front_cover_monitoring(&self) -> bool {
        self.feature_bits & 0x04 != 0
    }
    /// Bit 3: an autoload is installed.
    pub fn has_autoload(&self) -> bool {
        self.feature_bits & 0x08 != 0
    }
    /// Bits 4 and 5: wash stations 1 and 2.
    pub fn has_wash_station(&self, station: u8) -> bool {
        match station {
            1 => self.feature_bits & 0x10 != 0,
            2 => self.feature_bits & 0x20 != 0,
            _ => false,
        }
    }
    /// Bits 6 and 7: temperature carriers 1 and 2.
    pub fn has_temperature_carrier(&self, carrier: u8) -> bool {
        match carrier {
            1 => self.feature_bits & 0x40 != 0,
            2 => self.feature_bits & 0x80 != 0,
            _ => false,
        }
    }
}

/// `RM` — request the machine configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestMachineConfiguration;

impl Command for RequestMachineConfiguration {
    const CODE: &'static str = "RM";
    type Response = MachineConfiguration;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<MachineConfiguration, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::hex("kb", 2), FieldSpec::int("kp", 2)])?;
        Ok(MachineConfiguration {
            feature_bits: fields.hex("kb").unwrap_or(0) as u8,
            channel_count: fields.int("kp").unwrap_or(0).clamp(0, 16) as usize,
        })
    }
}

/// The decoded `QM` extended configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedConfiguration {
    /// The raw `ka` feature bits.
    pub ka_bits: u64,
    /// The raw `ke` feature bits.
    pub ke_bits: u64,
    /// `xw`: the tip-waste X position, the default target for channel
    /// initialization tip discard.
    pub tip_waste_x: TenthMm,
    /// Every scalar field of the reply by name, for callers that need the
    /// full table.
    pub fields: Fields,
}

impl ExtendedConfiguration {
    /// `ka` bit 1: a CoRe 96 head is installed.
    pub fn has_core96_head(&self) -> bool {
        self.ka_bits & (1 << 1) != 0
    }
    /// `ka` bit 13: XL channels are installed.
    pub fn has_xl_channels(&self) -> bool {
        self.ka_bits & (1 << 13) != 0
    }
    /// `ka` bit 19: robotic channels are installed.
    pub fn has_robotic_channels(&self) -> bool {
        self.ka_bits & (1 << 19) != 0
    }
}

/// `QM` — request the extended machine configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestExtendedConfiguration;

const QM_FIELDS: &[FieldSpec] = &[
    FieldSpec::hex("ka", 6),
    FieldSpec::hex("ke", 8),
    FieldSpec::int("xt", 2),
    FieldSpec::int("xa", 2),
    FieldSpec::int("xw", 5),
    FieldSpec::hex("xl", 2),
    FieldSpec::hex("xn", 2),
    FieldSpec::hex("xr", 2),
    FieldSpec::hex("xo", 2),
    FieldSpec::int("xm", 5),
    FieldSpec::int("xx", 5),
    FieldSpec::int("xu", 4),
    FieldSpec::int("xv", 4),
    FieldSpec::int("kc", 1),
    FieldSpec::int("kr", 1),
    FieldSpec::int("ys", 3),
    FieldSpec::int("kl", 3),
    FieldSpec::int("km", 3),
    FieldSpec::int("ym", 4),
    FieldSpec::int("yu", 4),
    FieldSpec::int("yx", 4),
];

impl Command for RequestExtendedConfiguration {
    const CODE: &'static str = "QM";
    type Response = ExtendedConfiguration;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<ExtendedConfiguration, ResponseParseError> {
        let fields = parse_fields(payload, QM_FIELDS)?;
        Ok(ExtendedConfiguration {
            ka_bits: fields.hex("ka").unwrap_or(0),
            ke_bits: fields.hex("ke").unwrap_or(0),
            tip_waste_x: TenthMm(fields.int("xw").unwrap_or(0).max(0) as u32),
            fields,
        })
    }
}

/// The `RU` maximum X travel ranges, in 0.1 mm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XTravelRanges {
    pub left_min: i64,
    pub left_max: i64,
    pub right_min: i64,
    pub right_max: i64,
}

/// `RU` — request the maximum X travel ranges of both arms. The reply is
/// four bare space-separated integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestMaxXTravel;

impl Command for RequestMaxXTravel {
    const CODE: &'static str = "RU";
    type Response = XTravelRanges;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<XTravelRanges, ResponseParseError> {
        let values = parse_bare_ints(payload);
        if values.len() < 4 {
            return Err(ResponseParseError::MissingField {
                payload: payload.to_string(),
                field: "ru",
                offset: 0,
            });
        }
        Ok(XTravelRanges {
            left_min: values[0],
            left_max: values[1],
            right_min: values[2],
            right_max: values[3],
        })
    }
}

/// `UA` — request the working envelopes. The reply is six bare
/// space-separated integers in 0.1 mm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestWorkingEnvelope;

impl Command for RequestWorkingEnvelope {
    const CODE: &'static str = "UA";
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        Ok(parse_bare_ints(payload))
    }
}

/// `RI` — request installation data (serial number and related text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestInstallationData;

impl Command for RequestInstallationData {
    const CODE: &'static str = "RI";
    type Response = String;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.trim().to_string())
    }
}

/// `RX` — request the left arm's X position (fmt `rx#####`, 0.1 mm).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestLeftArmXPosition;

fn parse_rx(payload: &str) -> Result<i64, ResponseParseError> {
    let fields = parse_fields(payload, &[FieldSpec::int("rx", 5)])?;
    Ok(fields.int("rx").unwrap_or(0))
}

impl Command for RequestLeftArmXPosition {
    const CODE: &'static str = "RX";
    /// The arm X position in 0.1 mm.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        parse_rx(payload)
    }
}

/// `QX` — request the right arm's X position (fmt `rx#####`, 0.1 mm).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestRightArmXPosition;

impl Command for RequestRightArmXPosition {
    const CODE: &'static str = "QX";
    /// The arm X position in 0.1 mm.
    type Response = i64;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<i64, ResponseParseError> {
        parse_rx(payload)
    }
}

/// The tip size classes of the `TT` `tg` parameter, which select the
/// pickup collar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipSizeCode {
    /// Code 1: low-volume tips with the 6 mm collar.
    LowVolume,
    /// Code 2: standard tips with the 8 mm collar.
    Standard,
    /// Code 3: high-volume tips with the 10 mm collar.
    HighVolume,
    /// Code 4: CoRe 384 tips.
    Core384,
    /// Code 5: XL tips.
    Xl,
}

impl TipSizeCode {
    pub fn code(self) -> u32 {
        match self {
            TipSizeCode::LowVolume => 1,
            TipSizeCode::Standard => 2,
            TipSizeCode::HighVolume => 3,
            TipSizeCode::Core384 => 4,
            TipSizeCode::Xl => 5,
        }
    }
}

/// The tip pickup methods of the `TT` `tu` and `TP` `td` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TipPickupMethod {
    /// Code 0: pick up out of a rack.
    #[default]
    OutOfRack,
    /// Code 1: pick up out of wash liquid.
    OutOfWashLiquid,
}

impl TipPickupMethod {
    pub fn code(self) -> u32 {
        match self {
            TipPickupMethod::OutOfRack => 0,
            TipPickupMethod::OutOfWashLiquid => 1,
        }
    }
}

/// `TT` — define a tip type in the firmware's volatile table (indices 0–99,
/// erased at power-off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefineTipType {
    index: u32,
    has_filter: bool,
    /// `tl`: tip length above the fitting depth, 0.1 mm.
    length: u32,
    /// `tv`: maximum volume, 0.1 µL.
    max_volume: u32,
    size: TipSizeCode,
    pickup: TipPickupMethod,
}

impl DefineTipType {
    pub fn new(
        index: u32,
        has_filter: bool,
        length_tenth_mm: u32,
        max_volume_tenth_ul: u32,
        size: TipSizeCode,
        pickup: TipPickupMethod,
    ) -> Result<DefineTipType, CommandError> {
        check_range("tt", "tip type index", "", f64::from(index), 0.0, 99.0)?;
        check_range(
            "tl",
            "tip length above the fitting depth",
            "0.1 mm",
            f64::from(length_tenth_mm),
            1.0,
            1999.0,
        )?;
        check_range(
            "tv",
            "maximum tip volume",
            "0.1 µL",
            f64::from(max_volume_tenth_ul),
            1.0,
            56000.0,
        )?;
        Ok(DefineTipType {
            index,
            has_filter,
            length: length_tenth_mm,
            max_volume: max_volume_tenth_ul,
            size,
            pickup,
        })
    }

    pub fn index(&self) -> u32 {
        self.index
    }
}

impl Command for DefineTipType {
    const CODE: &'static str = "TT";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("tt", 2, self.index)
            .flag("tf", self.has_filter)
            .uint("tl", 4, self.length)
            .uint("tv", 5, self.max_volume)
            .uint("tg", 1, self.size.code())
            .uint("tu", 1, self.pickup.code())
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `VP` — request the name of the last faulty parameter (fmt `vp&&`). The
/// session sends this automatically after a trace-31 unknown-parameter
/// error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestFaultyParameter;

impl Command for RequestFaultyParameter {
    const CODE: &'static str = "VP";
    /// The faulty parameter name plus the received/min/max text the firmware
    /// appends.
    type Response = String;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload
            .strip_prefix("vp")
            .unwrap_or(payload)
            .trim()
            .to_string())
    }
}

/// `RE` — request the firmware's error buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestErrorBuffer;

impl Command for RequestErrorBuffer {
    const CODE: &'static str = "RE";
    type Response = String;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.trim().to_string())
    }
}

/// `QB` — query the board type (fmt `qb#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryBoardType;

impl Command for QueryBoardType {
    const CODE: &'static str = "QB";
    type Response = u8;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<u8, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qb", 1)])?;
        Ok(fields.int("qb").unwrap_or(0) as u8)
    }
}

/// `QC` — query whether the cover is open (fmt `qc#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryCoverOpen;

impl Command for QueryCoverOpen {
    const CODE: &'static str = "QC";
    /// `true` when the cover is open.
    type Response = bool;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qc", 1)])?;
        Ok(fields.int("qc") == Some(1))
    }
}

/// `CO` — lock the cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LockCover;

impl Command for LockCover {
    const CODE: &'static str = "CO";
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

/// `HO` — unlock the cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UnlockCover;

impl Command for UnlockCover {
    const CODE: &'static str = "HO";
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

/// Arm prepositioning relative to a reserved deck area (`BA` `ap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArmPreposition {
    /// Code 0: left arm to the left, right arm to the right.
    #[default]
    Split,
    /// Code 1: all arms to the left.
    AllLeft,
    /// Code 2: all arms to the right.
    AllRight,
}

impl ArmPreposition {
    pub fn code(self) -> u32 {
        match self {
            ArmPreposition::Split => 0,
            ArmPreposition::AllLeft => 1,
            ArmPreposition::AllRight => 2,
        }
    }
}

/// `BA` — occupy (reserve) a deck area so arms stay clear of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OccupyArea {
    identification: u32,
    left_margin: u32,
    left_margin_negative: bool,
    size: u32,
    preposition: ArmPreposition,
}

impl OccupyArea {
    pub fn new(
        identification: u32,
        left_margin: u32,
        left_margin_negative: bool,
        size: u32,
        preposition: ArmPreposition,
    ) -> Result<OccupyArea, CommandError> {
        check_range(
            "aq",
            "area identification number",
            "",
            f64::from(identification),
            0.0,
            9999.0,
        )?;
        check_range(
            "al",
            "area left margin",
            "0.1 mm",
            f64::from(left_margin),
            0.0,
            99.0,
        )?;
        check_range("ar", "area size", "0.1 mm", f64::from(size), 0.0, 50000.0)?;
        Ok(OccupyArea {
            identification,
            left_margin,
            left_margin_negative,
            size,
            preposition,
        })
    }
}

impl Command for OccupyArea {
    const CODE: &'static str = "BA";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("aq", 4, self.identification)
            .uint("al", 2, self.left_margin)
            .flag("ad", self.left_margin_negative)
            .uint("ar", 5, self.size)
            .uint("ap", 1, self.preposition.code())
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `BB` — release one occupied deck area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseArea {
    identification: u32,
}

impl ReleaseArea {
    pub fn new(identification: u32) -> Result<ReleaseArea, CommandError> {
        check_range(
            "aq",
            "area identification number",
            "",
            f64::from(identification),
            0.0,
            9999.0,
        )?;
        Ok(ReleaseArea { identification })
    }
}

impl Command for ReleaseArea {
    const CODE: &'static str = "BB";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("aq", 4, self.identification)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `BC` — release every occupied deck area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReleaseAllAreas;

impl Command for ReleaseAllAreas {
    const CODE: &'static str = "BC";
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

/// `ZA` on the master — move all pipetting channels to the Z-safety height.
/// The workhorse retract: call it before any X or arm motion and in error
/// recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveAllChannelsToZSafety;

impl Command for MoveAllChannelsToZSafety {
    const CODE: &'static str = "ZA";
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

/// `NS` — trigger the next step. Sent without id; the firmware never
/// replies, so the session must not wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TriggerNextStep;

impl Command for TriggerNextStep {
    const CODE: &'static str = "NS";
    const EXPECTS_REPLY: bool = false;
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

/// `AB` — switch not-stop on. Sent without id; the firmware never replies,
/// so the session must not wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetNotStop;

impl Command for SetNotStop {
    const CODE: &'static str = "AB";
    const EXPECTS_REPLY: bool = false;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::CommandId;

    #[test]
    fn a_tip_type_definition_reproduces_the_golden_wire_string() {
        // Index 1, filtered, length 51.9 mm (59.9 total − 8 fitting depth),
        // 360.0 µL, STANDARD collar, pickup out of rack.
        let command = DefineTipType::new(
            1,
            true,
            519,
            3600,
            TipSizeCode::Standard,
            TipPickupMethod::OutOfRack,
        )
        .expect("the 300 µL filter tip is within every range");
        assert_eq!(
            command.to_wire(CommandId::new(1)),
            "C0TTid0001tt01tf1tl0519tv03600tg2tu0",
            "the encoder must reproduce the verified wire string byte for byte"
        );
    }

    #[test]
    fn tip_type_indices_beyond_99_are_rejected() {
        let error = DefineTipType::new(
            100,
            false,
            519,
            3600,
            TipSizeCode::Standard,
            TipPickupMethod::OutOfRack,
        )
        .expect_err("the firmware table has indices 0–99");
        assert!(
            error.to_string().contains("tt") && error.to_string().contains("99"),
            "the error names the parameter and its ceiling: {error}"
        );
    }

    #[test]
    fn machine_configuration_bits_decode_installed_options() {
        let config = RequestMachineConfiguration::parse_response("kb0Akp08")
            .expect("a well-formed RM reply parses");
        assert!(config.has_iswap(), "bit 1 of 0x0A is set");
        assert!(config.has_autoload(), "bit 3 of 0x0A is set");
        assert!(!config.has_1000ul_channels(), "bit 0 of 0x0A is clear");
        assert_eq!(config.channel_count, 8, "kp carries the channel count");
    }

    #[test]
    fn extended_configuration_decodes_head_presence_and_tip_waste_x() {
        let payload = "ka000002ke00000000xt04xa08xw04350xl00xn00xr00xo00xm00001xx00001xu0000xv0000kc0kr0ys000kl000km000ym0000yu0000yx0000";
        let config = RequestExtendedConfiguration::parse_response(payload)
            .expect("a well-formed QM reply parses");
        assert!(config.has_core96_head(), "ka bit 1 marks the CoRe 96 head");
        assert!(!config.has_xl_channels(), "ka bit 13 is clear");
        assert_eq!(
            config.tip_waste_x,
            TenthMm(4350),
            "xw is the tip-waste X in 0.1 mm"
        );
    }

    #[test]
    fn travel_ranges_parse_from_bare_integers() {
        let ranges = RequestMaxXTravel::parse_response("00100 15450 00000 00000")
            .expect("four bare integers parse");
        assert_eq!(
            ranges.left_max, 15450,
            "the second value is the left arm's maximum"
        );
    }

    fn expects_reply<C: Command>(_: &C) -> bool {
        C::EXPECTS_REPLY
    }

    #[test]
    fn the_no_reply_commands_are_marked_as_such() {
        assert!(
            !expects_reply(&TriggerNextStep),
            "NS produces no reply at all"
        );
        assert!(!expects_reply(&SetNotStop), "AB produces no reply at all");
        assert_eq!(
            TriggerNextStep.to_wire(None),
            "C0NS",
            "NS is sent without an id"
        );
    }
}
