//! The Hamilton tip catalog and the liquid-class volume-correction
//! mechanism, with a vendored water table.
//!
//! The firmware holds a volatile tip-type table (indices 0–99, defined via
//! `TT`, erased at power-off); the session re-defines cached types after
//! reconnect. The wire length `tl` is the total tip length minus the
//! fitting depth (the length that slides onto the channel cone); the wire
//! volume `tv` has a floor of 1 µL so zero-volume probing "tips" still
//! register.

use crate::commands::system::{TipPickupMethod, TipSizeCode};
use crate::units::Millimeters;

/// A tip's physical description, the input to the firmware `TT` definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipType {
    /// Total tip length in millimeters.
    pub total_length: Millimeters,
    /// Usable volume in microliters.
    pub max_volume: f64,
    pub size: TipSizeCode,
    pub has_filter: bool,
    pub pickup_method: TipPickupMethod,
}

/// The fitting depth for a size class: how far the tip slides onto the
/// channel cone. Low-, standard-, and high-volume tips fit 8 mm; CoRe 384
/// tips 7.55 mm; XL tips 10 mm.
pub fn fitting_depth(size: TipSizeCode) -> Millimeters {
    match size {
        TipSizeCode::LowVolume | TipSizeCode::Standard | TipSizeCode::HighVolume => {
            Millimeters(8.0)
        }
        TipSizeCode::Core384 => Millimeters(7.55),
        TipSizeCode::Xl => Millimeters(10.0),
    }
}

impl TipType {
    /// The `tl` wire value: (total length − fitting depth) in 0.1 mm.
    pub fn wire_length(&self) -> u32 {
        ((self.total_length.0 - fitting_depth(self.size).0) * 10.0)
            .round()
            .max(0.0) as u32
    }

    /// The `tv` wire value: the volume in 0.1 µL with a 1 µL floor, so
    /// zero-volume probing tips still register in the firmware table.
    pub fn wire_volume(&self) -> u32 {
        ((self.max_volume * 10.0).round() as u32).max(10)
    }

    /// The value identity used by the session's tip-type cache: two tips
    /// with equal wire encodings share a firmware index.
    pub fn cache_key(&self) -> (u32, u32, u32, bool, u32) {
        (
            self.wire_length(),
            self.wire_volume(),
            self.size.code(),
            self.has_filter,
            self.pickup_method.code(),
        )
    }

    /// The empirical size-class Z correction applied to tip pickup heights,
    /// in 0.1 mm: low-volume tips sit 0.2 mm higher, non-standard tips
    /// 0.2 mm lower.
    pub fn pickup_z_correction(&self) -> i32 {
        match self.size {
            TipSizeCode::LowVolume => 2,
            TipSizeCode::Standard => 0,
            _ => -2,
        }
    }
}

