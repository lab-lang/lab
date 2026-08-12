//! The runtime half of tracing: the sink that stamps virtual time on each
//! event. The trace document schema itself (`lab.sim-trace.v0`) lives in
//! `lab-runfmt` and is re-exported here.

use std::cell::RefCell;
use std::rc::Rc;

pub use lab_runfmt::{
    AttentionWindow, SIM_TRACE_FORMAT, SimSummary, SimTraceDocument, StationSummary, TimedEvent,
    summarize,
};

use crate::clock::VirtualClock;
use crate::events::{EventSink, RunEvent};

/// The sink a simulation emits through: stamps every event with the shared
/// clock's current time.
pub struct TraceSink {
    clock: Rc<RefCell<VirtualClock>>,
    pub events: Vec<TimedEvent>,
}

impl TraceSink {
    pub fn new(clock: Rc<RefCell<VirtualClock>>) -> Self {
        Self {
            clock,
            events: Vec::new(),
        }
    }
}

impl EventSink for TraceSink {
    fn emit(&mut self, event: RunEvent) {
        self.events.push(TimedEvent {
            t: self.clock.borrow().elapsed_seconds(),
            event,
        });
    }
}
