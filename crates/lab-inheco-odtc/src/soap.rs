//! The SOAP 1.1 protocol layer: request envelopes for the ODTC's SiLA
//! command vocabulary, synchronous response parsing, device-initiated
//! event parsing, and the canned event acknowledgements. Everything here
//! is string in, string out — nothing touches the network.
//!
//! The wire dialect is SOAP 1.1 doc/literal with the command element in
//! the default namespace `http://sila.coop`. Every command carries a
//! client-generated `requestId` (a 31-bit positive integer) that the
//! device echoes in the `ResponseEvent` completing an asynchronous
//! command.

use thiserror::Error;

/// The SiLA 1.x namespace every command and event lives in.
pub const SILA_NAMESPACE: &str = "http://sila.coop";

/// The synchronous return code meaning the command already completed.
/// Only `GetStatus` and `GetDeviceIdentification` answer this way.
pub const RETURN_CODE_SUCCESS: i32 = 1;

/// The synchronous return code meaning the command was accepted and will
/// complete later through a `ResponseEvent`.
pub const RETURN_CODE_ACCEPTED: i32 = 2;

/// The `ResponseEvent` return code meaning the asynchronous command
/// succeeded.
pub const RETURN_CODE_ASYNC_SUCCESS: i32 = 3;

/// The `ResponseEvent` return code meaning the asynchronous command
/// succeeded with a warning worth surfacing.
pub const RETURN_CODE_ASYNC_WARNING: i32 = 12;

/// One command in the ODTC's SiLA vocabulary, ready to render as a
/// request envelope.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Registers the client's event-receiver URI on the device. Every
    /// asynchronous completion is POSTed there from then on.
    Reset {
        device_id: String,
        event_receiver_uri: String,
        simulation_mode: bool,
    },
    /// Moves the device from standby to idle.
    Initialize,
    /// Reports the device state. Answers synchronously.
    GetStatus,
    /// Reports device name, serial number, and firmware version. Answers
    /// synchronously.
    GetDeviceIdentification,
    /// Opens the motorized door.
    OpenDoor,
    /// Closes the motorized door.
    CloseDoor,
    /// Uploads or replaces method definitions. The payload is a
    /// `ParameterSet` document carrying an escaped MethodSet.
    SetParameters { params_xml: String },
    /// Runs an uploaded Method or PreMethod. The `ResponseEvent` arrives
    /// at method completion — hours later for a long profile.
    ExecuteMethod { method_name: String },
    /// Aborts the running method.
    StopMethod,
    /// Reads every temperature sensor; the values arrive in the
    /// `ResponseEvent`'s `responseData`.
    ReadActualTemperature,
}

impl Command {
    /// The command's element name, which is also the tail of its
    /// `SOAPAction` header.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Reset { .. } => "Reset",
            Command::Initialize => "Initialize",
            Command::GetStatus => "GetStatus",
            Command::GetDeviceIdentification => "GetDeviceIdentification",
            Command::OpenDoor => "OpenDoor",
            Command::CloseDoor => "CloseDoor",
            Command::SetParameters { .. } => "SetParameters",
            Command::ExecuteMethod { .. } => "ExecuteMethod",
            Command::StopMethod => "StopMethod",
            Command::ReadActualTemperature => "ReadActualTemperature",
        }
    }

    /// The `SOAPAction` header value for this command.
    pub fn soap_action(&self) -> String {
        format!("{SILA_NAMESPACE}/{}", self.name())
    }

    /// Whether the device answers this command synchronously (return
    /// code 1) rather than accepting it for asynchronous completion
    /// (return code 2).
    pub fn is_synchronous(&self) -> bool {
        matches!(self, Command::GetStatus | Command::GetDeviceIdentification)
    }

    /// Renders the complete SOAP 1.1 request envelope carrying this
    /// command under the given request id.
    pub fn envelope(&self, request_id: u32) -> String {
        let mut params = String::new();
        match self {
            Command::Reset {
                device_id,
                event_receiver_uri,
                simulation_mode,
            } => {
                push_element(&mut params, "deviceId", device_id);
                push_element(&mut params, "eventReceiverURI", event_receiver_uri);
                push_element(
                    &mut params,
                    "simulationMode",
                    if *simulation_mode { "true" } else { "false" },
                );
            }
            Command::SetParameters { params_xml } => {
                push_element(&mut params, "paramsXML", params_xml);
            }
            Command::ExecuteMethod { method_name } => {
                push_element(&mut params, "methodName", method_name);
            }
            _ => {}
        }
        let name = self.name();
        format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <{name} xmlns=\"{SILA_NAMESPACE}\"><requestId>{request_id}</requestId>{params}</{name}>\
             </s:Body></s:Envelope>"
        )
    }
}

