//! A typed driver for the Inheco ODTC (On-Deck Thermal Cycler), speaking
//! its SiLA 1.x SOAP protocol over plain HTTP.
//!
//! The crate is three cleanly separated layers:
//!
//! - **Protocol** ([`soap`], [`methodset`]) — pure, no I/O: SOAP 1.1
//!   envelope encoding and decoding for the command vocabulary, the
//!   response and return-code model, device-event parsing with the
//!   canned acknowledgements, and a validated MethodSet builder
//!   rendering the vendor's thermal-profile XML dialect. Timestamps and
//!   method names are caller inputs, so every document renders to the
//!   same bytes and is testable as a string with no hardware.
//! - **Transport** ([`transport`]) — a blocking
//!   [`transport::SoapTransport`] trait carrying both directions of the
//!   asymmetric protocol: `ureq` for the client-to-device POSTs and a
//!   hand-rolled HTTP/1.1 listener thread for the device-to-client event
//!   POSTs, plus a scripted mock for tests.
//! - **Session** ([`session`]) — an [`Odtc`] handle owning the
//!   transport: the connect handshake, door control, method upload and
//!   execution, temperature readout, and completion resolved by the
//!   device's `ResponseEvent` callback with a `GetStatus` polling
//!   fallback so a firewall never wedges a run. It implements
//!   [`lab_instruments::Thermocycler`]; `progress()` stays `None`
//!   because the ODTC cannot report where a running method stands.
//!
//! The protocol knowledge here derives from PyLabRobot's ODTC backend
//! and SiLA interface (MIT licensed) and from the Inheco ODTC user
//! manual (document 900584); the device's own `odtc.wsdl` is the
//! authority on the full return-code table. No vendor library is
//! involved: control is HTTP and XML end to end.

pub mod methodset;
pub mod session;
pub mod soap;
pub mod transport;

pub use methodset::{
    BLOCK_MAX_CELSIUS, BLOCK_MIN_CELSIUS, LID_MAX_CELSIUS, LID_MIN_CELSIUS, MAX_SLOPE_C_PER_S,
    MethodSetError, MethodSettings, ProgramStage, ProgramStep, ThermalProgram,
};
pub use session::{
    ActualTemperatures, DeviceIdentification, MethodRun, Odtc, OdtcError, OdtcOptions, SensorValue,
};
pub use soap::{
    Command, DataEvent, DataSeries, DeviceState, IncomingEvent, ResponseEvent, SoapError,
    StatusEvent, SyncResponse,
};
pub use transport::{HttpSoapTransport, MockSoapTransport, SoapTransport, TransportError};
