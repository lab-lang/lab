//! The 64-byte HID report codec: pure encode and decode, no I/O.
//!
//! Every packet is exactly [`PACKET_BYTES`] long and all integers are
//! little-endian:
//!
//! - bytes 0–1: report id as a LE `u16`
//! - bytes 2–61: payload, zero-padded
//! - bytes 62–63: routing tag ([`RoutingTag`])
//!
//! Responses are matched by comparing the leading LE `u16` report id
//! against the request's. The absorbance reports carry full session
//! support; the Luminescence 96's reports ([`LUMINESCENCE_TRIGGER`],
//! [`LUMINESCENCE_CHUNK`]) and the LED bar reports are encode/decode only.

use thiserror::Error;

/// Every HID report is exactly this long.
pub const PACKET_BYTES: usize = 64;

/// Payload capacity between the report id and the routing tag.
const PAYLOAD_BYTES: usize = 60;

/// One wire packet.
pub type Packet = [u8; PACKET_BYTES];

/// Supported-reports query and measurement preamble (`0x0010`).
pub const SUPPORTED_REPORTS: u16 = 0x0010;
/// Optional keep-alive (`0x0040`); the modern flow does not require it.
pub const HEARTBEAT: u16 = 0x0040;
/// API version query (`0x0050`).
pub const API_VERSION: u16 = 0x0050;
/// Abort (`0x0060`): names the report id whose measurement should stop.
pub const ABORT: u16 = 0x0060;
/// Firmware component versions query (`0x0080`).
pub const VERSIONS: u16 = 0x0080;
/// Device data field read (`0x0200`).
pub const DEVICE_DATA: u16 = 0x0200;
/// Status query (`0x0300`); the post-measurement status check is the
/// authoritative error gate.
pub const STATUS: u16 = 0x0300;
/// Environment query (`0x0310`): temperature, humidity, acceleration.
pub const ENVIRONMENT: u16 = 0x0310;
/// Absorbance measurement trigger (`0x0320`).
pub const ABSORBANCE_TRIGGER: u16 = 0x0320;
/// Available-wavelengths query (`0x0330`).
pub const AVAILABLE_WAVELENGTHS: u16 = 0x0330;
/// Luminescence measurement trigger (`0x0340`).
pub const LUMINESCENCE_TRIGGER: u16 = 0x0340;
/// LED bar per-pixel colors (`0x0350`).
pub const LED_BAR_PIXELS: u16 = 0x0350;
/// LED bar effect (`0x0351`).
pub const LED_BAR_EFFECT: u16 = 0x0351;
/// Absorbance result chunk (`0x0500`), device to host.
pub const ABSORBANCE_CHUNK: u16 = 0x0500;
/// Luminescence result chunk (`0x0600`), device to host.
pub const LUMINESCENCE_CHUNK: u16 = 0x0600;

/// Device-data field indices for [`device_data_request`].
pub const FIELD_DEVICE_ID: u16 = 0;
pub const FIELD_DEVICE_NAME: u16 = 1;
pub const FIELD_MANUFACTURER: u16 = 2;
pub const FIELD_SERIAL_NO: u16 = 3;
pub const FIELD_FIRMWARE_VERSION: u16 = 4;
/// Field 7 is read fire-and-forget as a measurement preamble; the firmware
/// does not document what it names.
pub const FIELD_MEASUREMENT_PREAMBLE: u16 = 7;
pub const FIELD_REF_NUMBER: u16 = 8;

/// The number of LED bar pixels addressed by [`led_bar_pixels`].
pub const LED_BAR_PIXEL_COUNT: usize = 20;

/// The trailing two bytes of every packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingTag {
    /// `\x00\x00`, the default tag.
    Legacy,
    /// `\x00\x40`, required on the absorbance trigger and LED writes — the
    /// firmware silently drops those reports under the legacy tag.
    Command,
    /// `\x80\x40`, carried by query round-trips in observed traffic;
    /// queries answer under either tag, so this matches the wire, it does
    /// not gate correctness.
    Query,
}

impl RoutingTag {
    pub const fn bytes(self) -> [u8; 2] {
        match self {
            RoutingTag::Legacy => [0x00, 0x00],
            RoutingTag::Command => [0x00, 0x40],
            RoutingTag::Query => [0x80, 0x40],
        }
    }
}

/// The report id of any packet: its leading LE `u16`.
pub fn report_id(packet: &Packet) -> u16 {
    u16::from_le_bytes([packet[0], packet[1]])
}

fn packet(report_id: u16, payload: &[u8], routing: RoutingTag) -> Packet {
    debug_assert!(payload.len() <= PAYLOAD_BYTES);
    let mut bytes = [0u8; PACKET_BYTES];
    bytes[0..2].copy_from_slice(&report_id.to_le_bytes());
    bytes[2..2 + payload.len()].copy_from_slice(payload);
    bytes[62..64].copy_from_slice(&routing.bytes());
    bytes
}

