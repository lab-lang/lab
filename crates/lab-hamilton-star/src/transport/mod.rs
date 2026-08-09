//! The transport abstraction: blocking message-oriented reads and writes.
//!
//! A transport carries whole protocol messages — the USB implementation
//! maps message boundaries onto USB transfer boundaries; the mock hands
//! messages over directly. Reads and writes must be callable concurrently
//! from different threads: a long blocking read must never starve the
//! write whose response it awaits.

use std::time::Duration;

pub mod mock;
pub mod usb;

pub use mock::MockTransport;
#[cfg(feature = "usb")]
pub use usb::UsbTransport;
pub use usb::{BulkEndpoints, UsbDiscipline};

/// The error raised by a transport.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error(
        "no Hamilton STAR was found on USB (vendor 0x08AF, product 0x8000); check the cable and power"
    )]
    DeviceNotFound,
    #[error("the USB device has no bulk {direction} endpoint on interface 0")]
    MissingEndpoint { direction: &'static str },
    #[error("the transport is disconnected; reconnect before sending commands")]
    Disconnected,
    #[error("USB operation failed: {message}")]
    Usb { message: String },
    #[error(
        "the read timed out mid-message after {received} bytes; the device stopped mid-transfer"
    )]
    TruncatedMessage { received: usize },
}

/// A blocking, message-oriented transport.
pub trait Transport: Send + Sync {
    /// Writes one complete command message.
    fn write_message(&self, data: &[u8]) -> Result<(), TransportError>;

    /// Reads one complete response message, waiting up to `timeout`.
    /// `Ok(None)` means nothing arrived — not an error; the caller retries
    /// until its own deadline.
    fn read_message(&self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError>;

    /// Discards buffered messages: stale responses from a previous session
    /// must not be correlated with new commands. Returns how many messages
    /// were dropped.
    fn drain(&self) -> Result<usize, TransportError> {
        let mut dropped = 0;
        while self.read_message(Duration::from_millis(200))?.is_some() {
            dropped += 1;
        }
        Ok(dropped)
    }
}
