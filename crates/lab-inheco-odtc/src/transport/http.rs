//! The real transport: `ureq` for the client-to-device POSTs and a
//! hand-rolled HTTP/1.1 listener on a dedicated thread for the
//! device-to-client event POSTs. The envelope grammar both directions
//! speak is tiny and fixed, so no server framework is involved.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::soap::{IncomingEvent, RESPONSE_EVENT_ACK};
use crate::transport::{SoapTransport, TransportError};

/// How long the client waits for a device's synchronous HTTP response.
/// Synchronous replies arrive in milliseconds; a device that takes
/// longer than this is unreachable or wedged.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the listener waits on a half-sent event before dropping the
/// connection.
const EVENT_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest event body the listener accepts.
const MAX_EVENT_BODY_BYTES: usize = 4 * 1024 * 1024;

/// The HTTP transport: `ureq` toward the device, plus a listener thread
/// receiving, acknowledging, and queueing the device's event POSTs.
pub struct HttpSoapTransport {
    agent: ureq::Agent,
    endpoint: String,
    listener_uri: String,
    listener_addr: SocketAddr,
    queue: Arc<EventQueue>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct EventQueue {
    events: Mutex<VecDeque<IncomingEvent>>,
    available: Condvar,
}

impl EventQueue {
    fn push(&self, event: IncomingEvent) {
        self.events
            .lock()
            .expect("the event queue is never poisoned")
            .push_back(event);
        self.available.notify_all();
    }

    fn pop(&self, timeout: Duration) -> Option<IncomingEvent> {
        let mut events = self
            .events
            .lock()
            .expect("the event queue is never poisoned");
        if let Some(event) = events.pop_front() {
            return Some(event);
        }
        let (mut events, _timed_out) = self
            .available
            .wait_timeout(events, timeout)
            .expect("the event queue is never poisoned");
        events.pop_front()
    }
}

impl HttpSoapTransport {
    /// Connects the client side toward `device` (conventionally port
    /// 8080) and starts the callback listener on an ephemeral port of
    /// the local interface that routes to the device. The ODTC often
    /// lives on a link-local 169.254/16 network; binding the listener to
    /// the routed interface — never loopback or a guess — is what makes
    /// the callback URI reachable from the device's side.
    pub fn connect(device: SocketAddr) -> Result<HttpSoapTransport, TransportError> {
        let local_ip = local_ip_toward(device)?;
        let listener =
            TcpListener::bind((local_ip, 0)).map_err(|error| TransportError::Listener {
                detail: format!("binding an ephemeral port on {local_ip} failed: {error}"),
            })?;
        let listener_addr = listener
            .local_addr()
            .map_err(|error| TransportError::Listener {
                detail: format!("reading the bound listener address failed: {error}"),
            })?;
        let listener_uri = format!("http://{listener_addr}/");
        let queue = Arc::new(EventQueue::default());
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread = {
            let queue = Arc::clone(&queue);
            let shutdown = Arc::clone(&shutdown);
            std::thread::Builder::new()
                .name("odtc-event-listener".to_string())
                .spawn(move || listener_loop(&listener, &queue, &shutdown))
                .map_err(|error| TransportError::Listener {
                    detail: format!("spawning the listener thread failed: {error}"),
                })?
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Ok(HttpSoapTransport {
            agent,
            endpoint: format!("http://{device}/"),
            listener_uri,
            listener_addr,
            queue,
            shutdown,
            thread: Some(thread),
        })
    }
}

impl SoapTransport for HttpSoapTransport {
    fn send(&self, soap_action: &str, envelope: &str) -> Result<String, TransportError> {
        let mut response = self
            .agent
            .post(&self.endpoint)
            .header("Content-Type", "text/xml; charset=utf-8")
            .header("SOAPAction", soap_action)
            .send(envelope)
            .map_err(|error| TransportError::Http {
                detail: error.to_string(),
            })?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|error| TransportError::Http {
                detail: format!("reading the response body failed: {error}"),
            })
    }

    fn event_receiver_uri(&self) -> String {
        self.listener_uri.clone()
    }