fn push_element(out: &mut String, name: &str, text: &str) {
    out.push('<');
    out.push_str(name);
    out.push('>');
    out.push_str(&quick_xml::escape::partial_escape(text));
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// The error raised when a SOAP document cannot be understood.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SoapError {
    #[error("the document is not well-formed XML: {detail}")]
    Malformed { detail: String },
    #[error("the SOAP body carries no payload element")]
    EmptyBody,
    #[error("the document carries no <{element}> element")]
    MissingElement { element: String },
    #[error("<{element}> holds {value:?}, which does not parse as {expected}")]
    InvalidValue {
        element: String,
        value: String,
        expected: &'static str,
    },
    #[error("the device answered a SOAP fault: {fault}")]
    Fault { fault: String },
    #[error("the payload <{name}> is not a device event this driver knows")]
    UnknownEvent { name: String },
}

/// A parsed document: the first element inside the SOAP body (when the
/// document is an envelope) and every leaf element's text, in document
/// order, keyed by local name.
struct Parsed {
    payload_name: Option<String>,
    leaves: Vec<(String, String)>,
}

impl Parsed {
    fn first(&self, name: &str) -> Option<&str> {
        self.leaves
            .iter()
            .find(|(leaf, _)| leaf == name)
            .map(|(_, text)| text.as_str())
    }
}

fn malformed(detail: impl std::fmt::Display) -> SoapError {
    SoapError::Malformed {
        detail: detail.to_string(),
    }
}

/// Walks the document once, recording the payload element (the first
/// child of a `Body` element) and the trimmed text of every leaf.
/// Namespace prefixes are ignored: the device's documents vary between
/// prefixed and default-namespace forms, and local names are unambiguous
/// within this protocol.
fn parse_document(xml: &str) -> Result<Parsed, SoapError> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    // Each entry is (local name, accumulated text, has element children).
    let mut stack: Vec<(String, String, bool)> = Vec::new();
    let mut payload_name = None;
    let mut leaves = Vec::new();
    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Eof => break,
            Event::Start(start) => {
                let local = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
                note_child(&mut stack, &mut payload_name, &local);
                stack.push((local, String::new(), false));
            }
            Event::Empty(start) => {
                let local = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
                note_child(&mut stack, &mut payload_name, &local);
                leaves.push((local, String::new()));
            }
            Event::End(_) => {
                if let Some((name, text, has_children)) = stack.pop()
                    && !has_children
                {
                    leaves.push((name, text.trim().to_string()));
                }
            }
            Event::Text(text) => {
                let decoded = text.xml10_content().map_err(malformed)?;
                if let Some((_, buffer, _)) = stack.last_mut() {
                    buffer.push_str(&decoded);
                }
            }
            Event::GeneralRef(reference) => {
                if let Some((_, buffer, _)) = stack.last_mut() {
                    buffer.push_str(&resolve_reference(&reference)?);
                }
            }
            Event::CData(cdata) => {
                if let Some((_, buffer, _)) = stack.last_mut() {
                    buffer.push_str(&String::from_utf8_lossy(&cdata));
                }
            }
            _ => {}
        }
    }
    Ok(Parsed {
        payload_name,
        leaves,
    })
}

/// Resolves one entity reference — predefined (`&lt;` and friends) or a
/// character reference — to the text it stands for.
fn resolve_reference(reference: &quick_xml::events::BytesRef<'_>) -> Result<String, SoapError> {
    if let Some(character) = reference.resolve_char_ref().map_err(malformed)? {
        return Ok(character.to_string());
    }
    let name = reference.xml10_content().map_err(malformed)?;
    quick_xml::escape::resolve_predefined_entity(&name)
        .map(str::to_string)
        .ok_or_else(|| malformed(format!("unknown entity &{name};")))
}

