//! The clock port used to stamp durable run-ledger entries.

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
