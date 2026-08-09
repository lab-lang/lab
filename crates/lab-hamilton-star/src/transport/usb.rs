//! The USB transport: the exact bulk-transfer discipline the STAR expects,
//! implemented generically over a [`BulkEndpoints`] pair so the rules are
//! testable without libusb, plus the `rusb`-backed device implementation.
//!
//! The discipline:
//! - Commands are plain ASCII with no terminator and no length prefix —
//!   the USB transfer boundary is the message boundary.
//! - A write whose length is an exact multiple of `wMaxPacketSize` is
//!   followed by a zero-length packet to terminate the transfer.
//! - A read accumulates `wMaxPacketSize`-sized packets; a short packet
//!   (including a zero-length one) ends the message.
//! - A per-packet timeout with nothing read is not an error; the caller
//!   retries until its own deadline.

use std::time::{Duration, Instant};

use crate::transport::{Transport, TransportError};

/// Hamilton's USB vendor id.
pub const HAMILTON_VENDOR_ID: u16 = 0x08AF;
/// The STAR/STARlet product id.
pub const STAR_PRODUCT_ID: u16 = 0x8000;

/// The default write timeout: 30 s.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// The default per-packet read timeout: 3 s.
pub const DEFAULT_PACKET_TIMEOUT: Duration = Duration::from_secs(3);

/// A pair of bulk endpoints. `read_bulk` returns `Ok(None)` on a timeout
/// with nothing received.
pub trait BulkEndpoints: Send + Sync {
    fn write_bulk(&self, data: &[u8], timeout: Duration) -> Result<usize, TransportError>;
    fn read_bulk(
        &self,
        buffer: &mut [u8],
        timeout: Duration,
    ) -> Result<Option<usize>, TransportError>;
    /// The endpoint's `wMaxPacketSize`.
    fn max_packet_size(&self) -> usize;
}

/// The STAR's transfer discipline over any [`BulkEndpoints`].
pub struct UsbDiscipline<E: BulkEndpoints> {
    endpoints: E,
    write_timeout: Duration,
    packet_timeout: Duration,
}

impl<E: BulkEndpoints> UsbDiscipline<E> {
    pub fn new(endpoints: E) -> UsbDiscipline<E> {
        UsbDiscipline {
            endpoints,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            packet_timeout: DEFAULT_PACKET_TIMEOUT,
        }
    }

    pub fn endpoints(&self) -> &E {
        &self.endpoints
    }
}

impl<E: BulkEndpoints> Transport for UsbDiscipline<E> {
    fn write_message(&self, data: &[u8]) -> Result<(), TransportError> {
        let written = self.endpoints.write_bulk(data, self.write_timeout)?;
        if written != data.len() {
            return Err(TransportError::Usb {
                message: format!("short bulk write: {written} of {} bytes", data.len()),
            });
        }
        // A transfer whose length is an exact multiple of wMaxPacketSize
        // does not terminate on its own; the zero-length packet marks the
        // end of the message.
        if !data.is_empty() && data.len().is_multiple_of(self.endpoints.max_packet_size()) {
            self.endpoints.write_bulk(&[], self.write_timeout)?;
        }
        Ok(())
    }

    fn read_message(&self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        let deadline = Instant::now() + timeout;
        let packet_size = self.endpoints.max_packet_size();
        let mut message: Vec<u8> = Vec::new();
        let mut buffer = vec![0u8; packet_size];
        loop {
            match self.endpoints.read_bulk(&mut buffer, self.packet_timeout)? {
                Some(received) => {
                    message.extend_from_slice(&buffer[..received]);
                    if received < packet_size {
                        // A short packet (including zero-length) ends the
                        // message; one accumulated transfer is one response.
                        return Ok(Some(message));
                    }
                }
                None => {
                    if Instant::now() >= deadline {
                        return if message.is_empty() {
                            Ok(None)
                        } else {
                            Err(TransportError::TruncatedMessage {
                                received: message.len(),
                            })
                        };
                    }
                }
            }
        }
    }
}

/// The `rusb`-backed endpoints of a connected STAR.
#[cfg(feature = "usb")]
pub struct RusbEndpoints {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    write_endpoint: u8,
    read_endpoint: u8,
    max_packet_size: usize,
}

#[cfg(feature = "usb")]
impl BulkEndpoints for RusbEndpoints {
    fn write_bulk(&self, data: &[u8], timeout: Duration) -> Result<usize, TransportError> {
        self.handle
            .write_bulk(self.write_endpoint, data, timeout)
            .map_err(|error| match error {
                rusb::Error::NoDevice => TransportError::Disconnected,
                other => TransportError::Usb {
                    message: other.to_string(),
                },
            })
    }