/// The supported-reports request (`0x0010`). The reply arrives as
/// [`SupportedReportsChunk`]s. The same report is also written
/// fire-and-forget with the legacy tag as a measurement preamble, so the
/// caller picks the routing.
pub fn supported_reports_request(routing: RoutingTag) -> Packet {
    packet(SUPPORTED_REPORTS, &[], routing)
}

/// The keep-alive heartbeat (`0x0040`).
pub fn heartbeat() -> Packet {
    packet(HEARTBEAT, &[1], RoutingTag::Legacy)
}

/// The API version query (`0x0050`); decode the reply with
/// [`decode_api_version`].
pub fn api_version_request() -> Packet {
    packet(API_VERSION, &[], RoutingTag::Query)
}

/// The abort report (`0x0060`) naming the report to stop. The firmware
/// answers nothing — it simply stops emitting result chunks — so the read
/// loop needs its own cancellation flag.
pub fn abort(report_to_abort: u16) -> Packet {
    packet(ABORT, &report_to_abort.to_le_bytes(), RoutingTag::Legacy)
}

/// The firmware versions query (`0x0080`); decode the reply with
/// [`Versions::decode`].
pub fn versions_request() -> Packet {
    packet(VERSIONS, &[], RoutingTag::Query)
}

/// A device-data field read (`0x0200`); decode the reply with
/// [`DeviceDataReply::decode`]. [`FIELD_MEASUREMENT_PREAMBLE`] is also
/// written fire-and-forget with the legacy tag before a trigger, so the
/// caller picks the routing.
pub fn device_data_request(field_index: u16, routing: RoutingTag) -> Packet {
    let mut payload = [0u8; 3];
    payload[0..2].copy_from_slice(&field_index.to_le_bytes());
    packet(DEVICE_DATA, &payload, routing)
}

/// The status query (`0x0300`); decode the reply with [`Status::decode`].
pub fn status_request() -> Packet {
    packet(STATUS, &[], RoutingTag::Query)
}

/// The environment query (`0x0310`); decode the reply with
/// [`Environment::decode`].
pub fn environment_request() -> Packet {
    packet(ENVIRONMENT, &[], RoutingTag::Query)
}

/// The available-wavelengths query (`0x0330`); decode the reply with
/// [`decode_available_wavelengths`].
pub fn available_wavelengths_request() -> Packet {
    packet(AVAILABLE_WAVELENGTHS, &[], RoutingTag::Query)
}

/// An absorbance measurement trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbsorbanceTrigger {
    pub signal_wavelength_nm: i16,
    pub reference_wavelength_nm: i16,
    /// A reference measurement initializes the photodiode reference; every
    /// session performs one before its first real read.
    pub is_reference: bool,
}

/// Encodes the absorbance trigger (`0x0320`). The command routing tag is
/// mandatory: the firmware silently drops triggers sent under the legacy
/// tag.
pub fn absorbance_trigger(trigger: &AbsorbanceTrigger) -> Packet {
    let mut payload = [0u8; 6];
    payload[0..2].copy_from_slice(&trigger.signal_wavelength_nm.to_le_bytes());
    payload[2..4].copy_from_slice(&trigger.reference_wavelength_nm.to_le_bytes());
    payload[4] = u8::from(trigger.is_reference);
    // payload[5] is the flags byte; no flag is documented.
    packet(ABSORBANCE_TRIGGER, &payload, RoutingTag::Command)
}

/// A 96-well selection mask, twelve bytes LSB-first: well index
/// `row * 12 + column` (row-major, A1 first) lives in byte `index / 8`,
/// bit `index % 8`, so A1 is byte 0 bit 0 and H12 is byte 11 bit 7.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WellMask([u8; 12]);

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("well ({row}, {column}) is outside the 8×12 plate")]
pub struct WellOutOfRange {
    pub row: usize,
    pub column: usize,
}

impl WellMask {
    /// No well selected.
    pub const fn none() -> WellMask {
        WellMask([0u8; 12])
    }

    /// Every one of the 96 wells selected.
    pub const fn all() -> WellMask {
        WellMask([0xFF; 12])
    }

    /// Selects one well by zero-based row and column.
    pub fn set(&mut self, row: usize, column: usize) -> Result<(), WellOutOfRange> {
        if row >= 8 || column >= 12 {
            return Err(WellOutOfRange { row, column });
        }
        let index = row * 12 + column;
        self.0[index / 8] |= 1 << (index % 8);
        Ok(())
    }

    /// The wire form.
    pub const fn bytes(&self) -> [u8; 12] {
        self.0
    }
}

