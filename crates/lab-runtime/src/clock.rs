//! The clock port: live runs read the wall clock, simulated runs advance a
//! virtual one.

use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock {
    /// Seconds since the Unix epoch, as the ledger records them.
    fn now_unix(&self) -> u64;
}

/// The wall clock the live runner stamps ledger entries with.
pub struct WallClock;

impl Clock for WallClock {
    fn now_unix(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}

/// A clock that only moves when the simulation says so. Time starts at an
/// origin and accumulates fractional seconds as simulated work completes.
#[derive(Clone, Debug, Default)]
pub struct VirtualClock {
    origin_unix: u64,
    elapsed_seconds: f64,
}

impl VirtualClock {
    pub fn new(origin_unix: u64) -> Self {
        Self {
            origin_unix,
            elapsed_seconds: 0.0,
        }
    }

    /// Moves the clock forward. Negative durations are a programming error
    /// and are ignored rather than rewinding recorded history.
    pub fn advance(&mut self, seconds: f64) {
        if seconds > 0.0 {
            self.elapsed_seconds += seconds;
        }
    }

    /// Seconds elapsed since the simulation began.
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }
}

impl Clock for VirtualClock {
    fn now_unix(&self) -> u64 {
        self.origin_unix + self.elapsed_seconds as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_virtual_clock_accumulates_and_never_rewinds() {
        let mut clock = VirtualClock::new(1_000);
        clock.advance(90.0);
        clock.advance(0.5);
        clock.advance(-30.0);
        assert_eq!(clock.elapsed_seconds(), 90.5);
        assert_eq!(clock.now_unix(), 1_090);
    }
}
