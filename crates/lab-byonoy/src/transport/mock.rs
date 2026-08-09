//! A scripted in-memory transport for codec- and session-level tests.
//! Nothing here touches hardware.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::report::Packet;
use crate::transport::{HidError, HidTransport};

type Responder = Box<dyn Fn(&Packet) -> Vec<Packet> + Send + Sync>;

/// A mock transport: records every written packet and serves queued (or
/// responder-generated) packets to readers.
#[derive(Default)]
pub struct MockHidTransport {
    written: Mutex<Vec<Packet>>,
    incoming: Mutex<VecDeque<Packet>>,
    available: Condvar,
    responder: Mutex<Option<Responder>>,
}

impl MockHidTransport {
    pub fn new() -> MockHidTransport {
        MockHidTransport::default()
    }

    /// Every packet written so far, in order.
    pub fn written(&self) -> Vec<Packet> {
        self.written
            .lock()
            .expect("the mock lock is never poisoned")
            .clone()
    }

    /// Queues a packet for the reader.
    pub fn push_report(&self, packet: Packet) {
        self.incoming
            .lock()
            .expect("the mock lock is never poisoned")
            .push_back(packet);
        self.available.notify_all();
    }

    /// Installs a scripted responder: called with each written packet, its
    /// returned packets are queued as device replies. This is how tests
    /// play the firmware's side of a measurement.
    pub fn set_responder(
        &self,
        responder: impl Fn(&Packet) -> Vec<Packet> + Send + Sync + 'static,
    ) {
        *self
            .responder
            .lock()
            .expect("the mock lock is never poisoned") = Some(Box::new(responder));
    }
}

impl HidTransport for MockHidTransport {
    fn write_report(&self, packet: &Packet) -> Result<(), HidError> {
        self.written
            .lock()
            .expect("the mock lock is never poisoned")
            .push(*packet);
        let replies = self
            .responder
            .lock()
            .expect("the mock lock is never poisoned")
            .as_ref()
            .map(|respond| respond(packet))
            .unwrap_or_default();
        for reply in replies {
            self.push_report(reply);
        }
        Ok(())
    }

    fn read_report(&self, timeout: Duration) -> Result<Option<Packet>, HidError> {
        let mut incoming = self
            .incoming
            .lock()
            .expect("the mock lock is never poisoned");
        if let Some(packet) = incoming.pop_front() {
            return Ok(Some(packet));
        }
        let (mut incoming, _timed_out) = self
            .available
            .wait_timeout(incoming, timeout)
            .expect("the mock lock is never poisoned");
        Ok(incoming.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{self, RoutingTag};

    #[test]
    fn the_mock_records_writes_and_serves_queued_reports() {
        let transport = MockHidTransport::new();
        let request = report::status_request();
        transport
            .write_report(&request)
            .expect("mock writes never fail");
        let mut reply = [0u8; 64];
        reply[0..2].copy_from_slice(&report::STATUS.to_le_bytes());
        transport.push_report(reply);
        assert_eq!(transport.written(), vec![request], "the write is recorded");
        let read = transport
            .read_report(Duration::from_millis(10))
            .expect("mock reads never fail")
            .expect("a report is queued");
        assert_eq!(read, reply, "the queued report comes back verbatim");
    }

    #[test]
    fn a_responder_answers_each_write() {
        let transport = MockHidTransport::new();
        transport.set_responder(|packet| {
            let mut reply = [0u8; 64];
            reply[0..2].copy_from_slice(&report::report_id(packet).to_le_bytes());
            vec![reply]
        });
        transport
            .write_report(&report::supported_reports_request(RoutingTag::Query))
            .expect("mock writes never fail");
        let reply = transport
            .read_report(Duration::from_millis(10))
            .expect("mock reads never fail")
            .expect("the responder queued a reply");
        assert_eq!(
            report::report_id(&reply),
            report::SUPPORTED_REPORTS,
            "the responder echoes the report id"
        );
    }

    #[test]
    fn a_quiet_mock_reads_as_none_not_an_error() {
        let transport = MockHidTransport::new();
        let read = transport
            .read_report(Duration::from_millis(0))
            .expect("a timeout is not a transport error");
        assert_eq!(read, None, "nothing arrived within the deadline");
    }
}