fn note_child(
    stack: &mut [(String, String, bool)],
    payload_name: &mut Option<String>,
    local: &str,
) {
    if let Some((parent, _, has_children)) = stack.last_mut() {
        *has_children = true;
        if payload_name.is_none() && parent == "Body" {
            *payload_name = Some(local.to_string());
        }
    }
}

fn require<'a>(parsed: &'a Parsed, element: &str) -> Result<&'a str, SoapError> {
    parsed
        .first(element)
        .ok_or_else(|| SoapError::MissingElement {
            element: element.to_string(),
        })
}

fn parse_number<T: std::str::FromStr>(
    element: &str,
    value: &str,
    expected: &'static str,
) -> Result<T, SoapError> {
    value.parse().map_err(|_| SoapError::InvalidValue {
        element: element.to_string(),
        value: value.to_string(),
        expected,
    })
}

/// The synchronous HTTP response to a command: the `<Command>Result`
/// block plus any sibling fields the command carries (`state` for
/// `GetStatus`, the identification triple for
/// `GetDeviceIdentification`).
#[derive(Clone, Debug, PartialEq)]
pub struct SyncResponse {
    /// The payload element's local name, e.g. `ResetResponse`.
    pub command: String,
    pub return_code: i32,
    pub message: String,
    pub duration: Option<String>,
    pub device_class: Option<String>,
    fields: Vec<(String, String)>,
}

impl SyncResponse {
    /// Parses a synchronous response envelope. A SOAP fault becomes
    /// [`SoapError::Fault`] carrying the fault string.
    pub fn parse(xml: &str) -> Result<SyncResponse, SoapError> {
        let parsed = parse_document(xml)?;
        let command = parsed.payload_name.clone().ok_or(SoapError::EmptyBody)?;
        if command == "Fault" {
            return Err(SoapError::Fault {
                fault: parsed
                    .first("faultstring")
                    .unwrap_or("(no fault string)")
                    .to_string(),
            });
        }
        let code_text = require(&parsed, "returnCode")?;
        let return_code = parse_number("returnCode", code_text, "an integer")?;
        Ok(SyncResponse {
            command,
            return_code,
            message: parsed.first("message").unwrap_or_default().to_string(),
            duration: parsed.first("duration").map(str::to_string),
            device_class: parsed.first("deviceClass").map(str::to_string),
            fields: parsed.leaves,
        })
    }

    /// The first leaf element with the given local name, anywhere in the
    /// response payload.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(leaf, _)| leaf == name)
            .map(|(_, text)| text.as_str())
    }

    /// The `state` field a `GetStatus` response carries.
    pub fn state(&self) -> Option<&str> {
        self.field("state")
    }
}

/// The requestId a request envelope carries, for transports and tests
/// that correlate replies with commands.
pub fn request_id_of(envelope: &str) -> Option<u32> {
    let parsed = parse_document(envelope).ok()?;
    parsed.first("requestId")?.parse().ok()
}

/// A device state as `GetStatus` and `StatusEvent` report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceState {
    Startup,
    Resetting,
    Standby,
    Initializing,
    Idle,
    Busy,
    Paused,
    ErrorHandling,
    InError,
    /// A state string outside the documented vocabulary, kept verbatim.
    Unknown(String),
}

impl DeviceState {
    pub fn from_wire(state: &str) -> DeviceState {
        match state {
            "startup" => DeviceState::Startup,
            "resetting" => DeviceState::Resetting,
            "standby" => DeviceState::Standby,
            "initializing" => DeviceState::Initializing,
            "idle" => DeviceState::Idle,
            "busy" => DeviceState::Busy,
            "paused" => DeviceState::Paused,
            "errorHandling" => DeviceState::ErrorHandling,
            "inError" => DeviceState::InError,
            other => DeviceState::Unknown(other.to_string()),
        }
    }

