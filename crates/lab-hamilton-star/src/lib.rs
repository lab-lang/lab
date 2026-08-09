//! A typed implementation of the Hamilton STAR/STARlet liquid-handling
//! robot's firmware command protocol and its USB transport.
//!
//! The crate is three cleanly separated layers:
//!
//! - **Protocol** ([`framing`], [`response`], [`errors`], [`commands`],
//!   [`units`], [`catalog`]) — pure, no I/O: typed command construction and
//!   response parsing for the STAR's ASCII firmware protocol, with the
//!   complete firmware error and trace tables. Every command's wire form is
//!   testable as a string with no hardware.
//! - **Transport** ([`transport`]) — a blocking message-oriented
//!   [`transport::Transport`] trait with the exact USB bulk-transfer
//!   discipline over `rusb` and a scripted mock for tests.
//! - **Session** ([`session`]) — a [`Star`] handle owning the transport: a
//!   background reader thread correlating replies by command id, the
//!   per-module locking the firmware requires, per-command read timeouts,
//!   typed firmware-error decoding, a volatile tip-type cache, and the
//!   documented setup choreography. The public API is synchronous.
//!
//! Safety posture: these commands move a heavy, fast, expensive machine.
//! Every motion parameter is either explicit or a named documented default —
//! nothing is guessed silently — and constructors reject out-of-range
//! values with errors naming the parameter, its unit, and the permitted
//! range. Where the firmware's real behavior diverges from its
//! documentation (the 334.7 mm channel Z ceiling, the broken `H0 DL` home,
//! the `kf` read width), the crate encodes the real behavior and says so.
//!
//! The protocol knowledge here derives from PyLabRobot's Hamilton STAR
//! implementation (MIT licensed), the de-facto public specification of
//! this otherwise undocumented protocol; wire strings verified by its test
//! suite pin this crate's encoders byte for byte. PyLabRobot is not a
//! dependency.

pub mod catalog;
pub mod commands;
pub mod errors;
pub mod framing;
pub mod response;
pub mod session;
pub mod transport;
pub mod units;

pub use catalog::{CorrectionCurve, TipType};
pub use commands::Command;
pub use errors::{CommandError, FirmwareError, OutOfRange, Semantic};
pub use framing::{ChannelPattern, ChannelValues, CommandId, Module};
pub use session::{
    InitializeOptions, MachineInfo, RawCommand, RawCommandError, Star, StarError, TipSpot,
};
#[cfg(feature = "usb")]
pub use transport::UsbTransport;
pub use transport::{MockTransport, Transport, TransportError};
pub use units::{Axis, Microliters, Millimeters, TenthMm, TenthUl};
