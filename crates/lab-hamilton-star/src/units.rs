//! Measurement newtypes and the wire encodings the firmware expects.
//!
//! Master-level (`C0`) commands express positions in 0.1 mm steps, volumes in
//! 0.1 µL steps, pipetting speeds in 0.1 µL/s, Z and swap speeds in 0.1 mm/s,
//! and settling times in 0.1 s. Slave-direct commands (`P1`–`PG`, `H0`, `R0`)
//! express positions in motor increments with a per-axis conversion constant.
//! The types here are the only conversion path between engineering units and
//! wire integers, so a wrong constant can only live in one place.

/// A length in millimeters.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Millimeters(pub f64);

/// A volume in microliters.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Microliters(pub f64);

/// A wire position in 0.1 mm steps, the unit of every `C0`-level coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TenthMm(pub u32);

/// A wire volume in 0.1 µL steps, the unit of every `C0`-level volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TenthUl(pub u32);

impl Millimeters {
    /// Rounds to the nearest 0.1 mm wire step. Negative lengths round to zero
    /// because `C0` coordinate fields are unsigned.
    pub fn to_wire(self) -> TenthMm {
        TenthMm((self.0 * 10.0).round().max(0.0) as u32)
    }
}

impl TenthMm {
    pub fn to_millimeters(self) -> Millimeters {
        Millimeters(f64::from(self.0) / 10.0)
    }
}

impl Microliters {
    /// Rounds to the nearest 0.1 µL wire step. Negative volumes round to zero
    /// because volume fields are unsigned.
    pub fn to_wire(self) -> TenthUl {
        TenthUl((self.0 * 10.0).round().max(0.0) as u32)
    }
}

impl TenthUl {
    pub fn to_microliters(self) -> Microliters {
        Microliters(f64::from(self.0) / 10.0)
    }
}

/// A motor axis with a fixed millimeter-per-increment (or unit-per-increment)
/// conversion constant. Slave-direct commands address motors in increments;
/// each axis constant below is the measured firmware value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    /// Engineering units (mm, µL, or degrees) advanced by one motor increment.
    pub units_per_increment: f64,
}

impl Axis {
    /// Pipetting-channel Y drive: 0.046302083 mm per increment.
    pub const CHANNEL_Y: Axis = Axis {
        units_per_increment: 0.046302083,
    };
    /// Pipetting-channel Z drive: 0.01072765 mm per increment. The legal
    /// window 9320–31200 increments spans 99.98–334.7 mm.
    pub const CHANNEL_Z: Axis = Axis {
        units_per_increment: 0.01072765,
    };
    /// Pipetting-channel dispensing drive, volume: 0.046876 µL per increment.
    pub const CHANNEL_DISPENSE_UL: Axis = Axis {
        units_per_increment: 0.046876,
    };
    /// Pipetting-channel dispensing drive, travel: 0.002734375 mm per
    /// increment.
    pub const CHANNEL_DISPENSE_MM: Axis = Axis {
        units_per_increment: 0.002734375,
    };
    /// CoRe 96 head Z drive: 0.005 mm per increment. The legal window is
    /// 36100–68500 increments on legacy heads and 24200–76200 on FM-STAR.
    pub const HEAD96_Z: Axis = Axis {
        units_per_increment: 0.005,
    };
    /// CoRe 96 head Y drive: 0.015625 mm per increment.
    pub const HEAD96_Y: Axis = Axis {
        units_per_increment: 0.015625,
    };
    /// CoRe 96 head dispensing drive: 0.019340933 µL per increment (maximum
    /// 64350 increments on heads produced after 2013).
    pub const HEAD96_DISPENSE_UL: Axis = Axis {
        units_per_increment: 0.019340933,
    };
    /// CoRe 96 head squeezer drive: 0.0002086672009 mm per increment.
    pub const HEAD96_SQUEEZER: Axis = Axis {
        units_per_increment: 0.0002086672009,
    };
    /// iSWAP Y drive: 0.046302083 mm per increment, window 0–14000.
    pub const ISWAP_Y: Axis = Axis {
        units_per_increment: 0.046302083,
    };
    /// iSWAP Z drive: 0.01072765 mm per increment, window −187–26661.
    pub const ISWAP_Z: Axis = Axis {
        units_per_increment: 0.01072765,
    };
    /// iSWAP rotation drive: 0.00309619077 degrees per increment, window
    /// ±30032.
    pub const ISWAP_ROTATION: Axis = Axis {
        units_per_increment: 0.00309619077,
    };
    /// iSWAP wrist drive: 0.00507968798 degrees per increment, window ±30000.
    pub const ISWAP_WRIST: Axis = Axis {
        units_per_increment: 0.00507968798,
    };
    /// iSWAP gripper jaw drive: 0.00554337 mm per increment, jaw window
    /// 12780–24120 increments.
    pub const ISWAP_GRIPPER: Axis = Axis {
        units_per_increment: 0.00554337,
    };

