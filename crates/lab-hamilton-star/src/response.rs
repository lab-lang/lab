//! Response parsing: the reply envelope, the firmware error section, and
//! schema-driven field extraction.
//!
//! Responses mirror commands: `<module:2><command:2>id####[<error section>]
//! <param data...>`. Field schemas use a compact notation: a two-character
//! field name followed by width markers — `#` a decimal digit, `*` a hex
//! digit, `&` any character — with ` (n)` marking a space-separated
//! per-channel list. Parsing is positional: fields appear in schema order.

use crate::framing::CommandId;

/// The error raised when a reply cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseParseError {
    #[error(
        "the response {response:?} is shorter than the four-character module and command envelope"
    )]
    TooShort { response: String },
    #[error(
        "the response {response:?} carries id digits {digits:?} that are not a valid command id"
    )]
    BadId { response: String, digits: String },
    #[error(
        "the response payload {payload:?} is missing field {field}; expected it at or after offset {offset}"
    )]
    MissingField {
        payload: String,
        field: &'static str,
        offset: usize,
    },
    #[error("field {field} holds {text:?}, which does not parse as a {kind} of width {width}")]
    BadFieldValue {
        field: &'static str,
        text: String,
        kind: &'static str,
        width: usize,
    },
    #[error("the error section in {payload:?} is malformed after {at:?}")]
    BadErrorSection { payload: String, at: String },
}

/// One reply, decoded to its envelope with the payload left raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponse {
    /// The two-character module address the reply came from.
    pub module: String,
    /// The two-character command code the reply answers.
    pub code: String,
    /// The echoed command id; absent on replies to id-less commands.
    pub id: Option<CommandId>,
    /// Everything after the envelope: the error section (if any) followed by
    /// parameter data.
    pub payload: String,
}

impl RawResponse {
    /// Splits a reply into module, code, id, and payload.
    pub fn parse(text: &str) -> Result<RawResponse, ResponseParseError> {
        let trimmed = text.trim_end_matches(['\r', '\n']);
        if trimmed.len() < 4 || !trimmed.is_ascii() {
            return Err(ResponseParseError::TooShort {
                response: text.to_string(),
            });
        }
        let module = trimmed[..2].to_string();
        let code = trimmed[2..4].to_string();
        let mut rest = &trimmed[4..];
        let mut id = None;
        if let Some(after) = rest.strip_prefix("id") {
            let digits: String = after.chars().take(4).collect();
            if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) {
                // Id 0000 appears on unsolicited replies; keep it as no id.
                id = CommandId::parse(&digits);
                rest = &after[4..];
            } else {
                return Err(ResponseParseError::BadId {
                    response: text.to_string(),
                    digits,
                });
            }
        }
        Ok(RawResponse {
            module,
            code,
            id,
            payload: rest.to_string(),
        })
    }

    /// Whether this reply came from the master controller.
    pub fn is_master(&self) -> bool {
        self.module == "C0"
    }
}

/// One `<module><code>/<trace>` entry from a master error section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleEntry {
    /// The two-character module address the entry belongs to.
    pub address: String,
    /// The two-digit error code.
    pub code: u8,
    /// The two-digit trace code.
    pub trace: u8,
}

/// The decoded error section of a reply.
///
/// Master (`C0`) replies embed `er<code:2>/<trace:2>` optionally followed by
/// per-module entries; slave-direct replies carry only `er<trace:2>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorSection {
    /// A master reply: the master's own code/trace plus any per-module
    /// entries.
    Master {
        code: u8,
        trace: u8,
        modules: Vec<ModuleEntry>,
    },
    /// A slave-direct reply: a bare trace code.
    Slave { trace: u8 },
}

fn two_digits(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() {
        Some((bytes[0] - b'0') * 10 + (bytes[1] - b'0'))
    } else {
        None
    }
}

