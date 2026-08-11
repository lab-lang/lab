//! The event port: everything a run walk has to say goes through one sink.
//! The live runner's sink prints human narration; the simulator's sink
//! stamps virtual time on each event and accumulates the trace.

use serde::{Deserialize, Serialize};

/// One observable moment in a run walk. Events carry facts, not phrasing;
/// each sink decides how to present them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum RunEvent {
    /// The walk is about to start `pending` nodes, skipping `completed`.
    Planned {
        pending: usize,
        completed: usize,
    },
    Connecting {
        station: String,
        detail: String,
    },
    Connected {
        station: String,
    },
    NodeStarted {
        id: String,
    },
    NodeSkipped {
        id: String,
    },
    NodeCompleted {
        id: String,
    },
    /// A station program began: a STAR frame sequence or a thermal profile.
    ProgramStarted {
        station: String,
        title: String,
        #[serde(flatten)]
        extent: ProgramExtent,
    },
    /// One STAR frame is about to execute. When the frame carries deck
    /// coordinates, the first channel's target rides along so a viewer
    /// can move a pipetting head to where the work happens.
    Frame {
        station: String,
        index: usize,
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x_mm: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y_mm: Option<f64>,
    },
    /// The thermal profile is running to completion on its station.
    ThermalRunning {
        station: String,
    },
    ThermalWarning {
        station: String,
        warning: String,
    },
    /// The block holds a temperature until retrieval.
    ThermalHold {
        station: String,
        celsius: f64,
    },
    DoorOpened {
        station: String,
    },
    DoorClosed {
        station: String,
    },
    /// The operator is needed, starting now.
    AttentionRequired {
        node: String,
        prompt: String,
    },
    /// The operator's step is done; the walk is unattended again.
    AttentionReleased {
        node: String,
    },
    /// Labware physically moved between stations.
    LabwareMoved {
        labware: String,
        from: String,
        to: String,
    },
}

/// How large a station program is, in the unit the station thinks in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramExtent {
    Frames {
        frames: usize,
    },
    Plateaus {
        plateaus: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_hold_celsius: Option<f64>,
    },
}

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
