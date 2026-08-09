//! Command framing: module addresses, command ids, and fixed-width parameter
//! serialization.
//!
//! A command frame is plain ASCII with no separators and no terminator:
//!
//! ```text
//! <module:2><command:2>[id<4 digits>]<param name:2><value><param name:2><value>...
//! ```
//!
//! Every value is fixed-width and zero-padded; per-channel values are
//! space-separated fixed-width blocks with a trailing `&` when fewer entries
//! than the machine's channel count are given (the firmware treats the
//! remaining channels as don't-care).

use std::fmt::Write as _;

/// A firmware module address: the two ASCII characters every command and
/// response begins with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Module {
    /// `C0` — the master controller. Master commands are exclusive: the
    /// firmware runs them alone, so the session drains slave commands first.
    Master,
    /// `X0` — the X drives.
    XDrives,
    /// `I0` — the autoload.
    Autoload,
    /// `W1`/`W2` — the wash stations.
    WashStation(u8),
    /// `T1`/`T2`/… — temperature carriers and heater-shakers, addressed by
    /// 1-based index.
    TemperatureCarrier(u8),
    /// `R0` — the iSWAP plate gripper.
    Iswap,
    /// `P1`–`P9`, `PA`–`PG` — pipetting channels 1–16, addressed by 0-based
    /// channel index.
    PipettingChannel(u8),
    /// `H0` — the CoRe 96 head.
    Head96,
    /// `HW`/`HU`/`HV` — pump stations 1–3.
    PumpStation(u8),
    /// `N0` — the nano dispenser.
    NanoDispenser,
    /// `D0` — the 384 head.
    Head384,
    /// `NP` — the nano pressure controller.
    NanoPressure,
}

/// The channel digits for pipetting channels: channel 1 is `P1`, channel 10
/// is `PA`, channel 16 is `PG`.
const CHANNEL_DIGITS: &[u8; 16] = b"123456789ABCDEFG";

impl Module {
    /// The two-character wire address.
    pub fn address(self) -> [u8; 2] {
        match self {
            Module::Master => *b"C0",
            Module::XDrives => *b"X0",
            Module::Autoload => *b"I0",
            Module::WashStation(n) => [b'W', b'0' + n],
            Module::TemperatureCarrier(n) => [b'T', b'0' + n],
            Module::Iswap => *b"R0",
            Module::PipettingChannel(i) => [b'P', CHANNEL_DIGITS[usize::from(i) % 16]],
            Module::Head96 => *b"H0",
            Module::PumpStation(n) => [b'H', b"WUV"[usize::from(n.saturating_sub(1)) % 3]],
            Module::NanoDispenser => *b"N0",
            Module::Head384 => *b"D0",
            Module::NanoPressure => *b"NP",
        }
    }

    /// Parses a two-character wire address back into a module, when it is one
    /// this crate knows.
    pub fn from_address(address: &str) -> Option<Module> {
        let bytes = address.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        match bytes {
            b"C0" => Some(Module::Master),
            b"X0" => Some(Module::XDrives),
            b"I0" => Some(Module::Autoload),
            b"R0" => Some(Module::Iswap),
            b"H0" => Some(Module::Head96),
            b"HW" => Some(Module::PumpStation(1)),
            b"HU" => Some(Module::PumpStation(2)),
            b"HV" => Some(Module::PumpStation(3)),
            b"N0" => Some(Module::NanoDispenser),
            b"D0" => Some(Module::Head384),
            b"NP" => Some(Module::NanoPressure),
            [b'W', n @ b'1'..=b'2'] => Some(Module::WashStation(n - b'0')),
            [b'T', n @ b'1'..=b'9'] => Some(Module::TemperatureCarrier(n - b'0')),
            [b'P', digit] => CHANNEL_DIGITS
                .iter()
                .position(|d| d == digit)
                .map(|i| Module::PipettingChannel(i as u8)),
            _ => None,
        }
    }
}

/// A firmware command id: 1–9999, zero-padded to four digits on the wire.
/// Id 0 never appears; the session allocator wraps 9999 back to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(u16);

impl CommandId {
    /// The lowest id the allocator hands out.
    pub const FIRST: CommandId = CommandId(1);
    /// The highest id before the allocator wraps back to 1.
    pub const LAST: CommandId = CommandId(9999);

    /// Builds an id, rejecting 0 and values above 9999.
    pub fn new(id: u16) -> Option<CommandId> {
        (1..=9999).contains(&id).then_some(CommandId(id))
    }