/// Splits the error section out of a reply payload, returning the section
/// (when present) and the payload with the section removed.
pub fn split_error_section(
    payload: &str,
) -> Result<(Option<ErrorSection>, String), ResponseParseError> {
    let Some(er_at) = payload.find("er") else {
        return Ok((None, payload.to_string()));
    };
    let before = &payload[..er_at];
    let after_er = &payload[er_at + 2..];
    let Some(code) = two_digits(after_er) else {
        return Err(ResponseParseError::BadErrorSection {
            payload: payload.to_string(),
            at: after_er.chars().take(8).collect(),
        });
    };
    if after_er.as_bytes().get(2) == Some(&b'/') {
        let Some(trace) = two_digits(&after_er[3..]) else {
            return Err(ResponseParseError::BadErrorSection {
                payload: payload.to_string(),
                at: after_er.chars().take(8).collect(),
            });
        };
        let mut rest = &after_er[5..];
        let mut modules = Vec::new();
        loop {
            let candidate = rest.trim_start_matches(' ');
            let bytes = candidate.as_bytes();
            // A module entry is `<address:2><code:2>/<trace:2>` where the
            // address starts with an uppercase letter.
            if bytes.len() >= 7
                && bytes[0].is_ascii_uppercase()
                && bytes[4] == b'/'
                && let Some(entry_code) = two_digits(&candidate[2..])
                && let Some(entry_trace) = two_digits(&candidate[5..])
            {
                modules.push(ModuleEntry {
                    address: candidate[..2].to_string(),
                    code: entry_code,
                    trace: entry_trace,
                });
                rest = &candidate[7..];
            } else {
                break;
            }
        }
        let remainder = format!(
            "{}{}",
            before.trim_end_matches(' '),
            rest.trim_start_matches(' ')
        );
        Ok((
            Some(ErrorSection::Master {
                code,
                trace,
                modules,
            }),
            remainder,
        ))
    } else {
        let remainder = format!("{before}{}", &after_er[2..]);
        Ok((Some(ErrorSection::Slave { trace: code }), remainder))
    }
}

/// What a schema field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// `#` markers: a fixed-width decimal integer, optionally signed.
    Decimal,
    /// `*` markers: fixed-width uppercase hexadecimal.
    Hex,
    /// `&` markers: fixed-width raw characters.
    Any,
}

/// One field of a response schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// The two-character field name preceding the value.
    pub name: &'static str,
    pub ty: FieldType,
    /// The value width in characters (excluding any sign).
    pub width: usize,
    /// Whether the field is a space-separated per-channel list.
    pub list: bool,
}

impl FieldSpec {
    pub const fn int(name: &'static str, width: usize) -> FieldSpec {
        FieldSpec {
            name,
            ty: FieldType::Decimal,
            width,
            list: false,
        }
    }
    pub const fn int_list(name: &'static str, width: usize) -> FieldSpec {
        FieldSpec {
            name,
            ty: FieldType::Decimal,
            width,
            list: true,
        }
    }
    pub const fn hex(name: &'static str, width: usize) -> FieldSpec {
        FieldSpec {
            name,
            ty: FieldType::Hex,
            width,
            list: false,
        }
    }
    pub const fn text(name: &'static str, width: usize) -> FieldSpec {
        FieldSpec {
            name,
            ty: FieldType::Any,
            width,
            list: false,
        }
    }
}

/// One parsed field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Int(i64),
    IntList(Vec<i64>),
    Hex(u64),
    Text(String),
}

/// The parsed fields of one response, retrievable by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields {
    values: Vec<(&'static str, FieldValue)>,
}

impl Fields {
    fn get(&self, name: &str) -> Option<&FieldValue> {
        self.values.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    /// The integer value of a scalar decimal field.
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)? {
            FieldValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// The values of a per-channel decimal list field.
    pub fn int_list(&self, name: &str) -> Option<&[i64]> {
        match self.get(name)? {
            FieldValue::IntList(v) => Some(v),
            _ => None,
        }
    }

    /// The value of a hexadecimal field.
    pub fn hex(&self, name: &str) -> Option<u64> {
        match self.get(name)? {
            FieldValue::Hex(v) => Some(*v),
            _ => None,
        }
    }

    /// The raw characters of an any-character field.
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            FieldValue::Text(v) => Some(v),
            _ => None,
        }
    }
}

