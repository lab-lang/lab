//! A typed driver for the Byonoy Absorbance 96 plate reader, speaking its
//! raw USB HID protocol.
//!
//! The crate is three cleanly separated layers:
//!
//! - **Report codec** ([`report`]) — pure, no I/O: typed encode and decode
//!   of the 64-byte HID report protocol. Every report's wire form is
//!   testable as bytes with no hardware. The Luminescence 96's reports and
//!   the LED bar reports are covered at this layer only.
//! - **Transport** ([`transport`]) — a blocking
//!   [`transport::HidTransport`] trait over 64-byte packets, with a
//!   `hidapi`-backed implementation behind the `hid` cargo feature and a
//!   scripted mock for tests.
//! - **Session** ([`session`]) — an [`Absorbance96`] handle owning the
//!   transport: discovery and open-by-path, the mandatory reference
//!   measurement and wavelength query on setup, the chunk-reassembling
//!   measurement engine, the post-measurement status gate, and abort. It
//!   implements [`lab_instruments::PlateReader`].
//!
//! There is no vendor library anywhere in the stack: the wire protocol is
//! implemented directly over the OS HID layer. The protocol knowledge
//! derives from [PyLabRobot](https://github.com/PyLabRobot/pylabrobot)'s
//! Byonoy implementation (MIT licensed), the de-facto public specification
//! of this otherwise undocumented protocol. PyLabRobot is reference
//! material only, not a dependency.

pub mod report;
pub mod session;
pub mod transport;

pub use report::{
    Abs96FirmwareError, AbsorbanceChunk, AbsorbanceTrigger, DeviceDataReply, DeviceDataValue,
    Environment, LedBarEffect, LedEffect, LuminescenceChunk, LuminescenceTrigger, Packet,
    ReportDecodeError, Rgb, RoutingTag, SlotState, Status, SupportedReportsChunk, Versions,
    WellMask,
};
pub use session::{AbortHandle, Absorbance96, Absorbance96Error, AbsorbanceMeasurement, Timeouts};
#[cfg(feature = "hid")]
pub use transport::HidapiTransport;
pub use transport::{
    ABSORBANCE_96_PRODUCT_ID, BYONOY_VENDOR_ID, DiscoveredDevice, HidError, HidTransport,
    LUMINESCENCE_96_PRODUCT_ID, MockHidTransport,
};