/// 10 µL low-volume filter tip: 29.9 mm, 10 µL usable.
pub const TIP_10UL_FILTER: TipType = TipType {
    total_length: Millimeters(29.9),
    max_volume: 10.0,
    size: TipSizeCode::LowVolume,
    has_filter: true,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 50 µL tip: 50.4 mm, 60 µL usable.
pub const TIP_50UL: TipType = TipType {
    total_length: Millimeters(50.4),
    max_volume: 60.0,
    size: TipSizeCode::Standard,
    has_filter: false,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 300 µL standard-volume tip: 59.9 mm, 400 µL usable.
pub const TIP_300UL: TipType = TipType {
    total_length: Millimeters(59.9),
    max_volume: 400.0,
    size: TipSizeCode::Standard,
    has_filter: false,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 300 µL standard-volume filter tip: 59.9 mm, 360 µL usable.
pub const TIP_300UL_FILTER: TipType = TipType {
    total_length: Millimeters(59.9),
    max_volume: 360.0,
    size: TipSizeCode::Standard,
    has_filter: true,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 300 µL slim filter tip: 94.8 mm, 360 µL usable, high-volume collar.
pub const TIP_300UL_SLIM_FILTER: TipType = TipType {
    total_length: Millimeters(94.8),
    max_volume: 360.0,
    size: TipSizeCode::HighVolume,
    has_filter: true,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 1000 µL high-volume tip: 95.1 mm, 1250 µL usable.
pub const TIP_1000UL: TipType = TipType {
    total_length: Millimeters(95.1),
    max_volume: 1250.0,
    size: TipSizeCode::HighVolume,
    has_filter: false,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 1000 µL high-volume filter tip: 95.1 mm, 1065 µL usable.
pub const TIP_1000UL_FILTER: TipType = TipType {
    total_length: Millimeters(95.1),
    max_volume: 1065.0,
    size: TipSizeCode::HighVolume,
    has_filter: true,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 4 mL XL tip: 116 mm, 4367 µL usable.
pub const TIP_4ML: TipType = TipType {
    total_length: Millimeters(116.0),
    max_volume: 4367.0,
    size: TipSizeCode::Xl,
    has_filter: false,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// 5 mL XL tip: 116 mm, 5420 µL usable.
pub const TIP_5ML: TipType = TipType {
    total_length: Millimeters(116.0),
    max_volume: 5420.0,
    size: TipSizeCode::Xl,
    has_filter: false,
    pickup_method: TipPickupMethod::OutOfRack,
};

/// A liquid-class correction curve: target liquid volume (µL) to the piston
/// volume the firmware must be commanded with. Air compressibility,
/// viscosity, and tip geometry make the relationship nonlinear, so it is
/// calibrated pointwise and interpolated linearly, extrapolating on the
/// nearest segment outside the calibrated range.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionCurve {
    /// Calibration points `(target, commanded)` sorted by target.
    points: Vec<(f64, f64)>,
}

impl CorrectionCurve {
    /// Builds a curve from calibration points; they are sorted by target
    /// volume. A curve needs at least two points to interpolate.
    pub fn new(mut points: Vec<(f64, f64)>) -> Option<CorrectionCurve> {
        if points.len() < 2 {
            return None;
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(CorrectionCurve { points })
    }

    /// The identity curve: commanded volume equals target volume.
    pub fn identity() -> CorrectionCurve {
        CorrectionCurve {
            points: vec![(0.0, 0.0), (1.0, 1.0)],
        }
    }

    /// The commanded (piston) volume for a target liquid volume, in µL.
    pub fn corrected_volume(&self, target: f64) -> f64 {
        let points = &self.points;
        // Find the segment surrounding the target; the outermost segments
        // extend by extrapolation.
        let segment = points
            .windows(2)
            .find(|pair| target <= pair[1].0)
            .unwrap_or_else(|| &points[points.len() - 2..]);
        let (x0, y0) = segment[0];
        let (x1, y1) = segment[1];
        if x1 == x0 {
            return y0;
        }
        y0 + (target - x0) * (y1 - y0) / (x1 - x0)
    }
}

/// Water on a standard-volume (300 µL) filter tip, jet dispense.
pub fn water_standard_volume_filter_jet() -> CorrectionCurve {
    CorrectionCurve::new(vec![
        (0.0, 0.0),
        (20.0, 23.2),
        (50.0, 55.1),
        (100.0, 107.2),
        (200.0, 211.0),
        (300.0, 313.5),
    ])
    .expect("the vendored table has more than one point")
}

/// Water on a standard-volume (300 µL) filter tip, surface dispense.
pub fn water_standard_volume_filter_surface() -> CorrectionCurve {
    CorrectionCurve::new(vec![
        (0.0, 0.0),
        (0.5, 0.9),
        (1.0, 1.6),
        (2.0, 2.8),
        (5.0, 6.3),
        (10.0, 11.9),
        (20.0, 23.2),
        (50.0, 55.1),
        (100.0, 107.2),
        (200.0, 211.0),
        (300.0, 313.5),
    ])
    .expect("the vendored table has more than one point")
}

/// Water on a low-volume (10 µL) filter tip, surface dispense.
pub fn water_low_volume_filter_surface() -> CorrectionCurve {
    CorrectionCurve::new(vec![
        (0.0, 0.0),
        (0.5, 0.8),
        (1.0, 1.4),
        (2.0, 2.6),
        (5.0, 6.0),
        (10.0, 11.5),
        (15.0, 16.7),
    ])
    .expect("the vendored table has more than one point")
}

/// Water on a high-volume (1000 µL) filter tip, jet dispense.
pub fn water_high_volume_filter_jet() -> CorrectionCurve {
    CorrectionCurve::new(vec![
        (0.0, 0.0),
        (10.0, 13.3),
        (20.0, 24.6),
        (50.0, 57.2),
        (100.0, 109.6),
        (200.0, 212.9),
        (500.0, 521.7),
        (1000.0, 1034.0),
    ])
    .expect("the vendored table has more than one point")
}

/// Water on a high-volume (1000 µL) filter tip, surface dispense.
pub fn water_high_volume_filter_surface() -> CorrectionCurve {
    CorrectionCurve::new(vec![
        (0.0, 0.0),
        (10.0, 12.5),
        (20.0, 23.9),
        (50.0, 56.3),
        (100.0, 108.3),
        (200.0, 211.0),
        (500.0, 518.3),
        (1000.0, 1028.5),
    ])
    .expect("the vendored table has more than one point")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_300ul_filter_tip_encodes_the_golden_tt_values() {
        assert_eq!(
            TIP_300UL_FILTER.wire_length(),
            519,
            "59.9 mm total minus the 8 mm fitting depth is 51.9 mm"
        );
        assert_eq!(
            TIP_300UL_FILTER.wire_volume(),
            3600,
            "360 µL in 0.1 µL units"
        );
    }

    #[test]
    fn zero_volume_probing_tips_keep_the_one_microliter_floor() {
        let probe = TipType {
            max_volume: 0.0,
            ..TIP_300UL
        };
        assert_eq!(
            probe.wire_volume(),
            10,
            "the firmware rejects tv 0, so zero-volume tips register as 1 µL"
        );
    }

    #[test]
    fn pickup_corrections_follow_the_size_class() {
        assert_eq!(
            TIP_10UL_FILTER.pickup_z_correction(),
            2,
            "low-volume tips sit 0.2 mm higher"
        );
        assert_eq!(
            TIP_300UL.pickup_z_correction(),
            0,
            "standard tips need no correction"
        );
        assert_eq!(
            TIP_1000UL.pickup_z_correction(),
            -2,
            "high-volume tips sit 0.2 mm lower"
        );
        assert_eq!(
            TIP_5ML.pickup_z_correction(),
            -2,
            "XL tips sit 0.2 mm lower"
        );
    }

    #[test]
    fn water_at_100_microliters_commands_107_2() {
        let curve = water_standard_volume_filter_jet();
        assert!(
            (curve.corrected_volume(100.0) - 107.2).abs() < 1e-9,
            "the calibration point matches the golden aspirate's av01072"
        );
    }

    #[test]
    fn the_curve_interpolates_between_calibration_points() {
        let curve = water_standard_volume_filter_jet();
        let mid = curve.corrected_volume(150.0);
        assert!(
            (mid - 159.1).abs() < 1e-9,
            "150 µL interpolates halfway between 107.2 and 211.0, got {mid}"
        );
    }

    #[test]
    fn the_curve_extrapolates_beyond_the_calibrated_range() {
        let curve = water_standard_volume_filter_jet();
        let beyond = curve.corrected_volume(400.0);
        assert!(
            (beyond - 416.0).abs() < 1e-9,
            "400 µL extends the 200→300 segment linearly, got {beyond}"
        );
    }

    #[test]
    fn tips_with_equal_wire_encodings_share_a_cache_key() {
        let a = TIP_300UL_FILTER;
        let b = TipType {
            total_length: Millimeters(59.9),
            ..TIP_300UL_FILTER
        };
        assert_eq!(
            a.cache_key(),
            b.cache_key(),
            "the cache is keyed by value, not identity"
        );
    }
}