fn parse_decimal_block(text: &str, width: usize) -> Option<(i64, usize)> {
    let bytes = text.as_bytes();
    let (sign, start): (i64, usize) = match bytes.first() {
        Some(b'+') => (1, 1),
        Some(b'-') => (-1, 1),
        _ => (1, 0),
    };
    let end = start + width;
    if bytes.len() < end {
        return None;
    }
    let digits = &text[start..end];
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: i64 = digits.parse().ok()?;
    Some((sign * value, end))
}

/// Parses a payload (with the error section already removed) against a
/// schema. Fields must appear in schema order; spaces between fields are
/// tolerated.
pub fn parse_fields(payload: &str, specs: &[FieldSpec]) -> Result<Fields, ResponseParseError> {
    let mut values = Vec::with_capacity(specs.len());
    let mut cursor = 0usize;
    for spec in specs {
        let rest = &payload[cursor..];
        let found = rest
            .find(spec.name)
            .ok_or(ResponseParseError::MissingField {
                payload: payload.to_string(),
                field: spec.name,
                offset: cursor,
            })?;
        let mut at = cursor + found + spec.name.len();
        let value =
            match spec.ty {
                FieldType::Decimal => {
                    let (first, used) = parse_decimal_block(&payload[at..], spec.width)
                        .ok_or_else(|| ResponseParseError::BadFieldValue {
                            field: spec.name,
                            text: payload[at..].chars().take(spec.width + 1).collect(),
                            kind: "decimal integer",
                            width: spec.width,
                        })?;
                    at += used;
                    if spec.list {
                        let mut items = vec![first];
                        while payload[at..].starts_with(' ') {
                            match parse_decimal_block(&payload[at + 1..], spec.width) {
                                Some((item, used)) => {
                                    items.push(item);
                                    at += 1 + used;
                                }
                                None => break,
                            }
                        }
                        FieldValue::IntList(items)
                    } else {
                        FieldValue::Int(first)
                    }
                }
                FieldType::Hex => {
                    let text: String = payload[at..].chars().take(spec.width).collect();
                    if text.len() < spec.width || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(ResponseParseError::BadFieldValue {
                            field: spec.name,
                            text,
                            kind: "hexadecimal value",
                            width: spec.width,
                        });
                    }
                    at += spec.width;
                    FieldValue::Hex(
                        u64::from_str_radix(&text, 16).expect("checked hex digits parse as hex"),
                    )
                }
                FieldType::Any => {
                    let text: String = payload[at..].chars().take(spec.width).collect();
                    at += text.len();
                    FieldValue::Text(text)
                }
            };
        values.push((spec.name, value));
        cursor = at;
    }
    Ok(Fields { values })
}

/// Parses a payload that is bare space-separated integers (`RU` travel
/// ranges, `UA` working envelopes).
pub fn parse_bare_ints(payload: &str) -> Vec<i64> {
    payload
        .split_whitespace()
        .filter_map(|w| w.parse().ok())
        .collect()
}

