//! Autoload and carrier commands. Encode-only.
//!
//! Tracks are 1-based rail positions on the 22.5 mm pitch; a carrier's end
//! rail is `(carrier.x − 100 + size_x) / 22.5`. Deck presence masks put
//! track 1 in the least significant bit.

use crate::commands::Command;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::{FieldSpec, ResponseParseError, parse_barcodes, parse_fields};

/// The 1-based end rail a carrier occupies: `(x − 100 + size_x) / 22.5`.
pub fn carrier_end_rail(carrier_x_mm: f64, carrier_size_x_mm: f64) -> u32 {
    ((carrier_x_mm - 100.0 + carrier_size_x_mm) / 22.5)
        .round()
        .max(0.0) as u32
}

/// `II` — initialize the autoload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AutoloadInitialize;

impl Command for AutoloadInitialize {
    const CODE: &'static str = "II";
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

/// `IV` — move the autoload wheel to the safe Z position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AutoloadToSafeZ;

impl Command for AutoloadToSafeZ {
    const CODE: &'static str = "IV";
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

/// `QA` — query the autoload's current track (fmt `qa##`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryCurrentTrack;

impl Command for QueryCurrentTrack {
    const CODE: &'static str = "QA";
    type Response = u32;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<u32, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qa", 2)])?;
        Ok(fields.int("qa").unwrap_or(0).max(0) as u32)
    }
}

/// `CQ` — query the autoload type (fmt `cq#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryAutoloadType;

impl Command for QueryAutoloadType {
    const CODE: &'static str = "CQ";
    type Response = u8;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<u8, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("cq", 1)])?;
        Ok(fields.int("cq").unwrap_or(0).clamp(0, 9) as u8)
    }
}

/// Parses a hex presence mask following a two-character field name; bit 0
/// (least significant) is track 1.
fn parse_track_mask(payload: &str, name: &str) -> u64 {
    payload
        .find(name)
        .map(|at| {
            let hex: String = payload[at + name.len()..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .collect();
            u64::from_str_radix(&hex, 16).unwrap_or(0)
        })
        .unwrap_or(0)
}

/// `RC` — read the deck's carrier-presence sensors. The reply is a hex mask
/// after `ce` with track 1 in the least significant bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReadCarrierPresence;

impl Command for ReadCarrierPresence {
    const CODE: &'static str = "RC";
    /// The presence mask; bit 0 = track 1.
    type Response = u64;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<u64, ResponseParseError> {
        Ok(parse_track_mask(payload, "ce"))
    }
}

/// `CS` — scan the loading tray. The reply is a hex mask after `cd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanLoadingTray;

impl Command for ScanLoadingTray {
    const CODE: &'static str = "CS";
    /// The presence mask; bit 0 = track 1.
    type Response = u64;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<u64, ResponseParseError> {
        Ok(parse_track_mask(payload, "cd"))
    }
}

/// `CT` — query single-track carrier presence (fmt `ct#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryTrackPresence {
    track: u32,
}

impl QueryTrackPresence {
    pub fn new(track: u32) -> Result<QueryTrackPresence, CommandError> {
        check_range("cp", "track number", "", f64::from(track), 1.0, 54.0)?;
        Ok(QueryTrackPresence { track })
    }
}

impl Command for QueryTrackPresence {
    const CODE: &'static str = "CT";
    type Response = bool;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("cp", 2, self.track)
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("ct", 1)])?;
        Ok(fields.int("ct") == Some(1))
    }
}

/// `XP` on `I0` — move the autoload to a track. Parking is a move to the
/// machine's maximum track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveAutoloadToTrack {
    track: u32,
}

impl MoveAutoloadToTrack {
    pub fn new(track: u32) -> Result<MoveAutoloadToTrack, CommandError> {
        check_range("xp", "track number", "", f64::from(track), 1.0, 99.0)?;
        Ok(MoveAutoloadToTrack { track })
    }
}

impl Command for MoveAutoloadToTrack {
    const CODE: &'static str = "XP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Autoload
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("xp", 2, self.track)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `CN` — take a carrier out to the loading belt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakeCarrierToBelt {
    track: u32,
}

impl TakeCarrierToBelt {
    pub fn new(track: u32) -> Result<TakeCarrierToBelt, CommandError> {
        check_range("cp", "track number", "", f64::from(track), 1.0, 54.0)?;
        Ok(TakeCarrierToBelt { track })
    }
}

impl Command for TakeCarrierToBelt {
    const CODE: &'static str = "CN";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("cp", 2, self.track)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `CB` — set the 1D barcode symbology mask. `0x7F` accepts any symbology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetBarcodeSymbology {
    /// `bt`: the symbology bit mask.
    pub mask: u8,
}

impl Command for SetBarcodeSymbology {
    const CODE: &'static str = "CB";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.text("bt", &format!("{:02X}", self.mask))
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `CI` — load a carrier and scan its identification barcode. The reply
/// carries the barcode as `bb/<len><data>` (`00` = none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadCarrierWithBarcode {
    /// `cp`: end rail of the carrier.
    pub track: u32,
    /// `bi`: barcode position, 0.1 mm, 0–4700.
    pub barcode_position: u32,
    /// `bw`: reading window width, 0.1 mm.
    pub window_width: u32,
    /// `cv`: reading speed, 0.1 mm/s.
    pub reading_speed: u32,
}

/// The fixed container spacing pattern (`co`) for carrier loads: 96.0 mm.
pub const CONTAINER_SPACING: &str = "0960";

impl Command for LoadCarrierWithBarcode {
    const CODE: &'static str = "CI";
    /// The carrier barcode, when one was read.
    type Response = Option<String>;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("cp", 2, self.track)
            .uint("bi", 4, self.barcode_position)
            .uint("bw", 3, self.window_width)
            .text("co", CONTAINER_SPACING)
            .uint("cv", 4, self.reading_speed)
    }
    fn parse_response(payload: &str) -> Result<Option<String>, ResponseParseError> {
        Ok(parse_barcodes(payload).into_iter().next().flatten())
    }
}