    fn read_bulk(
        &self,
        buffer: &mut [u8],
        timeout: Duration,
    ) -> Result<Option<usize>, TransportError> {
        match self.handle.read_bulk(self.read_endpoint, buffer, timeout) {
            Ok(received) => Ok(Some(received)),
            Err(rusb::Error::Timeout) => Ok(None),
            Err(rusb::Error::NoDevice) => Err(TransportError::Disconnected),
            Err(other) => Err(TransportError::Usb {
                message: other.to_string(),
            }),
        }
    }

    fn max_packet_size(&self) -> usize {
        self.max_packet_size
    }
}

/// The USB transport of a connected STAR.
#[cfg(feature = "usb")]
pub type UsbTransport = UsbDiscipline<RusbEndpoints>;

/// A STAR seen on the USB bus, for disambiguating multiple machines.
/// STARs may lack unique serial numbers, so the bus position is the
/// reliable discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarUsbLocation {
    pub bus: u8,
    pub address: u8,
}

#[cfg(feature = "usb")]
fn usb_error(error: rusb::Error) -> TransportError {
    TransportError::Usb {
        message: error.to_string(),
    }
}

/// Lists every STAR on the bus.
#[cfg(feature = "usb")]
pub fn list_star_devices() -> Result<Vec<StarUsbLocation>, TransportError> {
    let devices = rusb::devices().map_err(usb_error)?;
    let mut found = Vec::new();
    for device in devices.iter() {
        let descriptor = device.device_descriptor().map_err(usb_error)?;
        if descriptor.vendor_id() == HAMILTON_VENDOR_ID
            && descriptor.product_id() == STAR_PRODUCT_ID
        {
            found.push(StarUsbLocation {
                bus: device.bus_number(),
                address: device.address(),
            });
        }
    }
    Ok(found)
}

#[cfg(feature = "usb")]
impl UsbTransport {
    /// Opens the first STAR on the bus. When several are connected, use
    /// [`list_star_devices`] and [`UsbTransport::open_at`] to pick one — the
    /// devices may not carry unique serial numbers.
    pub fn open() -> Result<UsbTransport, TransportError> {
        Self::open_matching(None)
    }

    /// Opens the STAR at a specific bus location.
    pub fn open_at(location: StarUsbLocation) -> Result<UsbTransport, TransportError> {
        Self::open_matching(Some(location))
    }