    /// The id after this one, wrapping 9999 to 1.
    pub fn next(self) -> CommandId {
        if self.0 >= 9999 {
            CommandId(1)
        } else {
            CommandId(self.0 + 1)
        }
    }

    pub fn value(self) -> u16 {
        self.0
    }

    /// Parses the four digits following `id` in a frame.
    pub fn parse(digits: &str) -> Option<CommandId> {
        let id: u16 = digits.parse().ok()?;
        CommandId::new(id)
    }
}

/// Per-channel values for a list parameter, carrying the machine channel
/// count so the serializer knows whether a trailing `&` (don't-care marker
/// for the remaining channels) is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelValues<T> {
    entries: Vec<T>,
    machine_channels: usize,
}

/// How unused channel positions are filled when densifying sparse
/// per-channel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// Unused positions carry zero. Coordinate lists (`xp`, `yp`) use this.
    Zero,
    /// Unused positions carry a copy of the first used value. Liquid
    /// parameter lists use this: the firmware ignores the value but requires
    /// a syntactically valid entry.
    FirstValue,
}

/// The error raised when per-channel inputs are inconsistent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChannelValuesError {
    #[error(
        "channel index {channel} is outside this machine's {machine_channels} pipetting channels"
    )]
    ChannelOutOfRange {
        channel: usize,
        machine_channels: usize,
    },
    #[error("channel {channel} is listed twice; each channel may carry at most one value")]
    DuplicateChannel { channel: usize },
    #[error("at least one channel must carry a value")]
    Empty,
    #[error("the list has {entries} entries but the machine has only {machine_channels} channels")]
    TooManyEntries {
        entries: usize,
        machine_channels: usize,
    },
}

impl<T: Copy> ChannelValues<T> {
    /// Wraps an already-dense entry list. The list must not exceed the
    /// machine channel count.
    pub fn dense(entries: Vec<T>, machine_channels: usize) -> Result<Self, ChannelValuesError> {
        if entries.is_empty() {
            return Err(ChannelValuesError::Empty);
        }
        if entries.len() > machine_channels {
            return Err(ChannelValuesError::TooManyEntries {
                entries: entries.len(),
                machine_channels,
            });
        }
        Ok(ChannelValues {
            entries,
            machine_channels,
        })
    }

    /// Densifies sparse `(channel, value)` pairs: entries run from channel 0
    /// through the highest used channel, plus one trailing don't-care entry
    /// when the machine has more channels. Unused positions are filled per
    /// `fill`.
    pub fn from_sparse(
        pairs: &[(usize, T)],
        machine_channels: usize,
        fill: Fill,
        zero: T,
    ) -> Result<Self, ChannelValuesError> {
        if pairs.is_empty() {
            return Err(ChannelValuesError::Empty);
        }
        let mut highest = 0usize;
        for &(channel, _) in pairs {
            if channel >= machine_channels {
                return Err(ChannelValuesError::ChannelOutOfRange {
                    channel,
                    machine_channels,
                });
            }
            highest = highest.max(channel);
        }
        let len = (highest + 2).min(machine_channels);
        let filler = match fill {
            Fill::Zero => zero,
            Fill::FirstValue => pairs[0].1,
        };
        let mut entries = vec![filler; len];
        let mut seen = vec![false; len];
        for &(channel, value) in pairs {
            if seen[channel] {
                return Err(ChannelValuesError::DuplicateChannel { channel });
            }
            seen[channel] = true;
            entries[channel] = value;
        }
        Ok(ChannelValues {
            entries,
            machine_channels,
        })
    }

    pub fn entries(&self) -> &[T] {
        &self.entries
    }

    pub fn machine_channels(&self) -> usize {
        self.machine_channels
    }

    /// Whether the serialized list needs the trailing `&` don't-care marker.
    pub fn is_partial(&self) -> bool {
        self.entries.len() < self.machine_channels
    }

    /// Rebuilds the list with each entry mapped, keeping the layout.
    pub fn map<U: Copy>(&self, f: impl Fn(T) -> U) -> ChannelValues<U> {
        ChannelValues {
            entries: self.entries.iter().map(|&v| f(v)).collect(),
            machine_channels: self.machine_channels,
        }
    }
}

/// The channel pattern (`tm`): which channels a command addresses. Unused
/// trailing positions in sibling value lists are don't-care entries.
pub type ChannelPattern = ChannelValues<bool>;

impl ChannelPattern {
    /// Builds the pattern for a set of used channel indices, with the same
    /// densification layout as [`ChannelValues::from_sparse`].
    pub fn from_channels(
        channels: &[usize],
        machine_channels: usize,
    ) -> Result<ChannelPattern, ChannelValuesError> {
        let pairs: Vec<(usize, bool)> = channels.iter().map(|&c| (c, true)).collect();
        ChannelValues::from_sparse(&pairs, machine_channels, Fill::Zero, false)
    }