    fn receive_event(&self, timeout: Duration) -> Result<Option<IncomingEvent>, TransportError> {
        Ok(self.queue.pop(timeout))
    }
}

impl Drop for HttpSoapTransport {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // A throwaway connection unblocks the accept loop so the thread
        // observes the shutdown flag.
        let _ = TcpStream::connect(self.listener_addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The interface that routes toward the device, discovered by
/// "connecting" a UDP socket at it and reading the local address back —
/// no packets flow; the OS only resolves the route.
fn local_ip_toward(device: SocketAddr) -> Result<IpAddr, TransportError> {
    let no_route = |detail: String| TransportError::NoLocalRoute {
        device: device.to_string(),
        detail,
    };
    let bind_address: SocketAddr = if device.is_ipv4() {
        (IpAddr::from([0u8, 0, 0, 0]), 0).into()
    } else {
        (IpAddr::from([0u16, 0, 0, 0, 0, 0, 0, 0]), 0).into()
    };
    let socket = UdpSocket::bind(bind_address)
        .map_err(|error| no_route(format!("binding a probe socket failed: {error}")))?;
    socket
        .connect(device)
        .map_err(|error| no_route(format!("routing the probe failed: {error}")))?;
    let local = socket
        .local_addr()
        .map_err(|error| no_route(format!("reading the probe's local address failed: {error}")))?;
    Ok(local.ip())
}

fn listener_loop(listener: &TcpListener, queue: &EventQueue, shutdown: &AtomicBool) {
    for stream in listener.incoming() {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        let Ok(stream) = stream else { continue };
        // Events are serialized by the device; one connection at a time
        // keeps the ack path simple and still prompt.
        let _ = handle_connection(stream, queue);
    }
}

/// Reads one HTTP request, parses the event, answers the matching canned
/// ack, and queues the event. A body that fails to parse is still
/// answered — with the generic ResponseEvent ack — so the device never
/// stalls on its own malformed POST.
fn handle_connection(mut stream: TcpStream, queue: &EventQueue) -> std::io::Result<()> {
    stream.set_read_timeout(Some(EVENT_READ_TIMEOUT))?;
    let body = read_http_request(&mut stream)?;
    let ack = match IncomingEvent::parse(&body) {
        Ok(event) => {
            let ack = event.ack();
            queue.push(event);
            ack
        }
        Err(_) => RESPONSE_EVENT_ACK,
    };
    write_http_response(&mut stream, ack)
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
        if buffer.len() > 64 * 1024 {
            return Err(std::io::Error::other("the request head never ended"));
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(std::io::Error::other("the connection closed mid-request"));
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_EVENT_BODY_BYTES {
        return Err(std::io::Error::other(
            "the event body exceeds the size limit",
        ));
    }
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(std::io::Error::other("the connection closed mid-body"));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(String::from_utf8_lossy(&buffer[header_end..header_end + content_length]).into_owned())
}

fn write_http_response(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soap::{ResponseEvent, STATUS_EVENT_ACK};

    fn response_event_document(request_id: u32) -> String {
        format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <ResponseEvent xmlns=\"http://sila.coop\"><requestId>{request_id}</requestId>\
             <returnValue><returnCode>3</returnCode><message>Success</message>\
             <duration>PT1.5S</duration><deviceClass>0</deviceClass></returnValue>\
             <responseData></responseData></ResponseEvent></s:Body></s:Envelope>"
        )
    }

    /// POSTs a raw HTTP request at the listener and returns the whole
    /// response, head and body.
    fn post_event(listener: SocketAddr, body: &str) -> String {
        let mut stream =
            TcpStream::connect(listener).expect("the listener accepts local connections");
        write!(
            stream,
            "POST / HTTP/1.1\r\nHost: {listener}\r\nContent-Type: text/xml; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("the request writes");
        stream.flush().expect("the request flushes");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("the listener answers before closing");
        response
    }

    #[test]
    fn the_listener_acks_a_posted_response_event_and_resolves_a_waiting_receive() {
        // The device address only shapes route discovery; nothing dials
        // it in this test.
        let transport =
            HttpSoapTransport::connect("127.0.0.1:8080".parse().expect("a socket addr"))
                .expect("the transport binds a loopback listener");
        assert!(
            transport
                .event_receiver_uri()
                .starts_with("http://127.0.0.1:"),
            "a loopback device routes through the loopback interface: {}",
            transport.event_receiver_uri()
        );

        let listener = transport.listener_addr;
        let poster = std::thread::spawn(move || post_event(listener, &response_event_document(77)));

        // The receive blocks until the listener thread has read, acked,
        // and queued the POSTed event.
        let event = transport
            .receive_event(Duration::from_secs(5))
            .expect("the queue never fails")
            .expect("the POSTed event resolves the wait");
        assert_eq!(
            event,
            IncomingEvent::Response(ResponseEvent {
                request_id: 77,
                return_code: 3,
                message: "Success".to_string(),
                response_data: None,
            })
        );

        let response = poster.join().expect("the posting thread finishes");
        assert!(
            response.starts_with("HTTP/1.1 200 OK\r\n"),
            "the listener answers 200: {response}"
        );
        assert!(
            response.ends_with(crate::soap::RESPONSE_EVENT_ACK),
            "the body is the canned ResponseEvent ack, byte for byte: {response}"
        );
    }

    #[test]
    fn the_listener_answers_each_event_kind_with_its_own_ack() {
        let transport =
            HttpSoapTransport::connect("127.0.0.1:8080".parse().expect("a socket addr"))
                .expect("the transport binds a loopback listener");
        let status_event = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <StatusEvent xmlns=\"http://sila.coop\"><state>busy</state></StatusEvent>\
             </s:Body></s:Envelope>";
        let response = post_event(transport.listener_addr, status_event);
        assert!(
            response.ends_with(STATUS_EVENT_ACK),
            "a StatusEvent draws the StatusEvent ack: {response}"
        );
        let event = transport
            .receive_event(Duration::from_secs(5))
            .expect("the queue never fails")
            .expect("the status event is queued");
        assert!(
            matches!(event, IncomingEvent::Status(_)),
            "the queued event is the status transition"
        );
    }
}