    fn open_matching(location: Option<StarUsbLocation>) -> Result<UsbTransport, TransportError> {
        let devices = rusb::devices().map_err(usb_error)?;
        for device in devices.iter() {
            let descriptor = device.device_descriptor().map_err(usb_error)?;
            if descriptor.vendor_id() != HAMILTON_VENDOR_ID
                || descriptor.product_id() != STAR_PRODUCT_ID
            {
                continue;
            }
            if let Some(wanted) = location
                && (device.bus_number() != wanted.bus || device.address() != wanted.address)
            {
                continue;
            }

            let config = device.config_descriptor(0).map_err(usb_error)?;
            let handle = device.open().map_err(usb_error)?;
            handle
                .set_active_configuration(config.number())
                .map_err(usb_error)?;
            handle.claim_interface(0).map_err(usb_error)?;
            handle.set_alternate_setting(0, 0).map_err(usb_error)?;

            let mut write_endpoint = None;
            let mut read_endpoint = None;
            let mut max_packet_size = 0usize;
            for interface in config.interfaces() {
                for interface_descriptor in interface.descriptors() {
                    if interface_descriptor.interface_number() != 0
                        || interface_descriptor.setting_number() != 0
                    {
                        continue;
                    }
                    for endpoint in interface_descriptor.endpoint_descriptors() {
                        match endpoint.direction() {
                            rusb::Direction::Out if write_endpoint.is_none() => {
                                write_endpoint = Some(endpoint.address());
                                max_packet_size = usize::from(endpoint.max_packet_size());
                            }
                            rusb::Direction::In if read_endpoint.is_none() => {
                                read_endpoint = Some(endpoint.address());
                            }
                            _ => {}
                        }
                    }
                }
            }

            let write_endpoint =
                write_endpoint.ok_or(TransportError::MissingEndpoint { direction: "OUT" })?;
            let read_endpoint =
                read_endpoint.ok_or(TransportError::MissingEndpoint { direction: "IN" })?;

            let transport = UsbDiscipline::new(RusbEndpoints {
                handle,
                write_endpoint,
                read_endpoint,
                max_packet_size: max_packet_size.max(64),
            });
            // Stale responses from a previous session must be discarded
            // before the first command is correlated.
            transport.drain()?;
            return Ok(transport);
        }
        Err(TransportError::DeviceNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct FakeEndpoints {
        packet_size: usize,
        written: Mutex<Vec<Vec<u8>>>,
        incoming: Mutex<VecDeque<Vec<u8>>>,
    }

    impl FakeEndpoints {
        fn new(packet_size: usize) -> FakeEndpoints {
            FakeEndpoints {
                packet_size,
                written: Mutex::new(Vec::new()),
                incoming: Mutex::new(VecDeque::new()),
            }
        }

        fn queue_packets(&self, packets: &[&[u8]]) {
            let mut incoming = self
                .incoming
                .lock()
                .expect("the fake lock is never poisoned");
            for packet in packets {
                incoming.push_back(packet.to_vec());
            }
        }
    }

    impl BulkEndpoints for FakeEndpoints {
        fn write_bulk(&self, data: &[u8], _timeout: Duration) -> Result<usize, TransportError> {
            self.written
                .lock()
                .expect("the fake lock is never poisoned")
                .push(data.to_vec());
            Ok(data.len())
        }
        fn read_bulk(
            &self,
            buffer: &mut [u8],
            _timeout: Duration,
        ) -> Result<Option<usize>, TransportError> {
            let mut incoming = self
                .incoming
                .lock()
                .expect("the fake lock is never poisoned");
            match incoming.pop_front() {
                Some(packet) => {
                    buffer[..packet.len()].copy_from_slice(&packet);
                    Ok(Some(packet.len()))
                }
                None => Ok(None),
            }
        }
        fn max_packet_size(&self) -> usize {
            self.packet_size
        }
    }

    #[test]
    fn a_write_at_an_exact_packet_multiple_appends_a_zero_length_packet() {
        let transport = UsbDiscipline::new(FakeEndpoints::new(8));
        transport
            .write_message(b"C0RTid01")
            .expect("an 8-byte write succeeds");
        let written = transport
            .endpoints()
            .written
            .lock()
            .expect("the fake lock is never poisoned");
        assert_eq!(
            written.len(),
            2,
            "the data transfer is followed by a terminating packet"
        );
        assert_eq!(
            written[1],
            Vec::<u8>::new(),
            "the terminator is zero-length"
        );
    }

    #[test]
    fn a_write_below_the_packet_size_needs_no_terminator() {
        let transport = UsbDiscipline::new(FakeEndpoints::new(64));
        transport
            .write_message(b"C0RT")
            .expect("a short write succeeds");
        let written = transport
            .endpoints()
            .written
            .lock()
            .expect("the fake lock is never poisoned");
        assert_eq!(written.len(), 1, "a short transfer terminates on its own");
    }

    #[test]
    fn a_message_accumulates_full_packets_until_a_short_one() {
        let transport = UsbDiscipline::new(FakeEndpoints::new(4));
        transport
            .endpoints()
            .queue_packets(&[b"C0RT", b"id00", b"01"]);
        let message = transport
            .read_message(Duration::from_millis(100))
            .expect("the read succeeds")
            .expect("a message is available");
        assert_eq!(
            message, b"C0RTid0001",
            "full packets accumulate; the short packet ends the message"
        );
    }

    #[test]
    fn a_zero_length_packet_ends_an_exact_multiple_message() {
        let transport = UsbDiscipline::new(FakeEndpoints::new(4));
        transport.endpoints().queue_packets(&[b"C0RT", b""]);
        let message = transport
            .read_message(Duration::from_millis(100))
            .expect("the read succeeds")
            .expect("a message is available");
        assert_eq!(
            message, b"C0RT",
            "the zero-length packet closes an exact-multiple transfer"
        );
    }

    #[test]
    fn a_quiet_bus_reads_as_none_not_an_error() {
        let transport = UsbDiscipline::new(FakeEndpoints::new(4));
        let result = transport
            .read_message(Duration::from_millis(0))
            .expect("a timeout is not a transport error");
        assert_eq!(result, None, "nothing arrived within the deadline");
    }

    #[test]
    fn drain_discards_every_stale_message() {
        let transport = UsbDiscipline::new(FakeEndpoints::new(64));
        transport
            .endpoints()
            .queue_packets(&[b"stale-1", b"stale-2"]);
        let dropped = transport.drain().expect("draining succeeds");
        assert_eq!(
            dropped, 2,
            "both stale responses are discarded before the first command"
        );
        assert_eq!(
            transport
                .read_message(Duration::from_millis(0))
                .expect("the bus is now quiet"),
            None,
            "nothing stale remains"
        );
    }
}
