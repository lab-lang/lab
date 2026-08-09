//! The transport abstraction: a blocking SOAP request/response channel
//! plus a stream of device-initiated events.
//!
//! The ODTC's protocol is asymmetric: commands travel client-to-device
//! over plain HTTP POSTs answered synchronously, while completions,
//! state transitions, and telemetry travel device-to-client as HTTP
//! POSTs to a listener the client hosts. A transport therefore carries
//! both directions: [`SoapTransport::send`] for commands and
//! [`SoapTransport::receive_event`] for whatever the device initiated.

use std::time::Duration;

use crate::soap::IncomingEvent;

pub mod http;
pub mod mock;

pub use http::HttpSoapTransport;
pub use mock::{MockReply, MockSoapTransport};

/// The error raised by a transport.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("the HTTP request to the device failed: {detail}")]
    Http { detail: String },
    #[error("the callback listener failed: {detail}")]
    Listener { detail: String },
    #[error(
        "no local route toward {device} was found ({detail}); the callback URI must name the interface that faces the device"
    )]
    NoLocalRoute { device: String, detail: String },
    #[error("the mock transport has no scripted response for {command}")]
    Unscripted { command: String },
}

/// A blocking SOAP channel to one device.
pub trait SoapTransport: Send + Sync {
    /// POSTs one request envelope under the given `SOAPAction` and
    /// returns the synchronous response body.
    fn send(&self, soap_action: &str, envelope: &str) -> Result<String, TransportError>;

    /// The URI the device should POST its events to. `Reset` registers
    /// it on the device.
    fn event_receiver_uri(&self) -> String;

    /// The next device-initiated event, waiting up to `timeout`.
    /// `Ok(None)` means nothing arrived — not an error; the caller falls
    /// back to polling or retries.
    fn receive_event(&self, timeout: Duration) -> Result<Option<IncomingEvent>, TransportError>;
}