    /// Converts engineering units to the nearest motor increment.
    pub fn increments_from(self, units: f64) -> i64 {
        (units / self.units_per_increment).round() as i64
    }

    /// Converts a motor increment count to engineering units.
    pub fn units_from(self, increments: i64) -> f64 {
        increments as f64 * self.units_per_increment
    }
}

/// Deck rail geometry: rails sit on a 22.5 mm pitch with rail 1 at
/// x = 100.0 mm.
pub const RAIL_PITCH_MM: f64 = 22.5;
/// Deck x position of rail 1 in millimeters.
pub const RAIL_ONE_X_MM: f64 = 100.0;

/// Returns the 1-based rail number for a deck x coordinate.
pub fn rail_for_x(x: Millimeters) -> f64 {
    (x.0 - RAIL_ONE_X_MM) / RAIL_PITCH_MM + 1.0
}

/// Default traverse height for pipetting channels: 245.0 mm.
pub const CHANNEL_TRAVERSE_HEIGHT: Millimeters = Millimeters(245.0);
/// Default traverse height for the iSWAP: 280.0 mm.
pub const ISWAP_TRAVERSE_HEIGHT: Millimeters = Millimeters(280.0);
/// Default tip fitting depth: 8 mm of the tip slides onto the channel cone.
pub const DEFAULT_TIP_FITTING_DEPTH: Millimeters = Millimeters(8.0);
/// Clearance added above the expected surface when starting an LLD search:
/// 5 mm.
pub const LLD_SEARCH_CLEARANCE: Millimeters = Millimeters(5.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimeters_round_to_the_nearest_tenth() {
        assert_eq!(
            Millimeters(224.44).to_wire(),
            TenthMm(2244),
            "224.44 mm rounds down to 2244 tenths"
        );
        assert_eq!(
            Millimeters(224.45).to_wire(),
            TenthMm(2245),
            "224.45 mm rounds up to 2245 tenths"
        );
    }

    #[test]
    fn a_hundred_microliters_on_the_head_dispense_drive_is_5170_increments() {
        assert_eq!(
            Axis::HEAD96_DISPENSE_UL.increments_from(100.0),
            5170,
            "100 µL ÷ 0.019340933 µL/increment rounds to 5170, matching the H0 PA wire example"
        );
    }

    #[test]
    fn rail_numbers_derive_from_the_22_5_mm_pitch() {
        assert_eq!(
            rail_for_x(Millimeters(100.0)),
            1.0,
            "rail 1 sits at x = 100 mm"
        );
        assert_eq!(
            rail_for_x(Millimeters(122.5)),
            2.0,
            "one 22.5 mm pitch to the right is rail 2"
        );
    }

    #[test]
    fn negative_lengths_clamp_to_zero_on_the_wire() {
        assert_eq!(
            Millimeters(-1.0).to_wire(),
            TenthMm(0),
            "unsigned coordinate fields cannot carry negative values"
        );
    }
}
