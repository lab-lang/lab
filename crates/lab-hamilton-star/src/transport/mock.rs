//! A scripted in-memory transport for protocol- and session-level tests.
//! Nothing here touches hardware.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::transport::{Transport, TransportError};

type Responder = Box<dyn Fn(&str) -> Vec<String> + Send + Sync>;

/// A mock transport: records every written command and serves queued (or
/// responder-generated) responses to readers.
#[derive(Default)]
pub struct MockTransport {
    written: Mutex<Vec<String>>,
    incoming: Mutex<VecDeque<Vec<u8>>>,
    available: Condvar,
    responder: Mutex<Option<Responder>>,
}

impl MockTransport {
    pub fn new() -> MockTransport {
        MockTransport::default()
    }

    /// Every command written so far, in order.
    pub fn written(&self) -> Vec<String> {
        self.written
            .lock()
            .expect("the mock lock is never poisoned")
            .clone()
    }

    /// Queues a response for the reader.
    pub fn push_response(&self, response: &str) {
        self.incoming
            .lock()
            .expect("the mock lock is never poisoned")
            .push_back(response.as_bytes().to_vec());
        self.available.notify_all();
    }

    /// Installs a scripted responder: called with each written command, its
    /// returned strings are queued as responses. This is how tests answer
    /// commands whose ids they cannot predict.
    pub fn set_responder(&self, responder: impl Fn(&str) -> Vec<String> + Send + Sync + 'static) {
        *self
            .responder
            .lock()
            .expect("the mock lock is never poisoned") = Some(Box::new(responder));
    }
}

impl Transport for MockTransport {
    fn write_message(&self, data: &[u8]) -> Result<(), TransportError> {
        let text = String::from_utf8_lossy(data).to_string();
        self.written
            .lock()
            .expect("the mock lock is never poisoned")
            .push(text.clone());
        let replies = self
            .responder
            .lock()
            .expect("the mock lock is never poisoned")
            .as_ref()
            .map(|respond| respond(&text))
            .unwrap_or_default();
        for reply in replies {
            self.push_response(&reply);
        }
        Ok(())
    }

    fn read_message(&self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        let mut incoming = self
            .incoming
            .lock()
            .expect("the mock lock is never poisoned");
        if let Some(message) = incoming.pop_front() {
            return Ok(Some(message));
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

    #[test]
    fn the_mock_records_writes_and_serves_queued_responses() {
        let transport = MockTransport::new();
        transport
            .write_message(b"C0RTid0001")
            .expect("mock writes never fail");
        transport.push_response("C0RTid0001er00/00rt0 0");
        assert_eq!(
            transport.written(),
            vec!["C0RTid0001".to_string()],
            "the write is recorded"
        );
        let response = transport
            .read_message(Duration::from_millis(10))
            .expect("mock reads never fail")
            .expect("a response is queued");
        assert_eq!(
            response, b"C0RTid0001er00/00rt0 0",
            "the queued response comes back verbatim"
        );
    }

    #[test]
    fn a_responder_answers_each_write() {
        let transport = MockTransport::new();
        transport.set_responder(|command| vec![format!("{}er00/00", &command[..10])]);
        transport
            .write_message(b"C0ZAid0007")
            .expect("mock writes never fail");
        let response = transport
            .read_message(Duration::from_millis(10))
            .expect("mock reads never fail")
            .expect("the responder queued a reply");
        assert_eq!(
            response, b"C0ZAid0007er00/00",
            "the responder echoes the envelope"
        );
    }
}
