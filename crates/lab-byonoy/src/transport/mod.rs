//! The transport abstraction: blocking 64-byte HID report exchange.
//!
//! A transport carries whole [`Packet`]s. The hidapi implementation
//! prepends the report-id byte the OS HID layer requires on writes; the
//! mock hands packets over directly. HID access is exclusive per process
//! on every OS, so a crashed session can leave the device claimed until
//! the USB cable is replugged.

use std::ffi::CString;
use std::time::Duration;

use crate::report::Packet;

#[cfg(feature = "hid")]
pub mod hid;
pub mod mock;

#[cfg(feature = "hid")]
pub use hid::HidapiTransport;
pub use mock::MockHidTransport;

/// Byonoy's USB vendor id.
pub const BYONOY_VENDOR_ID: u16 = 0x16D0;
/// The Absorbance 96 product id.
pub const ABSORBANCE_96_PRODUCT_ID: u16 = 0x1199;
/// The Luminescence 96 product id.
pub const LUMINESCENCE_96_PRODUCT_ID: u16 = 0x119B;

/// The error raised by a transport.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HidError {
    #[error(
        "no reader with USB ids {vendor_id:04X}:{product_id:04X} was found; check the cable and power"
    )]
    DeviceNotFound { vendor_id: u16, product_id: u16 },
    #[error(
        "{count} readers share USB ids {vendor_id:04X}:{product_id:04X}; pass a serial number to choose one"
    )]
    MultipleDevices {
        vendor_id: u16,
        product_id: u16,
        count: usize,
    },
    #[error(
        "no reader with USB ids {vendor_id:04X}:{product_id:04X} carries serial number {serial}"
    )]
    SerialNotFound {
        vendor_id: u16,
        product_id: u16,
        serial: String,
    },
    #[error(
        "{count} readers with USB ids {vendor_id:04X}:{product_id:04X} claim serial number {serial}; the serial cannot disambiguate them"
    )]
    AmbiguousSerial {
        vendor_id: u16,
        product_id: u16,
        serial: String,
        count: usize,
    },
    #[error(
        "opening the HID device failed: {message}; HID access is exclusive per process, so a crashed session can hold the device until the USB cable is replugged"
    )]
    OpenFailed { message: String },
    #[error("the HID operation failed: {message}")]
    Io { message: String },
    #[error("the device sent a {found}-byte report where the protocol is fixed at 64 bytes")]
    ShortReport { found: usize },
}

/// A blocking transport over 64-byte HID reports.
pub trait HidTransport: Send + Sync {
    /// Writes one report.
    fn write_report(&self, packet: &Packet) -> Result<(), HidError>;

    /// Reads one report, waiting up to `timeout`. `Ok(None)` means nothing
    /// arrived — not an error; the caller retries until its own deadline.
    fn read_report(&self, timeout: Duration) -> Result<Option<Packet>, HidError>;
}

/// One HID device seen during enumeration, reduced to what selection and
/// opening need: the serial number to disambiguate and the platform path
/// to open by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub serial_number: Option<String>,
    /// The platform device path; opening by path avoids re-racing the
    /// vendor/product/serial lookup.
    pub path: CString,
}