/// A luminescence measurement trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuminescenceTrigger {
    pub integration_time_us: i32,
    pub well_mask: WellMask,
    pub is_reference: bool,
}

/// Encodes the luminescence trigger (`0x0340`).
pub fn luminescence_trigger(trigger: &LuminescenceTrigger) -> Packet {
    let mut payload = [0u8; 18];
    payload[0..4].copy_from_slice(&trigger.integration_time_us.to_le_bytes());
    payload[4..16].copy_from_slice(&trigger.well_mask.bytes());
    payload[16] = u8::from(trigger.is_reference);
    // payload[17] is the flags byte; no flag is documented.
    packet(LUMINESCENCE_TRIGGER, &payload, RoutingTag::Legacy)
}

/// One LED bar pixel color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Encodes the per-pixel LED bar report (`0x0350`): twenty RGB triples.
/// The command routing tag is mandatory: the firmware silently drops LED
/// writes sent under the legacy tag.
pub fn led_bar_pixels(pixels: &[Rgb; LED_BAR_PIXEL_COUNT]) -> Packet {
    let mut payload = [0u8; LED_BAR_PIXEL_COUNT * 3];
    for (chunk, pixel) in payload.chunks_exact_mut(3).zip(pixels) {
        chunk.copy_from_slice(&[pixel.r, pixel.g, pixel.b]);
    }
    packet(LED_BAR_PIXELS, &payload, RoutingTag::Command)
}

/// The LED bar animation programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedEffect {
    Solid,
    Progress,
    Cylon,
    Rainbow,
    Blinking,
    Breathing,
}

impl LedEffect {
    pub const fn code(self) -> u8 {
        match self {
            LedEffect::Solid => 0x00,
            LedEffect::Progress => 0x01,
            LedEffect::Cylon => 0x02,
            LedEffect::Rainbow => 0x03,
            LedEffect::Blinking => 0x04,
            LedEffect::Breathing => 0x05,
        }
    }
}

/// An LED bar effect write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedBarEffect {
    pub effect: LedEffect,
    pub color: Rgb,
    /// Fill level for [`LedEffect::Progress`] (0 empty, 255 full); every
    /// other effect ignores it.
    pub effect_state: u8,
    /// Reduces brightness (flag `0x01`).
    pub low_power: bool,
    /// Overrides an unexpired previous `duration_ms` (flag `0x10`).
    pub force: bool,
    pub duration_ms: u32,
}

/// Encodes the LED bar effect report (`0x0351`). The command routing tag
/// is mandatory: the firmware silently drops LED writes sent under the
/// legacy tag.
pub fn led_bar_effect(effect: &LedBarEffect) -> Packet {
    let flags = u8::from(effect.low_power) | (u8::from(effect.force) << 4);
    let mut payload = [0u8; 10];
    payload[0] = effect.effect.code();
    payload[1..4].copy_from_slice(&[effect.color.r, effect.color.g, effect.color.b]);
    payload[4] = effect.effect_state;
    payload[5] = flags;
    payload[6..10].copy_from_slice(&effect.duration_ms.to_le_bytes());
    packet(LED_BAR_EFFECT, &payload, RoutingTag::Command)
}

/// The error raised when a packet does not decode as the expected report.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ReportDecodeError {
    #[error("the packet carries report 0x{found:04X} where report 0x{expected:04X} was expected")]
    UnexpectedReport { expected: u16, found: u16 },
}

/// Yields the 60-byte payload after checking the report id.
fn payload(packet: &Packet, expected: u16) -> Result<&[u8], ReportDecodeError> {
    let found = report_id(packet);
    if found != expected {
        return Err(ReportDecodeError::UnexpectedReport { expected, found });
    }
    Ok(&packet[2..62])
}

/// A sequential reader over a payload. Every decoder reads a fixed layout
/// well inside the 60-byte payload, so reads never run off the end.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Cursor<'a> {
        Cursor { bytes, at: 0 }
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        let taken: [u8; N] = self.bytes[self.at..self.at + N]
            .try_into()
            .expect("fixed report layouts stay inside the 60-byte payload");
        self.at += N;
        taken
    }

    fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take())
    }

    fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.take())
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }

    fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take())
    }
}

/// One chunk of the supported-reports reply (`0x0010`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportedReportsChunk {
    pub seq: u8,
    pub seq_len: u8,
    /// The report ids this chunk carries; zero-valued padding entries are
    /// dropped.
    pub report_ids: Vec<u16>,
}

impl SupportedReportsChunk {
    pub fn decode(packet: &Packet) -> Result<SupportedReportsChunk, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, SUPPORTED_REPORTS)?);
        let seq = cursor.u8();
        let seq_len = cursor.u8();
        let report_ids = (0..29)
            .map(|_| cursor.u16())
            .filter(|&id| id != 0)
            .collect();
        Ok(SupportedReportsChunk {
            seq,
            seq_len,
            report_ids,
        })
    }
}

