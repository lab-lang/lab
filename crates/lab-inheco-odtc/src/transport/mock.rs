//! A scripted in-memory transport for protocol- and session-level tests.
//! Nothing here touches the network.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::soap::{IncomingEvent, request_id_of};
use crate::transport::{SoapTransport, TransportError};

/// What a scripted responder answers a command with: the synchronous
/// response body plus any events the device would go on to POST.
#[derive(Clone, Debug, PartialEq)]
pub struct MockReply {
    pub response: String,
    pub events: Vec<IncomingEvent>,
}

impl MockReply {
    /// A reply with no follow-up events.
    pub fn sync(response: String) -> MockReply {
        MockReply {
            response,
            events: Vec::new(),
        }
    }
}

/// One command the mock saw, with its request id already extracted for
/// correlation assertions.
#[derive(Clone, Debug, PartialEq)]
pub struct SentCommand {
    /// The command name, from the `SOAPAction` tail.
    pub command: String,
    pub envelope: String,
    pub request_id: Option<u32>,
}

type Responder = Box<dyn Fn(&str, &str) -> MockReply + Send + Sync>;

/// A mock transport: records every sent command, answers through a
/// scripted responder, and serves injected events to
/// [`SoapTransport::receive_event`].
#[derive(Default)]
pub struct MockSoapTransport {
    sent: Mutex<Vec<SentCommand>>,
    responder: Mutex<Option<Responder>>,
    events: Mutex<VecDeque<IncomingEvent>>,
    available: Condvar,
}

impl MockSoapTransport {
    pub fn new() -> MockSoapTransport {
        MockSoapTransport::default()
    }

    /// Installs the responder: called with each command's name and full
    /// envelope, its reply is returned synchronously and its events are
    /// queued. This is how tests answer commands whose request ids they
    /// cannot predict.
    pub fn set_responder(
        &self,
        responder: impl Fn(&str, &str) -> MockReply + Send + Sync + 'static,
    ) {
        *self
            .responder
            .lock()
            .expect("the mock lock is never poisoned") = Some(Box::new(responder));
    }

    /// Queues a device-initiated event for the next receive.
    pub fn inject_event(&self, event: IncomingEvent) {
        self.events
            .lock()
            .expect("the mock lock is never poisoned")
            .push_back(event);
        self.available.notify_all();
    }

    /// Every command sent so far, in order.
    pub fn sent(&self) -> Vec<SentCommand> {
        self.sent
            .lock()
            .expect("the mock lock is never poisoned")
            .clone()
    }

    /// The command names sent so far, in order — the shape most sequence
    /// assertions want.
    pub fn sent_names(&self) -> Vec<String> {
        self.sent()
            .into_iter()
            .map(|command| command.command)
            .collect()
    }
}

impl SoapTransport for MockSoapTransport {
    fn send(&self, soap_action: &str, envelope: &str) -> Result<String, TransportError> {
        let command = soap_action
            .rsplit('/')
            .next()
            .unwrap_or(soap_action)
            .to_string();
        self.sent
            .lock()
            .expect("the mock lock is never poisoned")
            .push(SentCommand {
                command: command.clone(),
                envelope: envelope.to_string(),
                request_id: request_id_of(envelope),
            });
        let reply = {
            let responder = self
                .responder
                .lock()
                .expect("the mock lock is never poisoned");
            let Some(responder) = responder.as_ref() else {
                return Err(TransportError::Unscripted { command });
            };
            responder(&command, envelope)
        };
        for event in reply.events {
            self.inject_event(event);
        }
        Ok(reply.response)
    }

    fn event_receiver_uri(&self) -> String {
        "http://192.0.2.1:49152/".to_string()
    }

    fn receive_event(&self, timeout: Duration) -> Result<Option<IncomingEvent>, TransportError> {
        let mut events = self.events.lock().expect("the mock lock is never poisoned");
        if let Some(event) = events.pop_front() {
            return Ok(Some(event));
        }
        let (mut events, _timed_out) = self
            .available
            .wait_timeout(events, timeout)
            .expect("the mock lock is never poisoned");
        Ok(events.pop_front())
    }
}

/// Renders a device-side synchronous response for tests: the standard
/// `<Command>Response`/`<Command>Result` nesting, with any extra
/// elements appended as siblings of the result block (the way `GetStatus`
/// carries `state`).
pub fn sync_response(
    command: &str,
    return_code: i32,
    message: &str,
    extra: &[(&str, &str)],
) -> String {
    let extra_elements: String = extra
        .iter()
        .map(|(name, text)| format!("<{name}>{text}</{name}>"))
        .collect();
    format!(
        "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
         <{command}Response xmlns=\"http://sila.coop\"><{command}Result>\
         <returnCode>{return_code}</returnCode><message>{message}</message>\
         <duration>PT0.001S</duration><deviceClass>0</deviceClass>\
         </{command}Result>{extra_elements}</{command}Response></s:Body></s:Envelope>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soap::{Command, StatusEvent, SyncResponse};

    #[test]
    fn the_mock_records_sends_and_answers_through_its_responder() {
        let transport = MockSoapTransport::new();
        transport
            .set_responder(|command, _| MockReply::sync(sync_response(command, 1, "Success", &[])));
        let envelope = Command::GetStatus.envelope(9);
        let body = transport
            .send(&Command::GetStatus.soap_action(), &envelope)
            .expect("the responder answers");
        let response = SyncResponse::parse(&body).expect("the scripted response parses");
        assert_eq!(response.command, "GetStatusResponse");
        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].command, "GetStatus");
        assert_eq!(
            sent[0].request_id,
            Some(9),
            "the request id is extracted for correlation"
        );
    }

    #[test]
    fn an_unscripted_command_is_a_typed_error() {
        let transport = MockSoapTransport::new();
        let error = transport
            .send(
                &Command::Initialize.soap_action(),
                &Command::Initialize.envelope(1),
            )
            .expect_err("no responder is installed");
        assert_eq!(
            error,
            TransportError::Unscripted {
                command: "Initialize".to_string()
            }
        );
    }

    #[test]
    fn injected_events_are_served_in_order_and_absence_times_out_to_none() {
        let transport = MockSoapTransport::new();
        transport.inject_event(IncomingEvent::Status(StatusEvent {
            state: Some("busy".to_string()),
        }));
        transport.inject_event(IncomingEvent::Status(StatusEvent {
            state: Some("idle".to_string()),
        }));
        let first = transport
            .receive_event(Duration::from_millis(10))
            .expect("the mock queue never fails");
        assert_eq!(
            first,
            Some(IncomingEvent::Status(StatusEvent {
                state: Some("busy".to_string())
            }))
        );
        let second = transport
            .receive_event(Duration::from_millis(10))
            .expect("the mock queue never fails");
        assert_eq!(
            second,
            Some(IncomingEvent::Status(StatusEvent {
                state: Some("idle".to_string())
            }))
        );
        let third = transport
            .receive_event(Duration::from_millis(10))
            .expect("the mock queue never fails");
        assert_eq!(
            third, None,
            "an empty queue times out to None, not an error"
        );
    }
}