    pub fn as_wire(&self) -> &str {
        match self {
            DeviceState::Startup => "startup",
            DeviceState::Resetting => "resetting",
            DeviceState::Standby => "standby",
            DeviceState::Initializing => "initializing",
            DeviceState::Idle => "idle",
            DeviceState::Busy => "busy",
            DeviceState::Paused => "paused",
            DeviceState::ErrorHandling => "errorHandling",
            DeviceState::InError => "inError",
            DeviceState::Unknown(other) => other,
        }
    }

    /// Whether the device is at rest and ready for a new method: idle or
    /// standby. This is the polling-fallback completion condition.
    pub fn is_settled(&self) -> bool {
        matches!(self, DeviceState::Idle | DeviceState::Standby)
    }

    /// Whether the device is in an error state that fails any wait.
    pub fn is_error(&self) -> bool {
        matches!(self, DeviceState::ErrorHandling | DeviceState::InError)
    }
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_wire())
    }
}

/// The event completing an asynchronous command, POSTed by the device to
/// the registered event-receiver URI.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseEvent {
    /// The requestId of the command this event completes.
    pub request_id: u32,
    /// Code 3 is success; 12 is success with a warning; anything else is
    /// a failure described by `message`.
    pub return_code: i32,
    pub message: String,
    /// An embedded XML document with command-specific payload, already
    /// unescaped from its transport encoding. `None` when the device sent
    /// none.
    pub response_data: Option<String>,
}

/// A state-transition notification POSTed by the device.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusEvent {
    /// The state the device moved to, when the event names one.
    pub state: Option<String>,
}

impl StatusEvent {
    pub fn device_state(&self) -> Option<DeviceState> {
        self.state.as_deref().map(DeviceState::from_wire)
    }
}

/// A telemetry event POSTed by the device during a run, carrying live
/// temperature series.
#[derive(Clone, Debug, PartialEq)]
pub struct DataEvent {
    /// The embedded XML document holding the data series, already
    /// unescaped from its transport encoding.
    pub data_value: Option<String>,
}

/// One named series from a [`DataEvent`]. Temperature series carry
/// centi-degrees (3700 is 37.00 °C); the unit string comes verbatim from
/// the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSeries {
    pub name: String,
    pub unit: Option<String>,
    pub values: Vec<i64>,
}

impl DataSeries {
    /// The newest value in the series.
    pub fn latest(&self) -> Option<i64> {
        self.values.last().copied()
    }
}

impl DataEvent {
    /// Extracts the named integer series the event carries: the
    /// `dataValue` document embeds an `AnyData` string, itself an XML
    /// document of `dataSeries` elements.
    pub fn series(&self) -> Result<Vec<DataSeries>, SoapError> {
        let Some(data_value) = &self.data_value else {
            return Ok(Vec::new());
        };
        let outer = parse_document(data_value)?;
        let embedded = require(&outer, "AnyData")?;
        parse_series_document(embedded)
    }
}

fn parse_series_document(xml: &str) -> Result<Vec<DataSeries>, SoapError> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut series: Vec<DataSeries> = Vec::new();
    let mut in_integer_value = false;
    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Eof => break,
            Event::Start(start) | Event::Empty(start) => {
                let local = start.local_name().as_ref().to_vec();
                if local == b"dataSeries" {
                    let mut name = String::new();
                    let mut unit = None;
                    for attribute in start.attributes() {
                        let attribute = attribute.map_err(malformed)?;
                        let value = attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(malformed)?
                            .into_owned();
                        match attribute.key.local_name().as_ref() {
                            b"nameId" => name = value,
                            b"unit" => unit = Some(value),
                            _ => {}
                        }
                    }
                    series.push(DataSeries {
                        name,
                        unit,
                        values: Vec::new(),
                    });
                } else if local == b"integerValue" {
                    in_integer_value = true;
                }
            }
            Event::Text(text) if in_integer_value => {
                let decoded = text.xml10_content().map_err(malformed)?;
                let value = parse_number("integerValue", decoded.trim(), "an integer")?;
                if let Some(current) = series.last_mut() {
                    current.values.push(value);
                }
            }
            Event::End(end) if end.local_name().as_ref() == b"integerValue" => {
                in_integer_value = false;
            }
            _ => {}
        }
    }
    Ok(series)
}

/// A device-initiated event, as POSTed to the event-receiver URI.
#[derive(Clone, Debug, PartialEq)]
pub enum IncomingEvent {
    Response(ResponseEvent),
    Status(StatusEvent),
    Data(DataEvent),
}