/// Decodes the API version reply (`0x0050`).
pub fn decode_api_version(packet: &Packet) -> Result<u32, ReportDecodeError> {
    Ok(Cursor::new(payload(packet, API_VERSION)?).u32())
}

/// The firmware component versions (`0x0080`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Versions {
    pub system: u32,
    pub stm: u32,
    pub stm_dev: u32,
    pub esp: u32,
    pub esp_dev: u32,
    pub stm_bootloader: u32,
}

impl Versions {
    pub fn decode(packet: &Packet) -> Result<Versions, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, VERSIONS)?);
        Ok(Versions {
            system: cursor.u32(),
            stm: cursor.u32(),
            stm_dev: cursor.u32(),
            esp: cursor.u32(),
            esp_dev: cursor.u32(),
            stm_bootloader: cursor.u32(),
        })
    }
}

/// A typed device-data field value, per the reply's type flags.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceDataValue {
    Integer(u32),
    Text(String),
    Boolean(bool),
    Float(f32),
    /// The raw field bytes, for type codes the protocol does not name.
    Raw(Vec<u8>),
}

/// A device-data field reply (`0x0200`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDataReply {
    pub field_index: u16,
    pub flags: u8,
    pub data: [u8; 52],
}

impl DeviceDataReply {
    pub fn decode(packet: &Packet) -> Result<DeviceDataReply, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, DEVICE_DATA)?);
        let field_index = cursor.u16();
        let flags = cursor.u8();
        let data = cursor.take::<52>();
        Ok(DeviceDataReply {
            field_index,
            flags,
            data,
        })
    }

    /// Whether the field continues in a further reply (`flags & 0x10`).
    pub fn has_more_data(&self) -> bool {
        self.flags & 0x10 != 0
    }

    /// The field value typed by `flags & 0x0F`: 1 integer, 2 string,
    /// 3 boolean, 4 float; anything else is surfaced raw. Strings end at
    /// the first NUL.
    pub fn value(&self) -> DeviceDataValue {
        match self.flags & 0x0F {
            1 => DeviceDataValue::Integer(u32::from_le_bytes(
                self.data[0..4]
                    .try_into()
                    .expect("four bytes always fit in a u32"),
            )),
            2 => {
                let text = self.data.split(|&byte| byte == 0).next().unwrap_or(&[]);
                DeviceDataValue::Text(String::from_utf8_lossy(text).into_owned())
            }
            3 => DeviceDataValue::Boolean(self.data[0] != 0),
            4 => DeviceDataValue::Float(f32::from_le_bytes(
                self.data[0..4]
                    .try_into()
                    .expect("four bytes always fit in an f32"),
            )),
            _ => DeviceDataValue::Raw(self.data.to_vec()),
        }
    }
}

/// The plate slot as the device senses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Unknown,
    Empty,
    Occupied,
    Undetermined,
}

impl SlotState {
    /// Maps the wire byte; values outside the documented 0–3 read as
    /// [`SlotState::Unknown`].
    pub fn from_byte(byte: u8) -> SlotState {
        match byte {
            1 => SlotState::Empty,
            2 => SlotState::Occupied,
            3 => SlotState::Undetermined,
            _ => SlotState::Unknown,
        }
    }
}

/// The status reply (`0x0300`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub is_initialized: bool,
    pub slot_state: SlotState,
    pub error_code: u8,
    pub uptime_s: u32,
    pub is_measuring: bool,
    pub boot_completed: bool,
}

impl Status {
    pub fn decode(packet: &Packet) -> Result<Status, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, STATUS)?);
        Ok(Status {
            is_initialized: cursor.u8() != 0,
            slot_state: SlotState::from_byte(cursor.u8()),
            error_code: cursor.u8(),
            uptime_s: cursor.u32(),
            is_measuring: cursor.u8() != 0,
            boot_completed: cursor.u8() != 0,
        })
    }
}

/// The Absorbance 96 firmware error table, read from
/// [`Status::error_code`] after a measurement.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum Abs96FirmwareError {
    #[error("the calibration check failed; rerun the reference measurement")]
    Calibration,
    #[error(
        "ambient light reached the photodiodes; seat the illumination unit and shield the reader"
    )]
    AmbientLight,
    #[error("the firmware reported a USB fault; replug the cable and reopen the device")]
    Usb,
    #[error("the firmware reported a hardware fault")]
    Hardware,
    #[error("the device temperature is outside its operating range")]
    Temperature,
    #[error("no measurement unit is attached; seat the illumination unit on the base")]
    NoMeasurementUnit,
    #[error("the firmware did not acknowledge the measurement")]
    NoAck,
    /// A code outside the documented table, rendered the way the vendor
    /// tooling prints it.
    #[error("errorCode=0x{0:02X}")]
    Unknown(u8),
}

