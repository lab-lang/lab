//! The event port: everything a live or dry run has to say goes through one
//! sink. The CLI's sink turns these facts into operator-facing narration.

/// One observable moment in a reviewed facility run.
#[derive(Clone, Debug, PartialEq)]
pub enum RunEvent {
    /// The walk is about to start `pending` nodes, skipping `completed`.
    Planned {
        pending: usize,
        completed: usize,
    },
    Connecting {
        asset: String,
        detail: String,
    },
    Connected {
        asset: String,
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
    /// One reviewed child document is about to execute through its exact Asset/adapter binding.
    DocumentStarted {
        asset: String,
        driver: String,
        format: String,
        title: String,
    },
    /// An Asset program began: a STAR frame sequence or a thermal profile.
    ProgramStarted {
        asset: String,
        title: String,
        extent: ProgramExtent,
    },
    /// One STAR frame is about to execute.
    Frame {
        asset: String,
        index: usize,
        description: String,
    },
    /// The thermal profile is running to completion on its bound Asset.
    ThermalRunning {
        asset: String,
    },
    ThermalWarning {
        asset: String,
        warning: String,
    },
    /// The block holds a temperature until retrieval.
    ThermalHold {
        asset: String,
        celsius: f64,
    },
    DoorOpened {
        asset: String,
    },
    DoorClosed {
        asset: String,
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

/// How large a device program is, in the unit the device thinks in.
#[derive(Clone, Debug, PartialEq)]
pub enum ProgramExtent {
    Frames {
        frames: usize,
    },
    Plateaus {
        plateaus: usize,
        final_hold_celsius: Option<f64>,
    },
}

pub trait EventSink {
    fn emit(&mut self, event: RunEvent);
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