/// `CA` — unload a carrier to the loading tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnloadCarrierToTray {
    /// `cp`: end rail of the carrier.
    pub track: u32,
}

impl Command for UnloadCarrierToTray {
    const CODE: &'static str = "CA";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("cp", 2, self.track)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `CL` — finish loading and scan the container barcodes. The reply is
/// `bb/<b1>/<b2>/...` with `00` marking unread positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinishLoadingWithBarcodes {
    /// `bd`: reading direction code.
    pub reading_direction: u32,
    /// `bp`: reading position of the first barcode, 0.1 mm.
    pub first_barcode_position: u32,
    /// `cn`: containers per carrier.
    pub container_count: u32,
    /// `co`: distance between containers, 0.1 mm.
    pub container_spacing: u32,
    /// `cf`: reading window width, 0.1 mm.
    pub window_width: u32,
    /// `cv`: reading speed, 0.1 mm/s.
    pub reading_speed: u32,
}

impl Command for FinishLoadingWithBarcodes {
    const CODE: &'static str = "CL";
    /// One entry per container, `None` where no barcode was read.
    type Response = Vec<Option<String>>;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("bd", 1, self.reading_direction)
            .uint("bp", 4, self.first_barcode_position)
            .uint("cn", 2, self.container_count)
            .uint("co", 4, self.container_spacing)
            .uint("cf", 3, self.window_width)
            .uint("cv", 4, self.reading_speed)
    }
    fn parse_response(payload: &str) -> Result<Vec<Option<String>>, ResponseParseError> {
        Ok(parse_barcodes(payload))
    }
}

/// `CU` — enable or disable carrier monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetCarrierMonitoring {
    pub enabled: bool,
}

impl Command for SetCarrierMonitoring {
    const CODE: &'static str = "CU";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.flag("cu", self.enabled)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `CP` — set the loading LEDs: a 14-hex-character on mask (`cl`) and blink
/// mask (`cb`). The masks are sent verbatim as uppercase hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetLoadingLeds {
    /// `cl`: the on mask, 56 bits.
    pub on_mask: u64,
    /// `cb`: the blink mask, 56 bits.
    pub blink_mask: u64,
}

impl Command for SetLoadingLeds {
    const CODE: &'static str = "CP";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .text("cl", &format!("{:014X}", self.on_mask))
            .text("cb", &format!("{:014X}", self.blink_mask))
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `CR` — unload a carrier from the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnloadCarrier {
    /// `cp`: end rail of the carrier.
    pub track: u32,
}

impl Command for UnloadCarrier {
    const CODE: &'static str = "CR";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("cp", 2, self.track)
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
    fn carrier_end_rails_derive_from_the_deck_pitch() {
        assert_eq!(
            carrier_end_rail(100.0, 135.0),
            6,
            "a 135 mm carrier at rail 1 ends at rail 6 (135 / 22.5)"
        );
    }

    #[test]
    fn a_carrier_load_encodes_the_fixed_container_spacing() {
        let command = LoadCarrierWithBarcode {
            track: 12,
            barcode_position: 400,
            window_width: 100,
            reading_speed: 800,
        };
        assert_eq!(
            command.to_wire(CommandId::new(9)),
            "C0CIid0009cp12bi0400bw100co0960cv0800",
            "co carries the fixed 96 mm container spacing pattern"
        );
    }

    #[test]
    fn container_barcodes_decode_with_unread_positions_as_none() {
        let barcodes = FinishLoadingWithBarcodes::parse_response("bb/0512345/00/03ABC")
            .expect("a barcode reply parses");
        assert_eq!(
            barcodes,
            vec![Some("12345".to_string()), None, Some("ABC".to_string())],
            "00 segments mean no barcode was read at that container"
        );
    }

    #[test]
    fn presence_masks_put_track_one_in_the_least_significant_bit() {
        let mask = ReadCarrierPresence::parse_response("ce00000000000005")
            .expect("a presence reply parses");
        assert_eq!(mask & 1, 1, "bit 0 is track 1");
        assert_eq!(mask & 0b100, 0b100, "bit 2 is track 3");
    }
}