impl Abs96FirmwareError {
    /// Decodes a status error code; `None` means no error (code 0).
    pub fn from_code(code: u8) -> Option<Abs96FirmwareError> {
        match code {
            0 => None,
            1 => Some(Abs96FirmwareError::Calibration),
            2 => Some(Abs96FirmwareError::AmbientLight),
            3 => Some(Abs96FirmwareError::Usb),
            4 => Some(Abs96FirmwareError::Hardware),
            5 => Some(Abs96FirmwareError::Temperature),
            6 => Some(Abs96FirmwareError::NoMeasurementUnit),
            10 => Some(Abs96FirmwareError::NoAck),
            other => Some(Abs96FirmwareError::Unknown(other)),
        }
    }
}

/// The environment reply (`0x0310`), scaled to physical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub temperature_celsius: f64,
    /// Relative humidity in 0..1.
    pub relative_humidity: f64,
    /// Accelerometer x, y, z in g (16384 counts per g).
    pub acceleration_g: [f64; 3],
}

impl Environment {
    pub fn decode(packet: &Packet) -> Result<Environment, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, ENVIRONMENT)?);
        let temperature_celsius = f64::from(cursor.i16()) / 100.0;
        let relative_humidity = f64::from(cursor.i16()) / 1000.0;
        let acceleration_g =
            [cursor.i16(), cursor.i16(), cursor.i16()].map(|axis| f64::from(axis) / 16384.0);
        Ok(Environment {
            temperature_celsius,
            relative_humidity,
            acceleration_g,
        })
    }
}

/// Decodes the available-wavelengths reply (`0x0330`): the installed LED
/// wavelengths in nm. The reply carries thirty `i16` slots; zero-valued
/// padding entries are dropped.
pub fn decode_available_wavelengths(packet: &Packet) -> Result<Vec<i16>, ReportDecodeError> {
    let mut cursor = Cursor::new(payload(packet, AVAILABLE_WAVELENGTHS)?);
    Ok((0..30)
        .map(|_| cursor.i16())
        .filter(|&nm| nm != 0)
        .collect())
}

/// One absorbance result chunk (`0x0500`): a plate row of twelve unitless
/// OD values. A 96-well read arrives as `seq_len` = 8 chunks, rows A to H,
/// each row A1 to A12.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbsorbanceChunk {
    pub seq: u8,
    pub seq_len: u8,
    pub signal_wavelength_nm: i16,
    pub reference_wavelength_nm: i16,
    pub duration_ms: u32,
    pub values: [f32; 12],
    /// Undocumented firmware flags; non-zero values are surfaced, never
    /// failed on.
    pub flags: u8,
    /// Running completion percentage, 0..100.
    pub progress: u8,
}

impl AbsorbanceChunk {
    pub fn decode(packet: &Packet) -> Result<AbsorbanceChunk, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, ABSORBANCE_CHUNK)?);
        let seq = cursor.u8();
        let seq_len = cursor.u8();
        let signal_wavelength_nm = cursor.i16();
        let reference_wavelength_nm = cursor.i16();
        let duration_ms = cursor.u32();
        let mut values = [0f32; 12];
        for value in &mut values {
            *value = cursor.f32();
        }
        Ok(AbsorbanceChunk {
            seq,
            seq_len,
            signal_wavelength_nm,
            reference_wavelength_nm,
            duration_ms,
            values,
            flags: cursor.u8(),
            progress: cursor.u8(),
        })
    }
}

/// One luminescence result chunk (`0x0600`): a plate row of twelve RLU
/// values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LuminescenceChunk {
    pub seq: u8,
    pub seq_len: u8,
    pub integration_time_us: u32,
    pub duration_ms: u32,
    pub values: [f32; 12],
    /// Undocumented firmware flags; non-zero values are surfaced, never
    /// failed on.
    pub flags: u8,
    /// Running completion percentage, 0..100.
    pub progress: u8,
}