/// Parses barcode data of the form `bb/<len:2><data>/<len:2><data>...`.
/// A length of `00` means no barcode was read at that position.
pub fn parse_barcodes(payload: &str) -> Vec<Option<String>> {
    let Some(at) = payload.find("bb") else {
        return Vec::new();
    };
    payload[at + 2..]
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let len: usize = segment.get(..2).and_then(|d| d.parse().ok()).unwrap_or(0);
            if len == 0 {
                None
            } else {
                Some(segment[2..].chars().take(len).collect())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_envelope_splits_into_module_code_and_id() {
        let reply = RawResponse::parse("P1RFid0002rf1.0S 2009-06-24 A")
            .expect("a well-formed reply parses");
        assert_eq!(
            reply.module, "P1",
            "the first two characters are the module"
        );
        assert_eq!(reply.code, "RF", "the next two are the command code");
        assert_eq!(
            reply.id.map(CommandId::value),
            Some(2),
            "the id echoes the command"
        );
        assert_eq!(
            reply.payload, "rf1.0S 2009-06-24 A",
            "the payload keeps everything after the id"
        );
    }

    #[test]
    fn a_success_error_section_is_code_zero_trace_zero() {
        let (section, rest) =
            split_error_section("er00/00rt1 1").expect("a success section parses");
        assert_eq!(
            section,
            Some(ErrorSection::Master {
                code: 0,
                trace: 0,
                modules: Vec::new()
            }),
            "er00/00 is the success marker"
        );
        assert_eq!(rest, "rt1 1", "the parameter data survives section removal");
    }

    #[test]
    fn a_slave_error_section_lists_the_per_module_entries() {
        let (section, _) = split_error_section(" er99/00 P100/00 P235/00 P402/98 PG08/76")
            .expect("a multi-module section parses");
        let ErrorSection::Master {
            code,
            trace,
            modules,
        } = section.expect("the section is present")
        else {
            panic!("a C0 reply carries a master-style section");
        };
        assert_eq!(
            code, 99,
            "master code 99 delegates to the per-module entries"
        );
        assert_eq!(trace, 0, "the master trace is 00");
        assert_eq!(modules.len(), 4, "all four module entries are captured");
        assert_eq!(
            modules[3],
            ModuleEntry {
                address: "PG".to_string(),
                code: 8,
                trace: 76
            },
            "channel 16 reports code 08 trace 76"
        );
    }

    #[test]
    fn a_slave_direct_reply_carries_a_bare_trace() {
        let (section, rest) = split_error_section("er36").expect("a bare trace parses");
        assert_eq!(
            section,
            Some(ErrorSection::Slave { trace: 36 }),
            "slave replies carry only a trace"
        );
        assert_eq!(rest, "", "nothing else remains");
    }

    #[test]
    fn per_channel_lists_parse_to_one_value_per_channel() {
        let fields = parse_fields("rt1 0 1 1 0 0 0 0", &[FieldSpec::int_list("rt", 1)])
            .expect("a tip-presence reply parses");
        assert_eq!(
            fields.int_list("rt"),
            Some(&[1, 0, 1, 1, 0, 0, 0, 0][..]),
            "the list length is the machine's channel count"
        );
    }

    #[test]
    fn signed_positions_parse_with_their_sign() {
        let fields =
            parse_fields("rz-00042", &[FieldSpec::int("rz", 5)]).expect("a signed value parses");
        assert_eq!(
            fields.int("rz"),
            Some(-42),
            "the sign precedes the fixed-width digits"
        );
    }

    #[test]
    fn hex_fields_decode_machine_configuration_bits() {
        let fields = parse_fields(
            "kb1Fkp08",
            &[FieldSpec::hex("kb", 2), FieldSpec::int("kp", 2)],
        )
        .expect("an RM reply parses");
        assert_eq!(fields.hex("kb"), Some(0x1F), "kb is two hex characters");
        assert_eq!(fields.int("kp"), Some(8), "kp is the channel count");
    }

    #[test]
    fn missing_fields_name_the_field_and_payload() {
        let error = parse_fields("qw1", &[FieldSpec::int("qx", 1)]).expect_err("qx is absent");
        assert_eq!(
            error,
            ResponseParseError::MissingField {
                payload: "qw1".to_string(),
                field: "qx",
                offset: 0
            },
            "the error names what was expected and where"
        );
    }

    #[test]
    fn barcodes_decode_length_prefixed_segments() {
        assert_eq!(
            parse_barcodes("bb/051234A/00/03XYZ"),
            vec![Some("1234A".to_string()), None, Some("XYZ".to_string())],
            "each segment is a two-digit length then the data; 00 means none"
        );
    }
}