impl IncomingEvent {
    /// Parses one event envelope from the device.
    pub fn parse(xml: &str) -> Result<IncomingEvent, SoapError> {
        let parsed = parse_document(xml)?;
        let payload = parsed.payload_name.clone().ok_or(SoapError::EmptyBody)?;
        match payload.as_str() {
            "ResponseEvent" => {
                let request_id = parse_number(
                    "requestId",
                    require(&parsed, "requestId")?,
                    "a 31-bit integer",
                )?;
                let return_code =
                    parse_number("returnCode", require(&parsed, "returnCode")?, "an integer")?;
                let response_data = parsed
                    .first("responseData")
                    .filter(|data| !data.is_empty())
                    .map(str::to_string);
                Ok(IncomingEvent::Response(ResponseEvent {
                    request_id,
                    return_code,
                    message: parsed.first("message").unwrap_or_default().to_string(),
                    response_data,
                }))
            }
            "StatusEvent" => Ok(IncomingEvent::Status(StatusEvent {
                state: parsed.first("state").map(str::to_string),
            })),
            "DataEvent" => Ok(IncomingEvent::Data(DataEvent {
                data_value: parsed
                    .first("dataValue")
                    .filter(|data| !data.is_empty())
                    .map(str::to_string),
            })),
            _ => Err(SoapError::UnknownEvent { name: payload }),
        }
    }

    /// The canned success reply the listener must answer this event with,
    /// promptly, so the device never stalls waiting on its client.
    pub fn ack(&self) -> &'static str {
        match self {
            IncomingEvent::Response(_) => RESPONSE_EVENT_ACK,
            IncomingEvent::Status(_) => STATUS_EVENT_ACK,
            IncomingEvent::Data(_) => DATA_EVENT_ACK,
        }
    }
}

macro_rules! event_ack {
    ($event:literal) => {
        concat!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><",
            $event,
            "Response xmlns=\"http://sila.coop\"><",
            $event,
            "Result><returnCode>1</returnCode><message>Success</message>\
             <duration>PT0S</duration><deviceClass>0</deviceClass></",
            $event,
            "Result></",
            $event,
            "Response></s:Body></s:Envelope>"
        )
    };
}

/// The canned success reply to a `ResponseEvent`.
pub const RESPONSE_EVENT_ACK: &str = event_ack!("ResponseEvent");
/// The canned success reply to a `StatusEvent`.
pub const STATUS_EVENT_ACK: &str = event_ack!("StatusEvent");
/// The canned success reply to a `DataEvent`.
pub const DATA_EVENT_ACK: &str = event_ack!("DataEvent");

