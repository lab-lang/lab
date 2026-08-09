//! The Byonoy Absorbance 96 as a workcell plate-reader station.

use std::time::Duration;

use lab_byonoy::{Absorbance96, Absorbance96Error, AbsorbanceMeasurement, SlotState};
use lab_instruments::{
    MeasurementUnit, PlateData, PlateReader, ReaderCapabilities, WavelengthSupport,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ByonoyStationError {
    #[error(transparent)]
    Device(#[from] Absorbance96Error),
    #[error(
        "the Absorbance 96 has no luminescence optics; a luminescence read needs a different station"
    )]
    LuminescenceUnsupported,
}

/// An Absorbance 96 session speaking Lab's [`PlateReader`] capability.
pub struct ByonoyStation {
    device: Absorbance96,
}

impl ByonoyStation {
    /// Wraps an open vendor session.
    pub fn new(device: Absorbance96) -> ByonoyStation {
        ByonoyStation { device }
    }

    /// Opens the one connected Absorbance 96 and wraps the session.
    #[cfg(feature = "hid")]
    pub fn open() -> Result<ByonoyStation, ByonoyStationError> {
        Ok(ByonoyStation::new(Absorbance96::open()?))
    }

    /// The wrapped vendor session, for vendor-specific work the
    /// capability trait does not model.
    pub fn device(&mut self) -> &mut Absorbance96 {
        &mut self.device
    }
}

/// The vendor's whole-plate measurement in Lab's vocabulary: every well
/// present, optical density, no instrument temperature.
fn plate_data_from(measurement: &AbsorbanceMeasurement) -> PlateData {
    let values: Vec<Option<f64>> = measurement
        .rows
        .iter()
        .flatten()
        .map(|&value| Some(f64::from(value)))
        .collect();
    PlateData::new(measurement.rows.len(), 12, values, MeasurementUnit::Od)
        .expect("each vendor row carries exactly twelve wells")
}

impl PlateReader for ByonoyStation {
    type Error = ByonoyStationError;

    fn capabilities(&self) -> ReaderCapabilities {
        ReaderCapabilities {
            absorbance: Some(WavelengthSupport::Discrete(
                self.device.installed_wavelengths().to_vec(),
            )),
            luminescence: false,
            plate_sensing: true,
        }
    }

    fn read_absorbance(&mut self, wavelength_nm: u16) -> Result<PlateData, ByonoyStationError> {
        Ok(plate_data_from(
            &self.device.measure_absorbance(wavelength_nm)?,
        ))
    }

    fn read_luminescence(
        &mut self,
        _integration: Duration,
    ) -> Result<PlateData, ByonoyStationError> {
        Err(ByonoyStationError::LuminescenceUnsupported)
    }

    fn plate_present(&mut self) -> Result<Option<bool>, ByonoyStationError> {
        Ok(match self.device.status()?.slot_state {
            SlotState::Occupied => Some(true),
            SlotState::Empty => Some(false),
            SlotState::Unknown | SlotState::Undetermined => None,
        })
    }

    fn abort(&mut self) -> Result<(), ByonoyStationError> {
        self.device
            .abort_handle()
            .abort()
            .map_err(Absorbance96Error::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_measurement_translates_into_row_major_plate_data() {
        let measurement = AbsorbanceMeasurement {
            wavelength_nm: 600,
            rows: (0..8)
                .map(|row| std::array::from_fn(|column| row as f32 + column as f32 / 100.0))
                .collect(),
        };
        let plate = plate_data_from(&measurement);
        assert_eq!((plate.rows, plate.cols), (8, 12));
        assert_eq!(plate.unit, MeasurementUnit::Od);
        assert_eq!(
            plate.temperature_celsius, None,
            "the device reports no temperature"
        );
        assert_eq!(plate.value(0, 0), Some(0.0), "A1 leads the plate");
        assert_eq!(
            plate.value(2, 5),
            Some(f64::from(2.0f32 + 5.0 / 100.0)),
            "C6 sits at row 2, column 5"
        );
        assert_eq!(
            plate.value(7, 11),
            Some(f64::from(7.0f32 + 11.0 / 100.0)),
            "H12 ends the plate"
        );
    }
}