    /// The 0-based indices of the channels this pattern addresses.
    pub fn used_channels(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, &used)| used.then_some(i))
            .collect()
    }
}

/// Assembles one command frame. Parameter widths and order are declared at
/// the call site, one call per parameter, so a command's encoder reads as a
/// table.
#[derive(Debug)]
pub struct FrameBuilder {
    buf: String,
}

/// Asserts that a value fits its declared field width. Command constructors
/// validate ranges before encoding, so an overflow here is a crate bug, not
/// a caller error.
fn assert_fits(name: &str, width: usize, value: u64) {
    let max = 10u64.pow(width as u32) - 1;
    assert!(
        value <= max,
        "value {value} for parameter {name} does not fit its {width}-digit field; \
         the command constructor failed to validate its range"
    );
}

impl FrameBuilder {
    /// Starts a frame for a command with no id. The firmware sends no reply
    /// to id-less commands unless the response is matched by module and code.
    pub fn new(module: Module, code: &str) -> FrameBuilder {
        let address = module.address();
        let mut buf = String::new();
        buf.push(address[0] as char);
        buf.push(address[1] as char);
        buf.push_str(code);
        FrameBuilder { buf }
    }

    /// Starts a frame with the id parameter, which must come first.
    pub fn with_id(module: Module, code: &str, id: CommandId) -> FrameBuilder {
        let mut builder = FrameBuilder::new(module, code);
        let _ = write!(builder.buf, "id{:04}", id.value());
        builder
    }

    /// Appends an unsigned zero-padded decimal parameter.
    pub fn uint(mut self, name: &str, width: usize, value: u32) -> FrameBuilder {
        assert_fits(name, width, u64::from(value));
        let _ = write!(self.buf, "{name}{value:0width$}");
        self
    }

    /// Appends a signed parameter formatted like `+06`/`-06`: the width
    /// includes the sign character.
    pub fn int(mut self, name: &str, width: usize, value: i32) -> FrameBuilder {
        let digits = width - 1;
        assert_fits(name, digits, value.unsigned_abs().into());
        let sign = if value < 0 { '-' } else { '+' };
        let magnitude = value.unsigned_abs();
        let _ = write!(self.buf, "{name}{sign}{magnitude:0digits$}");
        self
    }

    /// Appends a boolean parameter as `1`/`0`.
    pub fn flag(self, name: &str, value: bool) -> FrameBuilder {
        self.uint(name, 1, u32::from(value))
    }

    /// Appends a raw text parameter with no padding.
    pub fn text(mut self, name: &str, value: &str) -> FrameBuilder {
        self.buf.push_str(name);
        self.buf.push_str(value);
        self
    }

    /// Appends a per-channel unsigned list: space-separated fixed-width
    /// blocks with a trailing `&` when the list covers fewer channels than
    /// the machine has.
    pub fn uint_list(
        mut self,
        name: &str,
        width: usize,
        values: &ChannelValues<u32>,
    ) -> FrameBuilder {
        self.buf.push_str(name);
        for (i, &value) in values.entries().iter().enumerate() {
            assert_fits(name, width, u64::from(value));
            if i > 0 {
                self.buf.push(' ');
            }
            let _ = write!(self.buf, "{value:0width$}");
        }
        if values.is_partial() {
            self.buf.push('&');
        }
        self
    }

    /// Appends a per-channel boolean list.
    pub fn flag_list(self, name: &str, values: &ChannelValues<bool>) -> FrameBuilder {
        self.uint_list(name, 1, &values.map(u32::from))
    }

    /// Appends a bit mask as uppercase hex, least-significant bit first in
    /// `bits`. The CoRe 96 pattern `cw` is 96 bits (24 hex chars, bit 0 =
    /// well A1); autoload LED masks are 56 bits (14 hex chars).
    pub fn hex_mask(mut self, name: &str, hex_chars: usize, bits: &[bool]) -> FrameBuilder {
        assert!(
            bits.len() <= hex_chars * 4,
            "a {hex_chars}-hex-character mask holds {} bits but {} were supplied",
            hex_chars * 4,
            bits.len()
        );
        let mut nibbles = vec![0u8; hex_chars];
        for (i, &bit) in bits.iter().enumerate() {
            if bit {
                nibbles[i / 4] |= 1 << (i % 4);
            }
        }
        self.buf.push_str(name);
        for nibble in nibbles.iter().rev() {
            let _ = write!(self.buf, "{nibble:X}");
        }
        self
    }

