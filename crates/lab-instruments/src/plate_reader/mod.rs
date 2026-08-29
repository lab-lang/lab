//! Plate measurements, the plate-reader interface, and concrete adapters.

mod byonoy;

pub use byonoy::{ByonoyStation, ByonoyStationError};

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// How a reader states which wavelengths it can measure.
///
/// Filter- and LED-based instruments carry a small fixed set chosen at
/// purchase (the Byonoy Absorbance 96 ships up to six LEDs);
/// monochromator instruments sweep a range.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WavelengthSupport {
    Discrete(Vec<u16>),
    Range { min_nm: u16, max_nm: u16 },
}

impl WavelengthSupport {
    pub fn supports(&self, nm: u16) -> bool {
        match self {
            Self::Discrete(list) => list.contains(&nm),
            Self::Range { min_nm, max_nm } => (*min_nm..=*max_nm).contains(&nm),
        }
    }
}

/// What one reader can do, queried before a plan step is attempted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderCapabilities {
    pub absorbance: Option<WavelengthSupport>,
    pub luminescence: bool,
    /// Whether the device can report if a plate is seated.
    pub plate_sensing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeasurementUnit {
    /// Optical density (absorbance, unitless).
    Od,
    /// Relative luminescence units per integration period.
    Rlu,
}

/// One plate's worth of measurements, row-major with A1 first. `None`
/// marks a well the device did not measure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlateData {
    pub rows: usize,
    pub cols: usize,
    pub values: Vec<Option<f64>>,
    pub unit: MeasurementUnit,
    /// Instrument temperature at read time, on devices that report one.
    pub temperature_celsius: Option<f64>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PlateDataError {
    #[error("{rows}×{cols} plate data needs {expected} values, found {found}")]
    WrongLength {
        rows: usize,
        cols: usize,
        expected: usize,
        found: usize,
    },
}

impl PlateData {
    pub fn new(
        rows: usize,
        cols: usize,
        values: Vec<Option<f64>>,
        unit: MeasurementUnit,
    ) -> Result<Self, PlateDataError> {
        if values.len() != rows * cols {
            return Err(PlateDataError::WrongLength {
                rows,
                cols,
                expected: rows * cols,
                found: values.len(),
            });
        }
        Ok(Self {
            rows,
            cols,
            values,
            unit,
            temperature_celsius: None,
        })
    }

    /// The value at a zero-based row/column, `None` outside the plate or
    /// where the device measured nothing.
    pub fn value(&self, row: usize, col: usize) -> Option<f64> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.values[row * self.cols + col]
    }
}

/// A standalone plate reader.
///
/// Reads are whole-plate: instruments that can mask wells still integrate
/// every sensor, so well selection is a reporting concern the caller
/// applies to the returned data. Plate access is physical — a reader with
/// no drawer relies on whoever (or whatever) carries the plate, which is
/// represented as an explicit material-movement node in a reviewed facility plan.
pub trait PlateReader {
    type Error: std::error::Error + Send + Sync + 'static;

    fn capabilities(&self) -> ReaderCapabilities;

    /// Measures the whole plate at one wavelength. The wavelength must be
    /// one the device supports; drivers reject others by name rather than
    /// measuring something else.
    fn read_absorbance(&mut self, wavelength_nm: u16) -> Result<PlateData, Self::Error>;

    /// Measures the whole plate's luminescence over one integration period.
    fn read_luminescence(&mut self, integration: Duration) -> Result<PlateData, Self::Error>;

    /// Whether a plate is seated: `Some(bool)` from devices that sense it,
    /// `None` from devices that cannot say.
    fn plate_present(&mut self) -> Result<Option<bool>, Self::Error>;

    /// Cancels an in-flight measurement, if any.
    fn abort(&mut self) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discrete_and_ranged_wavelength_support_answer_membership() {
        let byonoy_like = WavelengthSupport::Discrete(vec![450, 600, 660]);
        assert!(byonoy_like.supports(600));
        assert!(
            !byonoy_like.supports(405),
            "an uninstalled LED is unsupported"
        );
        let monochromator = WavelengthSupport::Range {
            min_nm: 230,
            max_nm: 999,
        };
        assert!(monochromator.supports(405));
        assert!(!monochromator.supports(1000));
    }

    #[test]
    fn plate_data_rejects_a_value_count_that_does_not_match_the_plate() {
        let error = PlateData::new(8, 12, vec![Some(0.1); 95], MeasurementUnit::Od)
            .expect_err("95 values cannot fill a 96-well plate");
        assert_eq!(
            error,
            PlateDataError::WrongLength {
                rows: 8,
                cols: 12,
                expected: 96,
                found: 95,
            }
        );
    }

    #[test]
    fn plate_values_address_row_major_with_a1_first() {
        let mut values = vec![None; 96];
        values[0] = Some(0.11); // A1
        values[13] = Some(0.22); // B2
        let data = PlateData::new(8, 12, values, MeasurementUnit::Od).expect("shape matches");
        assert_eq!(data.value(0, 0), Some(0.11));
        assert_eq!(data.value(1, 1), Some(0.22));
        assert_eq!(data.value(0, 1), None, "unmeasured wells read as None");
        assert_eq!(
            data.value(9, 0),
            None,
            "off-plate reads are None, not panics"
        );
    }
}