/// Picks exactly one device from an enumeration, or explains why it
/// cannot: no device, several devices with no serial to disambiguate, or
/// a serial that matches none or several.
pub fn select_device<'d>(
    devices: &'d [DiscoveredDevice],
    serial: Option<&str>,
    vendor_id: u16,
    product_id: u16,
) -> Result<&'d DiscoveredDevice, HidError> {
    if devices.is_empty() {
        return Err(HidError::DeviceNotFound {
            vendor_id,
            product_id,
        });
    }
    match serial {
        None => match devices {
            [only] => Ok(only),
            several => Err(HidError::MultipleDevices {
                vendor_id,
                product_id,
                count: several.len(),
            }),
        },
        Some(wanted) => {
            let matching: Vec<&DiscoveredDevice> = devices
                .iter()
                .filter(|device| device.serial_number.as_deref() == Some(wanted))
                .collect();
            match matching.as_slice() {
                [] => Err(HidError::SerialNotFound {
                    vendor_id,
                    product_id,
                    serial: wanted.to_string(),
                }),
                [only] => Ok(only),
                several => Err(HidError::AmbiguousSerial {
                    vendor_id,
                    product_id,
                    serial: wanted.to_string(),
                    count: several.len(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(serial: Option<&str>, path: &str) -> DiscoveredDevice {
        DiscoveredDevice {
            serial_number: serial.map(str::to_string),
            path: CString::new(path).expect("test paths carry no NUL"),
        }
    }

    #[test]
    fn a_single_device_is_selected_without_a_serial() {
        let devices = [device(Some("BY0042"), "/dev/hidraw0")];
        let chosen = select_device(&devices, None, BYONOY_VENDOR_ID, ABSORBANCE_96_PRODUCT_ID)
            .expect("one device needs no disambiguation");
        assert_eq!(chosen, &devices[0]);
    }

    #[test]
    fn multiple_devices_without_a_serial_are_a_hard_error_naming_the_count() {
        let devices = [
            device(Some("BY0042"), "/dev/hidraw0"),
            device(Some("BY0043"), "/dev/hidraw1"),
        ];
        let error = select_device(&devices, None, BYONOY_VENDOR_ID, ABSORBANCE_96_PRODUCT_ID)
            .expect_err("two readers cannot be told apart without a serial");
        assert_eq!(
            error,
            HidError::MultipleDevices {
                vendor_id: BYONOY_VENDOR_ID,
                product_id: ABSORBANCE_96_PRODUCT_ID,
                count: 2,
            }
        );
        assert_eq!(
            error.to_string(),
            "2 readers share USB ids 16D0:1199; pass a serial number to choose one"
        );
    }

    #[test]
    fn a_serial_number_selects_among_several_devices() {
        let devices = [
            device(Some("BY0042"), "/dev/hidraw0"),
            device(Some("BY0043"), "/dev/hidraw1"),
        ];
        let chosen = select_device(
            &devices,
            Some("BY0043"),
            BYONOY_VENDOR_ID,
            ABSORBANCE_96_PRODUCT_ID,
        )
        .expect("the serial names the second reader");
        assert_eq!(chosen, &devices[1]);
    }

    #[test]
    fn a_serial_matching_nothing_or_several_devices_is_rejected() {
        let devices = [
            device(Some("BY0042"), "/dev/hidraw0"),
            device(Some("BY0042"), "/dev/hidraw1"),
        ];
        assert_eq!(
            select_device(
                &devices,
                Some("BY9999"),
                BYONOY_VENDOR_ID,
                ABSORBANCE_96_PRODUCT_ID
            )
            .expect_err("no reader carries that serial"),
            HidError::SerialNotFound {
                vendor_id: BYONOY_VENDOR_ID,
                product_id: ABSORBANCE_96_PRODUCT_ID,
                serial: "BY9999".to_string(),
            }
        );
        assert_eq!(
            select_device(
                &devices,
                Some("BY0042"),
                BYONOY_VENDOR_ID,
                ABSORBANCE_96_PRODUCT_ID
            )
            .expect_err("two readers claim the same serial"),
            HidError::AmbiguousSerial {
                vendor_id: BYONOY_VENDOR_ID,
                product_id: ABSORBANCE_96_PRODUCT_ID,
                serial: "BY0042".to_string(),
                count: 2,
            }
        );
    }

    #[test]
    fn an_empty_enumeration_is_device_not_found() {
        assert_eq!(
            select_device(&[], None, BYONOY_VENDOR_ID, ABSORBANCE_96_PRODUCT_ID)
                .expect_err("nothing was enumerated"),
            HidError::DeviceNotFound {
                vendor_id: BYONOY_VENDOR_ID,
                product_id: ABSORBANCE_96_PRODUCT_ID,
            }
        );
    }
}