    /// Finishes the frame as the ASCII string written to the wire.
    pub fn build(self) -> String {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_starts_with_module_code_and_zero_padded_id() {
        let frame = FrameBuilder::with_id(
            Module::Master,
            "TT",
            CommandId::new(1).expect("1 is a valid id"),
        )
        .uint("tt", 2, 1)
        .build();
        assert_eq!(
            frame, "C0TTid0001tt01",
            "the id is four digits and comes before all parameters"
        );
    }

    #[test]
    fn a_partial_channel_list_ends_with_an_ampersand() {
        let values =
            ChannelValues::dense(vec![1179, 1179, 0], 8).expect("three entries fit eight channels");
        let frame = FrameBuilder::new(Module::Master, "TP")
            .uint_list("xp", 5, &values)
            .build();
        assert_eq!(
            frame, "C0TPxp01179 01179 00000&",
            "entries are five digits, space separated, with a trailing don't-care marker"
        );
    }

    #[test]
    fn a_full_channel_list_has_no_ampersand() {
        let values = ChannelValues::dense(vec![1, 2], 2).expect("two entries fit two channels");
        let frame = FrameBuilder::new(Module::Master, "ZZ")
            .uint_list("yp", 4, &values)
            .build();
        assert_eq!(
            frame, "C0ZZyp0001 0002",
            "a list covering every channel needs no marker"
        );
    }

    #[test]
    fn sparse_values_densify_with_a_trailing_dont_care_entry() {
        let values = ChannelValues::from_sparse(&[(4, 1179u32), (5, 1179)], 8, Fill::Zero, 0)
            .expect("channels 4 and 5 exist on an 8-channel machine");
        assert_eq!(
            values.entries(),
            &[0, 0, 0, 0, 1179, 1179, 0],
            "entries run to the highest used channel plus one don't-care position"
        );
        assert!(
            values.is_partial(),
            "seven entries on an eight-channel machine leave one don't-care channel"
        );
    }

    #[test]
    fn liquid_parameters_fill_unused_positions_with_the_first_value() {
        let values = ChannelValues::from_sparse(&[(0, 2000u32)], 8, Fill::FirstValue, 0)
            .expect("channel 0 exists");
        assert_eq!(
            values.entries(),
            &[2000, 2000],
            "the don't-care position repeats the first real value so the field stays syntactically valid"
        );
    }

    #[test]
    fn the_96_bit_head_pattern_puts_well_a1_in_the_least_significant_bit() {
        let bits = vec![true; 96];
        let frame = FrameBuilder::new(Module::Master, "EA")
            .hex_mask("cw", 24, &bits)
            .build();
        assert_eq!(
            frame, "C0EAcwFFFFFFFFFFFFFFFFFFFFFFFF",
            "a full pattern is 24 F characters"
        );

        let mut only_a1 = vec![false; 96];
        only_a1[0] = true;
        let frame = FrameBuilder::new(Module::Master, "EA")
            .hex_mask("cw", 24, &only_a1)
            .build();
        assert_eq!(
            frame, "C0EAcw000000000000000000000001",
            "well A1 alone sets the least significant bit"
        );
    }

    #[test]
    fn channel_addresses_run_p1_through_pg() {
        assert_eq!(
            Module::PipettingChannel(0).address(),
            *b"P1",
            "channel index 0 is module P1"
        );
        assert_eq!(
            Module::PipettingChannel(9).address(),
            *b"PA",
            "channel index 9 is module PA"
        );
        assert_eq!(
            Module::PipettingChannel(15).address(),
            *b"PG",
            "channel index 15 is module PG"
        );
        assert_eq!(
            Module::from_address("PA"),
            Some(Module::PipettingChannel(9)),
            "the address parses back to the same channel"
        );
    }

    #[test]
    fn command_ids_wrap_from_9999_to_1() {
        assert_eq!(
            CommandId::LAST.next(),
            CommandId::FIRST,
            "the id space skips 0"
        );
        assert_eq!(CommandId::new(0), None, "0 is never a valid id");
        assert_eq!(CommandId::new(10000), None, "ids are at most four digits");
    }

    #[test]
    fn signed_parameters_carry_an_explicit_sign_inside_the_width() {
        let frame = FrameBuilder::new(Module::Master, "ZZ")
            .int("kf", 3, -6)
            .build();
        assert_eq!(
            frame, "C0ZZkf-06",
            "width 3 leaves two digits after the sign"
        );
    }
}