impl LuminescenceChunk {
    pub fn decode(packet: &Packet) -> Result<LuminescenceChunk, ReportDecodeError> {
        let mut cursor = Cursor::new(payload(packet, LUMINESCENCE_CHUNK)?);
        let seq = cursor.u8();
        let seq_len = cursor.u8();
        let integration_time_us = cursor.u32();
        let duration_ms = cursor.u32();
        let mut values = [0f32; 12];
        for value in &mut values {
            *value = cursor.f32();
        }
        Ok(LuminescenceChunk {
            seq,
            seq_len,
            integration_time_us,
            duration_ms,
            values,
            flags: cursor.u8(),
            progress: cursor.u8(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an inbound packet the way the firmware frames one: report id,
    /// payload, zero routing.
    fn inbound(report_id: u16, payload_bytes: &[u8]) -> Packet {
        packet(report_id, payload_bytes, RoutingTag::Legacy)
    }

    #[test]
    fn the_absorbance_trigger_encodes_byte_exactly_with_the_command_tag() {
        let wire = absorbance_trigger(&AbsorbanceTrigger {
            signal_wavelength_nm: 450,
            reference_wavelength_nm: 0,
            is_reference: false,
        });
        let mut expected = [0u8; 64];
        expected[0..2].copy_from_slice(&[0x20, 0x03]); // 0x0320 LE
        expected[2..4].copy_from_slice(&[0xC2, 0x01]); // 450 LE
        expected[62..64].copy_from_slice(&[0x00, 0x40]);
        assert_eq!(
            wire, expected,
            "a 450 nm trigger is the id, the wavelength, zeros, and the command tag"
        );
    }

    #[test]
    fn the_reference_trigger_raises_the_is_reference_byte() {
        let wire = absorbance_trigger(&AbsorbanceTrigger {
            signal_wavelength_nm: 660,
            reference_wavelength_nm: 0,
            is_reference: true,
        });
        assert_eq!(wire[2..4], [0x94, 0x02], "660 nm little-endian");
        assert_eq!(wire[6], 1, "the is_reference byte is set");
        assert_eq!(wire[62..64], [0x00, 0x40], "the command tag is mandatory");
    }

    #[test]
    fn the_abort_report_names_the_report_to_stop() {
        let wire = abort(ABSORBANCE_TRIGGER);
        let mut expected = [0u8; 64];
        expected[0..2].copy_from_slice(&[0x60, 0x00]); // 0x0060 LE
        expected[2..4].copy_from_slice(&[0x20, 0x03]); // 0x0320 LE
        assert_eq!(
            wire, expected,
            "the abort payload is the aborted report's id under the legacy tag"
        );
    }

    #[test]
    fn the_well_mask_packs_lsb_first_with_a1_at_byte_zero_bit_zero() {
        let mut mask = WellMask::none();
        mask.set(0, 0).expect("A1 is on the plate");
        assert_eq!(mask.bytes()[0], 0x01, "A1 is byte 0 bit 0");
        mask.set(0, 7).expect("A8 is on the plate");
        assert_eq!(mask.bytes()[0], 0x81, "A8 is byte 0 bit 7");
        mask.set(7, 11).expect("H12 is on the plate");
        assert_eq!(mask.bytes()[11], 0x80, "H12 is byte 11 bit 7");
        assert_eq!(
            WellMask::all().bytes(),
            [0xFF; 12],
            "every well raises every bit"
        );
        assert_eq!(
            WellMask::none().set(8, 0),
            Err(WellOutOfRange { row: 8, column: 0 }),
            "row 8 is off an 8-row plate"
        );
    }

    #[test]
    fn the_luminescence_trigger_encodes_byte_exactly_under_the_legacy_tag() {
        let mut mask = WellMask::none();
        mask.set(0, 0).expect("A1 is on the plate");
        let wire = luminescence_trigger(&LuminescenceTrigger {
            integration_time_us: 100_000,
            well_mask: mask,
            is_reference: false,
        });
        let mut expected = [0u8; 64];
        expected[0..2].copy_from_slice(&[0x40, 0x03]); // 0x0340 LE
        expected[2..6].copy_from_slice(&[0xA0, 0x86, 0x01, 0x00]); // 100000 LE
        expected[6] = 0x01; // the A1 mask bit
        assert_eq!(
            wire, expected,
            "integration time, then the twelve mask bytes, under the legacy tag"
        );
    }

    #[test]
    fn led_writes_carry_the_command_tag_the_firmware_requires() {
        let mut pixels = [Rgb::default(); LED_BAR_PIXEL_COUNT];
        pixels[0] = Rgb { r: 1, g: 2, b: 3 };
        pixels[19] = Rgb {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        };
        let wire = led_bar_pixels(&pixels);
        assert_eq!(wire[0..2], [0x50, 0x03], "0x0350 little-endian");
        assert_eq!(wire[2..5], [1, 2, 3], "pixel 0 leads the payload");
        assert_eq!(wire[59..62], [0xAA, 0xBB, 0xCC], "pixel 19 ends it");
        assert_eq!(wire[62..64], [0x00, 0x40], "the command tag is mandatory");

        let effect = led_bar_effect(&LedBarEffect {
            effect: LedEffect::Breathing,
            color: Rgb {
                r: 10,
                g: 20,
                b: 30,
            },
            effect_state: 0,
            low_power: true,
            force: true,
            duration_ms: 5000,
        });
        assert_eq!(effect[0..2], [0x51, 0x03], "0x0351 little-endian");
        assert_eq!(
            effect[2..8],
            [0x05, 10, 20, 30, 0x00, 0x11],
            "effect code, color, state, then low-power|force flags"
        );
        assert_eq!(effect[8..12], [0x88, 0x13, 0x00, 0x00], "5000 ms LE");
        assert_eq!(effect[62..64], [0x00, 0x40], "the command tag is mandatory");
    }

    #[test]
    fn query_requests_carry_the_observed_query_tag() {
        for wire in [
            api_version_request(),
            versions_request(),
            status_request(),
            environment_request(),
            available_wavelengths_request(),
        ] {
            assert_eq!(
                wire[62..64],
                [0x80, 0x40],
                "queries match the observed 0x80 0x40 traffic"
            );
        }
        assert_eq!(heartbeat()[2], 1, "the heartbeat payload is a single 1");
        let preamble = supported_reports_request(RoutingTag::Legacy);
        assert_eq!(
            preamble[62..64],
            [0x00, 0x00],
            "the measurement preamble uses the legacy tag"
        );
    }

    #[test]
    fn the_device_data_request_carries_the_field_index() {
        let wire = device_data_request(FIELD_MEASUREMENT_PREAMBLE, RoutingTag::Legacy);
        assert_eq!(wire[0..2], [0x00, 0x02], "0x0200 little-endian");
        assert_eq!(wire[2..5], [7, 0, 0], "field 7, zero flags");
    }

    #[test]
    fn a_status_reply_decodes_every_field() {
        let mut payload_bytes = [0u8; 9];
        payload_bytes[0] = 1; // initialized
        payload_bytes[1] = 2; // occupied
        payload_bytes[2] = 0; // no error
        payload_bytes[3..7].copy_from_slice(&3600u32.to_le_bytes());
        payload_bytes[7] = 0; // not measuring
        payload_bytes[8] = 1; // boot completed
        let status = Status::decode(&inbound(STATUS, &payload_bytes)).expect("the id matches");
        assert_eq!(
            status,
            Status {
                is_initialized: true,
                slot_state: SlotState::Occupied,
                error_code: 0,
                uptime_s: 3600,
                is_measuring: false,
                boot_completed: true,
            }
        );
    }

    #[test]
    fn firmware_error_codes_decode_to_their_meanings() {
        assert_eq!(Abs96FirmwareError::from_code(0), None, "code 0 is no error");
        assert_eq!(
            Abs96FirmwareError::from_code(2),
            Some(Abs96FirmwareError::AmbientLight)
        );
        assert_eq!(
            Abs96FirmwareError::from_code(10),
            Some(Abs96FirmwareError::NoAck)
        );
        let unknown = Abs96FirmwareError::from_code(0x2A).expect("a non-zero code is an error");
        assert_eq!(
            unknown.to_string(),
            "errorCode=0x2A",
            "unknown codes render as the hex sentinel"
        );
    }

    #[test]
    fn an_absorbance_chunk_decodes_its_row_of_twelve_values() {
        let mut payload_bytes = [0u8; 60];
        payload_bytes[0] = 5; // seq
        payload_bytes[1] = 8; // seq_len
        payload_bytes[2..4].copy_from_slice(&600i16.to_le_bytes());
        payload_bytes[4..6].copy_from_slice(&0i16.to_le_bytes());
        payload_bytes[6..10].copy_from_slice(&65_000u32.to_le_bytes());
        for column in 0..12 {
            let value = 0.25f32 * column as f32;
            payload_bytes[10 + column * 4..14 + column * 4].copy_from_slice(&value.to_le_bytes());
        }
        payload_bytes[58] = 0; // flags
        payload_bytes[59] = 75; // progress
        let chunk = AbsorbanceChunk::decode(&inbound(ABSORBANCE_CHUNK, &payload_bytes))
            .expect("the id matches");
        assert_eq!(chunk.seq, 5);
        assert_eq!(chunk.seq_len, 8);
        assert_eq!(chunk.signal_wavelength_nm, 600);
        assert_eq!(chunk.duration_ms, 65_000);
        assert_eq!(chunk.values[3], 0.75, "column values decode in order");
        assert_eq!(chunk.progress, 75);
    }

    #[test]
    fn a_luminescence_chunk_decodes_its_integration_time_and_row() {
        let mut payload_bytes = [0u8; 60];
        payload_bytes[0] = 2;
        payload_bytes[1] = 8;
        payload_bytes[2..6].copy_from_slice(&2_000_000u32.to_le_bytes());
        payload_bytes[6..10].copy_from_slice(&16_500u32.to_le_bytes());
        payload_bytes[10..14].copy_from_slice(&123.5f32.to_le_bytes());
        payload_bytes[58] = 0x02; // flags
        payload_bytes[59] = 37;
        let chunk = LuminescenceChunk::decode(&inbound(LUMINESCENCE_CHUNK, &payload_bytes))
            .expect("the id matches");
        assert_eq!(chunk.integration_time_us, 2_000_000);
        assert_eq!(chunk.duration_ms, 16_500);
        assert_eq!(chunk.values[0], 123.5);
        assert_eq!(chunk.flags, 0x02, "undocumented flags surface verbatim");
        assert_eq!(chunk.progress, 37);
    }

    #[test]
    fn a_supported_reports_chunk_drops_its_zero_padding() {
        let mut payload_bytes = [0u8; 60];
        payload_bytes[0] = 0; // seq
        payload_bytes[1] = 1; // seq_len
        payload_bytes[2..4].copy_from_slice(&STATUS.to_le_bytes());
        payload_bytes[4..6].copy_from_slice(&ABSORBANCE_TRIGGER.to_le_bytes());
        let chunk = SupportedReportsChunk::decode(&inbound(SUPPORTED_REPORTS, &payload_bytes))
            .expect("the id matches");
        assert_eq!(
            chunk.report_ids,
            vec![STATUS, ABSORBANCE_TRIGGER],
            "only the non-zero entries are report ids"
        );
    }

    #[test]
    fn the_environment_reply_scales_to_physical_units() {
        let mut payload_bytes = [0u8; 10];
        payload_bytes[0..2].copy_from_slice(&2345i16.to_le_bytes());
        payload_bytes[2..4].copy_from_slice(&500i16.to_le_bytes());
        payload_bytes[4..6].copy_from_slice(&16384i16.to_le_bytes());
        payload_bytes[6..8].copy_from_slice(&(-16384i16).to_le_bytes());
        let environment =
            Environment::decode(&inbound(ENVIRONMENT, &payload_bytes)).expect("the id matches");
        assert_eq!(environment.temperature_celsius, 23.45);
        assert_eq!(environment.relative_humidity, 0.5);
        assert_eq!(
            environment.acceleration_g,
            [1.0, -1.0, 0.0],
            "16384 counts is one g"
        );
    }

    #[test]
    fn the_wavelengths_reply_lists_only_installed_leds() {
        let mut payload_bytes = [0u8; 60];
        for (slot, nm) in [450i16, 600, 660].iter().enumerate() {
            payload_bytes[slot * 2..slot * 2 + 2].copy_from_slice(&nm.to_le_bytes());
        }
        let installed =
            decode_available_wavelengths(&inbound(AVAILABLE_WAVELENGTHS, &payload_bytes))
                .expect("the id matches");
        assert_eq!(installed, vec![450, 600, 660], "zero slots are padding");
    }

    #[test]
    fn a_device_data_string_field_ends_at_its_first_nul() {
        let mut payload_bytes = [0u8; 55];
        payload_bytes[0..2].copy_from_slice(&FIELD_SERIAL_NO.to_le_bytes());
        payload_bytes[2] = 0x02; // string type
        payload_bytes[3..9].copy_from_slice(b"BY0042");
        let reply =
            DeviceDataReply::decode(&inbound(DEVICE_DATA, &payload_bytes)).expect("the id matches");
        assert_eq!(reply.field_index, FIELD_SERIAL_NO);
        assert!(!reply.has_more_data(), "no continuation flag is set");
        assert_eq!(reply.value(), DeviceDataValue::Text("BY0042".to_string()));
    }

    #[test]
    fn the_versions_reply_decodes_its_six_components() {
        let mut payload_bytes = [0u8; 24];
        for (slot, version) in [7u32, 6, 0, 5, 0, 4].iter().enumerate() {
            payload_bytes[slot * 4..slot * 4 + 4].copy_from_slice(&version.to_le_bytes());
        }
        let versions = Versions::decode(&inbound(VERSIONS, &payload_bytes)).expect("id matches");
        assert_eq!(
            versions,
            Versions {
                system: 7,
                stm: 6,
                stm_dev: 0,
                esp: 5,
                esp_dev: 0,
                stm_bootloader: 4,
            }
        );
        let api =
            decode_api_version(&inbound(API_VERSION, &3u32.to_le_bytes())).expect("the id matches");
        assert_eq!(api, 3);
    }

    #[test]
    fn a_mismatched_report_id_is_rejected_with_both_ids() {
        let error = Status::decode(&inbound(ABSORBANCE_CHUNK, &[]))
            .expect_err("a chunk is not a status reply");
        assert_eq!(
            error,
            ReportDecodeError::UnexpectedReport {
                expected: STATUS,
                found: ABSORBANCE_CHUNK,
            }
        );
        assert_eq!(
            error.to_string(),
            "the packet carries report 0x0500 where report 0x0300 was expected"
        );
    }
}
