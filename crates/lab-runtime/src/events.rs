//! The event port: everything a run walk has to say goes through one sink.
//! The live runner's sink prints human narration; the simulator's sink
//! stamps virtual time on each event and accumulates the trace.
//!
//! The event vocabulary itself is trace schema and lives in `lab-runfmt`;
//! this module owns the runtime ports that carry it.

pub use lab_runfmt::{ProgramExtent, RunEvent};

pub trait EventSink {
    fn emit(&mut self, event: RunEvent);
}

/// The first channel's deck target in a STAR firmware frame, in
/// millimeters, when the frame carries `xp`/`yp` position parameters.
/// Purely observational: the machine plans nothing from this.
pub fn frame_position(frame: &str) -> Option<(f64, f64)> {
    fn parameter(frame: &str, key: &str) -> Option<f64> {
        let start = frame.find(key)? + key.len();
        let digits: String = frame[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if digits.is_empty() {
            return None;
        }
        // Firmware positions are 0.1 mm units.
        Some(digits.parse::<f64>().ok()? / 10.0)
    }
    Some((parameter(frame, "xp")?, parameter(frame, "yp")?))
}

/// A sink that discards everything, for tests that only assert outcomes.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&mut self, _event: RunEvent) {}
}

/// A sink that keeps every event, for tests that assert the walk's shape.
#[derive(Default)]
pub struct RecordingSink {
    pub events: Vec<RunEvent>,
}

impl EventSink for RecordingSink {
    fn emit(&mut self, event: RunEvent) {
        self.events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pickup_frame_yields_its_first_channel_target() {
        let frame = "C0TPxp01179 01179 00000&yp2418 2328 0000&tm1 1 0&tt01tp2244tz2164th2450td0";
        assert_eq!(frame_position(frame), Some((117.9, 241.8)));
    }

    #[test]
    fn a_frame_without_positions_yields_none() {
        assert_eq!(frame_position("C0ZA"), None);
        assert_eq!(
            frame_position("C0TTtt00tf1tl0519tv03600tg2tu0"),
            None,
            "tip definitions carry no deck target"
        );
    }
}
