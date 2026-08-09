//! The hidapi-backed transport of a connected reader.
//!
//! The HID discipline:
//! - Writes carry a leading report-id byte of `0x00` before the 64-byte
//!   packet, 65 bytes total, as the OS HID layer requires.
//! - Reads poll in short slices so a concurrent abort write is never
//!   starved by a long read; an empty slice past the caller's deadline is
//!   `Ok(None)`, not an error.
//! - The device is opened by its platform path; opening by the
//!   vendor/product/serial triple would re-race the enumeration.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::report::{PACKET_BYTES, Packet};
use crate::transport::{BYONOY_VENDOR_ID, DiscoveredDevice, HidError, HidTransport, select_device};

/// The polling slice a blocking read holds the device lock for; between
/// slices the lock is free for writers.
const READ_SLICE: Duration = Duration::from_millis(100);

fn io_error(error: hidapi::HidError) -> HidError {
    HidError::Io {
        message: error.to_string(),
    }
}

/// The HID transport of a connected Byonoy reader.
pub struct HidapiTransport {
    device: Mutex<hidapi::HidDevice>,
}

impl HidapiTransport {
    /// Lists every connected reader with the given product id.
    pub fn discover(product_id: u16) -> Result<Vec<DiscoveredDevice>, HidError> {
        let api = hidapi::HidApi::new().map_err(io_error)?;
        Ok(Self::enumerate(&api, product_id))
    }

    fn enumerate(api: &hidapi::HidApi, product_id: u16) -> Vec<DiscoveredDevice> {
        api.device_list()
            .filter(|info| info.vendor_id() == BYONOY_VENDOR_ID && info.product_id() == product_id)
            .map(|info| DiscoveredDevice {
                serial_number: info.serial_number().map(str::to_string),
                path: info.path().to_owned(),
            })
            .collect()
    }

    /// Opens the one reader with the given product id, using `serial` to
    /// disambiguate when several are connected.
    pub fn open(product_id: u16, serial: Option<&str>) -> Result<HidapiTransport, HidError> {
        let api = hidapi::HidApi::new().map_err(io_error)?;
        let devices = Self::enumerate(&api, product_id);
        let chosen = select_device(&devices, serial, BYONOY_VENDOR_ID, product_id)?;
        let device = api
            .open_path(&chosen.path)
            .map_err(|error| HidError::OpenFailed {
                message: error.to_string(),
            })?;
        Ok(HidapiTransport {
            device: Mutex::new(device),
        })
    }
}

impl HidTransport for HidapiTransport {
    fn write_report(&self, packet: &Packet) -> Result<(), HidError> {
        let mut buffer = [0u8; PACKET_BYTES + 1];
        buffer[1..].copy_from_slice(packet);
        self.device
            .lock()
            .expect("the transport lock is never poisoned")
            .write(&buffer)
            .map_err(io_error)?;
        Ok(())
    }

    fn read_report(&self, timeout: Duration) -> Result<Option<Packet>, HidError> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            let slice = READ_SLICE
                .min(deadline.saturating_duration_since(now))
                .as_millis();
            let slice = i32::try_from(slice).expect("a read slice is at most 100 ms");
            let mut buffer = [0u8; PACKET_BYTES];
            let received = self
                .device
                .lock()
                .expect("the transport lock is never poisoned")
                .read_timeout(&mut buffer, slice)
                .map_err(io_error)?;
            if received == 0 {
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                continue;
            }
            if received != PACKET_BYTES {
                return Err(HidError::ShortReport { found: received });
            }
            return Ok(Some(buffer));
        }
    }
}