/// Parses the `responseData` of a completed `ReadActualTemperature`: an
/// XML document whose `String` element embeds a second document of
/// sensor readings in centi-degrees. Returns `(sensor name, °C)` pairs in
/// document order.
pub fn parse_temperature_data(response_data: &str) -> Result<Vec<(String, f64)>, SoapError> {
    let outer = parse_document(response_data)?;
    let embedded = require(&outer, "String")?;
    let inner = parse_document(embedded)?;
    let mut readings = Vec::with_capacity(inner.leaves.len());
    for (name, text) in inner.leaves {
        let raw: i64 = parse_number(&name, &text, "an integer in centi-degrees")?;
        readings.push((name, raw as f64 / 100.0));
    }
    Ok(readings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_renders_its_documented_envelope() {
        let cases: Vec<(Command, &str)> = vec![
            (
                Command::Reset {
                    device_id: "lab".to_string(),
                    event_receiver_uri: "http://192.168.1.5:49152/".to_string(),
                    simulation_mode: false,
                },
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <Reset xmlns=\"http://sila.coop\"><requestId>7</requestId><deviceId>lab</deviceId>\
                 <eventReceiverURI>http://192.168.1.5:49152/</eventReceiverURI>\
                 <simulationMode>false</simulationMode></Reset></s:Body></s:Envelope>",
            ),
            (
                Command::Initialize,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <Initialize xmlns=\"http://sila.coop\"><requestId>7</requestId></Initialize>\
                 </s:Body></s:Envelope>",
            ),
            (
                Command::GetStatus,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <GetStatus xmlns=\"http://sila.coop\"><requestId>7</requestId></GetStatus>\
                 </s:Body></s:Envelope>",
            ),
            (
                Command::GetDeviceIdentification,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <GetDeviceIdentification xmlns=\"http://sila.coop\"><requestId>7</requestId>\
                 </GetDeviceIdentification></s:Body></s:Envelope>",
            ),
            (
                Command::OpenDoor,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <OpenDoor xmlns=\"http://sila.coop\"><requestId>7</requestId></OpenDoor>\
                 </s:Body></s:Envelope>",
            ),
            (
                Command::CloseDoor,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <CloseDoor xmlns=\"http://sila.coop\"><requestId>7</requestId></CloseDoor>\
                 </s:Body></s:Envelope>",
            ),
            (
                Command::SetParameters {
                    params_xml: "<ParameterSet><Parameter name=\"MethodsXML\"><String>x &amp; y\
                                 </String></Parameter></ParameterSet>"
                        .to_string(),
                },
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <SetParameters xmlns=\"http://sila.coop\"><requestId>7</requestId>\
                 <paramsXML>&lt;ParameterSet&gt;&lt;Parameter name=\"MethodsXML\"&gt;\
                 &lt;String&gt;x &amp;amp; y&lt;/String&gt;&lt;/Parameter&gt;\
                 &lt;/ParameterSet&gt;</paramsXML></SetParameters></s:Body></s:Envelope>",
            ),
            (
                Command::ExecuteMethod {
                    method_name: "lab_profile_001".to_string(),
                },
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <ExecuteMethod xmlns=\"http://sila.coop\"><requestId>7</requestId>\
                 <methodName>lab_profile_001</methodName></ExecuteMethod></s:Body></s:Envelope>",
            ),
            (
                Command::StopMethod,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <StopMethod xmlns=\"http://sila.coop\"><requestId>7</requestId></StopMethod>\
                 </s:Body></s:Envelope>",
            ),
            (
                Command::ReadActualTemperature,
                "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                 <ReadActualTemperature xmlns=\"http://sila.coop\"><requestId>7</requestId>\
                 </ReadActualTemperature></s:Body></s:Envelope>",
            ),
        ];
        for (command, expected) in cases {
            assert_eq!(
                command.envelope(7),
                expected,
                "the {} envelope is pinned byte for byte",
                command.name()
            );
            assert_eq!(
                command.soap_action(),
                format!("http://sila.coop/{}", command.name()),
                "the SOAPAction header names the command in the SiLA namespace"
            );
        }
    }

    #[test]
    fn only_the_two_documented_commands_are_synchronous() {
        assert!(Command::GetStatus.is_synchronous());
        assert!(Command::GetDeviceIdentification.is_synchronous());
        assert!(!Command::Initialize.is_synchronous());
        assert!(!Command::StopMethod.is_synchronous());
    }

    #[test]
    fn a_request_envelope_reports_its_own_request_id() {
        let envelope = Command::Initialize.envelope(1234567);
        assert_eq!(request_id_of(&envelope), Some(1234567));
    }

    #[test]
    fn a_synchronous_result_parses_its_return_fields() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <ResetResponse xmlns=\"http://sila.coop\"><ResetResult>\
                   <returnCode>2</returnCode><message>Accepted</message>\
                   <duration>PT0.0006S</duration><deviceClass>0</deviceClass>\
                   </ResetResult></ResetResponse></s:Body></s:Envelope>";
        let response = SyncResponse::parse(xml).expect("a well-formed result parses");
        assert_eq!(response.command, "ResetResponse");
        assert_eq!(response.return_code, RETURN_CODE_ACCEPTED);
        assert_eq!(response.message, "Accepted");
        assert_eq!(response.duration.as_deref(), Some("PT0.0006S"));
        assert_eq!(response.device_class.as_deref(), Some("0"));
    }

    #[test]
    fn a_get_status_response_carries_the_device_state() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <GetStatusResponse xmlns=\"http://sila.coop\"><GetStatusResult>\
                   <returnCode>1</returnCode><message>Success</message>\
                   <duration>PT0.0004S</duration><deviceClass>0</deviceClass>\
                   </GetStatusResult><deviceId>ODTC</deviceId><state>idle</state>\
                   </GetStatusResponse></s:Body></s:Envelope>";
        let response = SyncResponse::parse(xml).expect("a status response parses");
        assert_eq!(response.return_code, RETURN_CODE_SUCCESS);
        assert_eq!(response.state(), Some("idle"));
        assert_eq!(DeviceState::from_wire("idle"), DeviceState::Idle);
        assert!(DeviceState::Idle.is_settled());
        assert!(DeviceState::InError.is_error());
        assert!(DeviceState::ErrorHandling.is_error());
        assert!(!DeviceState::Busy.is_settled());
    }

    #[test]
    fn a_device_identification_response_carries_its_fields() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <GetDeviceIdentificationResponse xmlns=\"http://sila.coop\">\
                   <GetDeviceIdentificationResult><returnCode>1</returnCode>\
                   <message>Success</message><duration>PT0.0005S</duration>\
                   <deviceClass>0</deviceClass></GetDeviceIdentificationResult>\
                   <DeviceName>ODTC</DeviceName><SerialNumber>12345</SerialNumber>\
                   <FirmwareVersion>2.13.0</FirmwareVersion>\
                   </GetDeviceIdentificationResponse></s:Body></s:Envelope>";
        let response = SyncResponse::parse(xml).expect("an identification response parses");
        assert_eq!(response.field("DeviceName"), Some("ODTC"));
        assert_eq!(response.field("SerialNumber"), Some("12345"));
        assert_eq!(response.field("FirmwareVersion"), Some("2.13.0"));
    }

    #[test]
    fn a_soap_fault_is_a_typed_error() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <s:Fault><faultcode>s:Client</faultcode>\
                   <faultstring>Unknown command</faultstring></s:Fault></s:Body></s:Envelope>";
        let error = SyncResponse::parse(xml).expect_err("a fault is not a result");
        assert_eq!(
            error,
            SoapError::Fault {
                fault: "Unknown command".to_string()
            }
        );
    }

    #[test]
    fn a_result_without_a_return_code_is_a_typed_error() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <ResetResponse xmlns=\"http://sila.coop\"><ResetResult>\
                   <message>hm</message></ResetResult></ResetResponse></s:Body></s:Envelope>";
        let error = SyncResponse::parse(xml).expect_err("a code-less result cannot be accepted");
        assert_eq!(
            error,
            SoapError::MissingElement {
                element: "returnCode".to_string()
            }
        );
    }

    #[test]
    fn a_response_event_parses_id_code_message_and_unescaped_data() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <ResponseEvent xmlns=\"http://sila.coop\"><requestId>1234567</requestId>\
                   <returnValue><returnCode>3</returnCode><message>Success</message>\
                   <duration>PT7.2S</duration><deviceClass>0</deviceClass></returnValue>\
                   <responseData>&lt;ResponseData&gt;&lt;String&gt;ok&lt;/String&gt;\
                   &lt;/ResponseData&gt;</responseData></ResponseEvent></s:Body></s:Envelope>";
        let event = IncomingEvent::parse(xml).expect("a response event parses");
        assert_eq!(
            event,
            IncomingEvent::Response(ResponseEvent {
                request_id: 1234567,
                return_code: RETURN_CODE_ASYNC_SUCCESS,
                message: "Success".to_string(),
                response_data: Some("<ResponseData><String>ok</String></ResponseData>".to_string()),
            }),
            "the transport escaping is undone exactly once"
        );
    }

    #[test]
    fn a_status_event_parses_its_state() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <StatusEvent xmlns=\"http://sila.coop\"><deviceId>ODTC</deviceId>\
                   <state>busy</state></StatusEvent></s:Body></s:Envelope>";
        let event = IncomingEvent::parse(xml).expect("a status event parses");
        let IncomingEvent::Status(status) = &event else {
            panic!("the payload is a StatusEvent");
        };
        assert_eq!(status.device_state(), Some(DeviceState::Busy));
    }

    #[test]
    fn an_unknown_event_payload_is_a_typed_error() {
        let xml = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
                   <SurpriseEvent xmlns=\"http://sila.coop\"/></s:Body></s:Envelope>";
        let error = IncomingEvent::parse(xml).expect_err("a surprise payload is rejected");
        assert_eq!(
            error,
            SoapError::UnknownEvent {
                name: "SurpriseEvent".to_string()
            }
        );
    }

    #[test]
    fn a_data_event_extracts_its_series() {
        let series_xml = "<data><dataSeries nameId=\"Mount\" unit=\"0.01 C\">\
                          <integerValue>3650</integerValue><integerValue>3700</integerValue>\
                          </dataSeries><dataSeries nameId=\"Lid\">\
                          <integerValue>10500</integerValue></dataSeries></data>";
        let any_data = format!(
            "<Data><AnyData>{}</AnyData></Data>",
            quick_xml::escape::partial_escape(series_xml)
        );
        let event = DataEvent {
            data_value: Some(any_data),
        };
        let series = event.series().expect("a well-formed data event parses");
        assert_eq!(
            series,
            vec![
                DataSeries {
                    name: "Mount".to_string(),
                    unit: Some("0.01 C".to_string()),
                    values: vec![3650, 3700],
                },
                DataSeries {
                    name: "Lid".to_string(),
                    unit: None,
                    values: vec![10500],
                },
            ]
        );
        assert_eq!(
            series[0].latest(),
            Some(3700),
            "the last value is the newest"
        );
    }

    #[test]
    fn event_acks_are_byte_exact() {
        assert_eq!(
            RESPONSE_EVENT_ACK,
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <ResponseEventResponse xmlns=\"http://sila.coop\"><ResponseEventResult>\
             <returnCode>1</returnCode><message>Success</message><duration>PT0S</duration>\
             <deviceClass>0</deviceClass></ResponseEventResult></ResponseEventResponse>\
             </s:Body></s:Envelope>"
        );
        assert_eq!(
            STATUS_EVENT_ACK,
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <StatusEventResponse xmlns=\"http://sila.coop\"><StatusEventResult>\
             <returnCode>1</returnCode><message>Success</message><duration>PT0S</duration>\
             <deviceClass>0</deviceClass></StatusEventResult></StatusEventResponse>\
             </s:Body></s:Envelope>"
        );
        assert_eq!(
            DATA_EVENT_ACK,
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <DataEventResponse xmlns=\"http://sila.coop\"><DataEventResult>\
             <returnCode>1</returnCode><message>Success</message><duration>PT0S</duration>\
             <deviceClass>0</deviceClass></DataEventResult></DataEventResponse>\
             </s:Body></s:Envelope>"
        );
        let event = IncomingEvent::Status(StatusEvent { state: None });
        assert_eq!(
            event.ack(),
            STATUS_EVENT_ACK,
            "each event kind picks its own ack"
        );
    }

    #[test]
    fn temperature_data_scales_centi_degrees_to_celsius() {
        let sensors = "<Temperature><Mount>3700</Mount><Mount_Monitor>3695</Mount_Monitor>\
                       <Lid>10500</Lid><Lid_Monitor>10496</Lid_Monitor><Ambient>2210</Ambient>\
                       <PCB>3050</PCB><Heatsink>2900</Heatsink>\
                       <Heatsink_TEC>2895</Heatsink_TEC></Temperature>";
        let response_data = format!(
            "<ResponseData><ParameterSet><Parameter name=\"Temperature\"><String>{}</String>\
             </Parameter></ParameterSet></ResponseData>",
            quick_xml::escape::partial_escape(sensors)
        );
        let readings = parse_temperature_data(&response_data).expect("a temperature report parses");
        assert_eq!(readings.len(), 8, "all eight sensors are reported");
        assert_eq!(readings[0], ("Mount".to_string(), 37.0));
        assert_eq!(readings[2], ("Lid".to_string(), 105.0));
        assert_eq!(readings[4], ("Ambient".to_string(), 22.1));
    }

    #[test]
    fn temperature_data_without_the_embedded_string_is_a_typed_error() {
        let error = parse_temperature_data("<ResponseData></ResponseData>")
            .expect_err("a report without the sensor document is rejected");
        assert_eq!(
            error,
            SoapError::MissingElement {
                element: "String".to_string()
            }
        );
    }
}
