//! The [`Absorbance96`] session: discovery and open, the mandatory
//! initialization sequence, the measurement engine with chunk reassembly,
//! status-based error decoding, and abort.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::report::{
    self, Abs96FirmwareError, AbsorbanceChunk, AbsorbanceTrigger, ReportDecodeError, RoutingTag,
    Status,
};
#[cfg(feature = "hid")]
use crate::transport::{ABSORBANCE_96_PRODUCT_ID, HidapiTransport};
use crate::transport::{HidError, HidTransport};

/// The deadlines and polling cadence of a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    /// Overall deadline for one command round-trip.
    pub command: Duration,
    /// One blocking read; the loop re-checks its deadline and the abort
    /// flag between reads, so this bounds abort latency.
    pub poll: Duration,
    /// Overall deadline for one measurement; a full plate read takes
    /// about 65 s.
    pub measurement: Duration,
}

impl Default for Timeouts {
    fn default() -> Timeouts {
        Timeouts {
            command: Duration::from_secs(30),
            poll: Duration::from_millis(250),
            measurement: Duration::from_secs(120),
        }
    }
}

/// The error raised by an [`Absorbance96`] session.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Absorbance96Error {
    #[error(transparent)]
    Hid(#[from] HidError),
    #[error(transparent)]
    Decode(#[from] ReportDecodeError),
    #[error(
        "this unit's installed wavelengths are {} nm; {requested} nm is not among them",
        format_nm_list(.installed)
    )]
    UnsupportedWavelength { requested: u16, installed: Vec<u16> },
    #[error("the firmware reported an error after the measurement: {0}")]
    Firmware(Abs96FirmwareError),
    #[error("the measurement was aborted")]
    Aborted,
    #[error(
        "the measurement did not complete within {seconds} s; a full-plate read finishes in about 65 s"
    )]
    MeasurementTimeout { seconds: u64 },
    #[error("report 0x{report_id:04X} received no reply within {seconds} s")]
    CommandTimeout { report_id: u16, seconds: u64 },
    #[error(
        "the firmware sent a result chunk with seq_len 0; a measurement carries at least one chunk"
    )]
    EmptySequence,
    #[error("the firmware changed the chunk count mid-measurement from {expected} to {found}")]
    InconsistentSequence { expected: u8, found: u8 },
    #[error("the firmware sent chunk {seq} of a {seq_len}-chunk sequence; chunks index from 0")]
    SequenceOutOfRange { seq: u8, seq_len: u8 },
}

fn format_nm_list(list: &[u16]) -> String {
    match list {
        [] => "none".to_string(),
        [one] => one.to_string(),
        [head @ .., last] => {
            let head = head
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{head} and {last}")
        }
    }
}

/// A cross-thread cancellation handle for an in-flight measurement.
///
/// The firmware answers an abort with silence — it simply stops emitting
/// result chunks — so cancellation is two-sided: the handle writes the
/// abort report naming the absorbance trigger and raises the flag the
/// chunk loop polls between reads.
#[derive(Clone)]
pub struct AbortHandle {
    flag: Arc<AtomicBool>,
    transport: Arc<dyn HidTransport>,
}

impl AbortHandle {
    /// Aborts the in-flight measurement, if any; the blocked
    /// [`Absorbance96::measure_absorbance`] call returns
    /// [`Absorbance96Error::Aborted`].
    pub fn abort(&self) -> Result<(), HidError> {
        self.flag.store(true, Ordering::SeqCst);
        self.transport
            .write_report(&report::abort(report::ABSORBANCE_TRIGGER))
    }
}

/// A Byonoy Absorbance 96 plate reader session.
///
/// Opening a session runs the device's mandatory initialization: a
/// reference measurement at 660 nm (which initializes the photodiode
/// reference — skipping it yields garbage data) followed by the
/// available-wavelengths query. Wavelengths are per-unit hardware: each
/// unit ships with up to six LEDs chosen at purchase, and
/// [`Absorbance96::measure_absorbance`] rejects wavelengths the unit does
/// not carry.
///
/// # Example
///
/// ```no_run
/// use lab_byonoy::Absorbance96;
///
/// # #[cfg(feature = "hid")]
/// # fn main() -> Result<(), lab_byonoy::Absorbance96Error> {
/// let mut reader = Absorbance96::open()?;
/// let plate = reader.measure_absorbance(600)?;
/// println!("A1 reads {} OD", plate.rows[0][0]);
/// # Ok(())
/// # }
/// # #[cfg(not(feature = "hid"))]
/// # fn main() {}
/// ```
pub struct Absorbance96 {
    transport: Arc<dyn HidTransport>,
    abort_requested: Arc<AtomicBool>,
    installed_wavelengths: Vec<u16>,
    last_chunk_flags: Vec<u8>,
    timeouts: Timeouts,
}

impl Absorbance96 {
    /// Opens the one connected Absorbance 96 and runs the initialization
    /// sequence. With several connected, use [`Absorbance96::open_serial`].
    #[cfg(feature = "hid")]
    pub fn open() -> Result<Absorbance96, Absorbance96Error> {
        let transport = HidapiTransport::open(ABSORBANCE_96_PRODUCT_ID, None)?;
        Absorbance96::with_transport(Arc::new(transport))
    }

    /// Opens the Absorbance 96 carrying the given serial number and runs
    /// the initialization sequence.
    #[cfg(feature = "hid")]
    pub fn open_serial(serial: &str) -> Result<Absorbance96, Absorbance96Error> {
        let transport = HidapiTransport::open(ABSORBANCE_96_PRODUCT_ID, Some(serial))?;
        Absorbance96::with_transport(Arc::new(transport))
    }

    /// Builds a session over any transport and runs the initialization
    /// sequence: the mandatory 660 nm reference measurement, then the
    /// installed-wavelengths query.
    pub fn with_transport(
        transport: Arc<dyn HidTransport>,
    ) -> Result<Absorbance96, Absorbance96Error> {
        Absorbance96::with_transport_and_timeouts(transport, Timeouts::default())
    }

    /// [`Absorbance96::with_transport`] with explicit deadlines.
    pub fn with_transport_and_timeouts(
        transport: Arc<dyn HidTransport>,
        timeouts: Timeouts,
    ) -> Result<Absorbance96, Absorbance96Error> {
        let mut session = Absorbance96 {
            transport,
            abort_requested: Arc::new(AtomicBool::new(false)),
            installed_wavelengths: Vec::new(),
            last_chunk_flags: Vec::new(),
            timeouts,
        };
        session.run_absorbance(660, 0, true)?;
        session.installed_wavelengths = session.query_installed_wavelengths()?;
        Ok(session)
    }

    /// The wavelengths this unit's LEDs cover, in nm.
    pub fn installed_wavelengths(&self) -> &[u16] {
        &self.installed_wavelengths
    }

    /// The per-chunk firmware flag bytes of the latest measurement, one
    /// per plate row. The flag bits are undocumented; non-zero values are
    /// surfaced here for diagnostics, never failed on.
    pub fn last_chunk_flags(&self) -> &[u8] {
        &self.last_chunk_flags
    }

    /// A handle that cancels an in-flight measurement from another thread.
    pub fn abort_handle(&self) -> AbortHandle {
        AbortHandle {
            flag: Arc::clone(&self.abort_requested),
            transport: Arc::clone(&self.transport),
        }
    }

    /// The device status.
    pub fn status(&self) -> Result<Status, Absorbance96Error> {
        let reply = self.command_round_trip(&report::status_request(), report::STATUS)?;
        Ok(Status::decode(&reply)?)
    }

    fn query_installed_wavelengths(&self) -> Result<Vec<u16>, Absorbance96Error> {
        let reply = self.command_round_trip(
            &report::available_wavelengths_request(),
            report::AVAILABLE_WAVELENGTHS,
        )?;
        Ok(report::decode_available_wavelengths(&reply)?
            .into_iter()
            .filter_map(|nm| u16::try_from(nm).ok())
            .collect())
    }

    /// Writes a request and reads until the reply with the matching report
    /// id arrives; unrelated packets are skipped.
    fn command_round_trip(
        &self,
        request: &report::Packet,
        expected_id: u16,
    ) -> Result<report::Packet, Absorbance96Error> {
        self.transport.write_report(request)?;
        let deadline = Instant::now() + self.timeouts.command;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(Absorbance96Error::CommandTimeout {
                    report_id: expected_id,
                    seconds: self.timeouts.command.as_secs(),
                });
            }
            let wait = self
                .timeouts
                .poll
                .min(deadline.saturating_duration_since(now));
            match self.transport.read_report(wait)? {
                Some(packet) if report::report_id(&packet) == expected_id => return Ok(packet),
                _ => continue,
            }
        }
    }

    /// Runs one absorbance measurement: preamble, trigger, chunk
    /// reassembly, and the authoritative post-measurement status gate.
    /// Returns one row of twelve values per chunk, rows A to H.
    fn run_absorbance(
        &mut self,
        signal_wavelength_nm: i16,
        reference_wavelength_nm: i16,
        is_reference: bool,
    ) -> Result<Vec<[f32; 12]>, Absorbance96Error> {
        self.abort_requested.store(false, Ordering::SeqCst);

        // The measurement preamble, fire-and-forget with the legacy tag,
        // matching observed vendor-app traffic.
        self.transport
            .write_report(&report::supported_reports_request(RoutingTag::Legacy))?;
        self.transport.write_report(&report::device_data_request(
            report::FIELD_MEASUREMENT_PREAMBLE,
            RoutingTag::Legacy,
        ))?;
        self.transport
            .write_report(&report::absorbance_trigger(&AbsorbanceTrigger {
                signal_wavelength_nm,
                reference_wavelength_nm,
                is_reference,
            }))?;

        let deadline = Instant::now() + self.timeouts.measurement;
        // Chunks are indexed into their `seq` slot, never appended, so a
        // reordered packet still lands in the right plate row.
        let mut rows: Vec<Option<[f32; 12]>> = Vec::new();
        let mut chunk_flags: Vec<u8> = Vec::new();
        loop {
            if self.abort_requested.load(Ordering::SeqCst) {
                return Err(Absorbance96Error::Aborted);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Absorbance96Error::MeasurementTimeout {
                    seconds: self.timeouts.measurement.as_secs(),
                });
            }
            let wait = self
                .timeouts
                .poll
                .min(deadline.saturating_duration_since(now));
            let Some(packet) = self.transport.read_report(wait)? else {
                continue;
            };
            if report::report_id(&packet) != report::ABSORBANCE_CHUNK {
                continue;
            }
            let chunk = AbsorbanceChunk::decode(&packet)?;
            if chunk.seq_len == 0 {
                return Err(Absorbance96Error::EmptySequence);
            }
            if rows.is_empty() {
                rows = vec![None; usize::from(chunk.seq_len)];
                chunk_flags = vec![0u8; usize::from(chunk.seq_len)];
            } else if rows.len() != usize::from(chunk.seq_len) {
                return Err(Absorbance96Error::InconsistentSequence {
                    expected: u8::try_from(rows.len())
                        .expect("the chunk count came from a u8 seq_len"),
                    found: chunk.seq_len,
                });
            }
            if chunk.seq >= chunk.seq_len {
                return Err(Absorbance96Error::SequenceOutOfRange {
                    seq: chunk.seq,
                    seq_len: chunk.seq_len,
                });
            }
            rows[usize::from(chunk.seq)] = Some(chunk.values);
            chunk_flags[usize::from(chunk.seq)] = chunk.flags;
            if rows.iter().all(Option::is_some) {
                break;
            }
        }

        // The post-measurement status check is the authoritative error
        // gate: chunk delivery alone does not mean the data is good.
        let status = self.status()?;
        if let Some(error) = Abs96FirmwareError::from_code(status.error_code) {
            return Err(Absorbance96Error::Firmware(error));
        }
        self.last_chunk_flags = chunk_flags;
        Ok(rows
            .into_iter()
            .map(|row| row.expect("the loop exits only with every sequence slot filled"))
            .collect())
    }
}

/// One completed absorbance measurement: every well, in device units.
///
/// The device always reads the whole plate; rows run A to H, each row
/// A1 to A12, and values are unitless optical density.
#[derive(Clone, Debug, PartialEq)]
pub struct AbsorbanceMeasurement {
    pub wavelength_nm: u16,
    pub rows: Vec<[f32; 12]>,
}

impl Absorbance96 {
    /// Measures the whole plate at one installed wavelength.
    pub fn measure_absorbance(
        &mut self,
        wavelength_nm: u16,
    ) -> Result<AbsorbanceMeasurement, Absorbance96Error> {
        if !self.installed_wavelengths.contains(&wavelength_nm) {
            return Err(Absorbance96Error::UnsupportedWavelength {
                requested: wavelength_nm,
                installed: self.installed_wavelengths.clone(),
            });
        }
        let signal = i16::try_from(wavelength_nm)
            .expect("installed wavelengths decode from i16, so a member fits");
        let rows = self.run_absorbance(signal, 0, false)?;
        Ok(AbsorbanceMeasurement {
            wavelength_nm,
            rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::SlotState;
    use crate::transport::MockHidTransport;
    use std::sync::atomic::AtomicU8;

    /// Frames a device-to-host packet: report id, payload, zero routing.
    fn inbound(report_id: u16, payload: &[u8]) -> report::Packet {
        let mut packet = [0u8; 64];
        packet[0..2].copy_from_slice(&report_id.to_le_bytes());
        packet[2..2 + payload.len()].copy_from_slice(payload);
        packet
    }

    fn status_packet(error_code: u8, slot_state: u8) -> report::Packet {
        let mut payload = [0u8; 9];
        payload[0] = 1; // initialized
        payload[1] = slot_state;
        payload[2] = error_code;
        payload[8] = 1; // boot completed
        inbound(report::STATUS, &payload)
    }

    fn wavelengths_packet(installed: &[i16]) -> report::Packet {
        let mut payload = [0u8; 60];
        for (slot, nm) in installed.iter().enumerate() {
            payload[slot * 2..slot * 2 + 2].copy_from_slice(&nm.to_le_bytes());
        }
        inbound(report::AVAILABLE_WAVELENGTHS, &payload)
    }

    /// The scripted plate: well (row, column) reads `row + column / 100`.
    fn row_values(row: u8) -> [f32; 12] {
        let mut values = [0f32; 12];
        for (column, value) in values.iter_mut().enumerate() {
            *value = f32::from(row) + column as f32 / 100.0;
        }
        values
    }

    fn chunk_packet(seq: u8, seq_len: u8, signal_nm: i16, values: &[f32; 12]) -> report::Packet {
        let mut payload = [0u8; 60];
        payload[0] = seq;
        payload[1] = seq_len;
        payload[2..4].copy_from_slice(&signal_nm.to_le_bytes());
        payload[6..10].copy_from_slice(&65_000u32.to_le_bytes());
        for (column, value) in values.iter().enumerate() {
            payload[10 + column * 4..14 + column * 4].copy_from_slice(&value.to_le_bytes());
        }
        payload[59] = 100; // progress
        inbound(report::ABSORBANCE_CHUNK, &payload)
    }

    /// Plays the firmware's side of a session: chunk replies to triggers
    /// (in the given sequence order), status and wavelength replies to
    /// their queries. The shared cells let a test change the firmware's
    /// error code or slot state after setup.
    fn script_device(
        mock: &MockHidTransport,
        installed: Vec<i16>,
        chunk_order: Vec<u8>,
        error_code: Arc<AtomicU8>,
        slot_state: Arc<AtomicU8>,
    ) {
        mock.set_responder(move |packet| match report::report_id(packet) {
            report::ABSORBANCE_TRIGGER => {
                let signal = i16::from_le_bytes([packet[2], packet[3]]);
                chunk_order
                    .iter()
                    .map(|&seq| chunk_packet(seq, 8, signal, &row_values(seq)))
                    .collect()
            }
            report::STATUS => vec![status_packet(
                error_code.load(Ordering::SeqCst),
                slot_state.load(Ordering::SeqCst),
            )],
            report::AVAILABLE_WAVELENGTHS => vec![wavelengths_packet(&installed)],
            _ => Vec::new(),
        });
    }

    fn ready_session(
        chunk_order: Vec<u8>,
    ) -> (
        Arc<MockHidTransport>,
        Absorbance96,
        Arc<AtomicU8>,
        Arc<AtomicU8>,
    ) {
        let mock = Arc::new(MockHidTransport::new());
        let error_code = Arc::new(AtomicU8::new(0));
        let slot_state = Arc::new(AtomicU8::new(2));
        script_device(
            &mock,
            vec![450, 600, 660],
            chunk_order,
            Arc::clone(&error_code),
            Arc::clone(&slot_state),
        );
        let session = Absorbance96::with_transport(Arc::clone(&mock) as Arc<dyn HidTransport>)
            .expect("the scripted device completes the initialization sequence");
        (mock, session, error_code, slot_state)
    }

    fn triggers(mock: &MockHidTransport) -> Vec<report::Packet> {
        mock.written()
            .into_iter()
            .filter(|packet| report::report_id(packet) == report::ABSORBANCE_TRIGGER)
            .collect()
    }

    #[test]
    fn setup_writes_the_mandatory_660_nm_reference_as_its_first_trigger() {
        let (mock, session, _, _) = ready_session((0..8).collect());
        let written = mock.written();
        assert_eq!(
            report::report_id(&written[0]),
            report::SUPPORTED_REPORTS,
            "the preamble leads the measurement"
        );
        assert_eq!(
            written[0][62..64],
            [0x00, 0x00],
            "the preamble uses the legacy routing tag"
        );
        assert_eq!(
            report::report_id(&written[1]),
            report::DEVICE_DATA,
            "field 7 is read fire-and-forget as the second preamble step"
        );
        let first_trigger = triggers(&mock)[0];
        assert_eq!(
            first_trigger,
            report::absorbance_trigger(&AbsorbanceTrigger {
                signal_wavelength_nm: 660,
                reference_wavelength_nm: 0,
                is_reference: true,
            }),
            "the first trigger is the 660 nm reference that initializes the photodiodes"
        );
        assert_eq!(
            session.installed_wavelengths(),
            &[450, 600, 660],
            "setup stores the installed LED list"
        );
    }

    #[test]
    fn a_full_measurement_assembles_eight_chunks_into_ordered_rows() {
        let (_, mut session, _, _) = ready_session((0..8).collect());
        let plate = session
            .measure_absorbance(600)
            .expect("eight chunks complete a plate");
        assert_eq!(plate.wavelength_nm, 600);
        assert_eq!(plate.rows.len(), 8);
        assert_eq!(plate.rows[0][0], 0.0, "A1 leads the plate");
        assert_eq!(
            plate.rows[2][5],
            2.0f32 + 5.0 / 100.0,
            "C6 sits at row 2, column 5 in row-major order"
        );
        assert_eq!(
            plate.rows[7][11],
            7.0f32 + 11.0 / 100.0,
            "H12 ends the plate"
        );
    }

    #[test]
    fn reordered_chunks_land_in_their_sequence_slots() {
        let (_, mut session, _, _) = ready_session(vec![5, 0, 3, 1, 7, 2, 6, 4]);
        let plate = session
            .measure_absorbance(450)
            .expect("every slot fills regardless of arrival order");
        for row in 0..8 {
            assert_eq!(
                plate.rows[row][0], row as f32,
                "row {row} carries its own values even when its chunk arrived out of order"
            );
        }
    }

    #[test]
    fn a_firmware_error_after_the_measurement_fails_with_its_meaning() {
        let (_, mut session, error_code, _) = ready_session((0..8).collect());
        error_code.store(2, Ordering::SeqCst);
        let error = session
            .measure_absorbance(600)
            .expect_err("the status gate rejects the measurement");
        assert_eq!(
            error,
            Absorbance96Error::Firmware(Abs96FirmwareError::AmbientLight)
        );
        assert!(
            error.to_string().contains("ambient light"),
            "the message carries the decoded meaning, got: {error}"
        );
    }

    #[test]
    fn an_unknown_firmware_code_renders_as_the_hex_sentinel() {
        let (_, mut session, error_code, _) = ready_session((0..8).collect());
        error_code.store(0x2A, Ordering::SeqCst);
        let error = session
            .measure_absorbance(600)
            .expect_err("a non-zero code fails the measurement");
        assert!(
            error.to_string().contains("errorCode=0x2A"),
            "undocumented codes surface verbatim, got: {error}"
        );
    }

    #[test]
    fn an_uninstalled_wavelength_is_rejected_naming_the_installed_set() {
        let (mock, mut session, _, _) = ready_session((0..8).collect());
        let error = session
            .measure_absorbance(405)
            .expect_err("no 405 nm LED is installed");
        assert_eq!(
            error.to_string(),
            "this unit's installed wavelengths are 450, 600 and 660 nm; 405 nm is not among them"
        );
        assert_eq!(
            triggers(&mock).len(),
            1,
            "only the setup reference was triggered; the rejected read never reached the device"
        );
    }

    #[test]
    fn the_status_reports_every_documented_slot_state() {
        let (_, session, _, slot_state) = ready_session((0..8).collect());
        assert_eq!(
            session.status().expect("status readable").slot_state,
            SlotState::Occupied,
            "OCCUPIED is a seated plate"
        );
        slot_state.store(1, Ordering::SeqCst);
        assert_eq!(
            session.status().expect("status readable").slot_state,
            SlotState::Empty
        );
        slot_state.store(0, Ordering::SeqCst);
        assert_eq!(
            session.status().expect("status readable").slot_state,
            SlotState::Unknown
        );
        slot_state.store(3, Ordering::SeqCst);
        assert_eq!(
            session.status().expect("status readable").slot_state,
            SlotState::Undetermined
        );
    }

    #[test]
    fn an_abort_mid_measurement_stops_the_read_and_names_the_trigger() {
        let (mock, mut session, _, _) = ready_session((0..8).collect());
        // The firmware's side after setup: three chunks and then silence,
        // as if the measurement were still running.
        mock.set_responder(move |packet| match report::report_id(packet) {
            report::ABSORBANCE_TRIGGER => {
                let signal = i16::from_le_bytes([packet[2], packet[3]]);
                (0..3)
                    .map(|seq| chunk_packet(seq, 8, signal, &row_values(seq)))
                    .collect()
            }
            _ => Vec::new(),
        });
        let handle = session.abort_handle();
        let aborter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            handle.abort().expect("the abort report writes to the mock");
        });
        let error = session
            .measure_absorbance(600)
            .expect_err("the read ends with an abort, not a full plate");
        assert_eq!(error, Absorbance96Error::Aborted);
        aborter.join().expect("the aborting thread finishes");
        let abort_packets: Vec<_> = mock
            .written()
            .into_iter()
            .filter(|packet| report::report_id(packet) == report::ABORT)
            .collect();
        assert_eq!(
            abort_packets,
            vec![report::abort(report::ABSORBANCE_TRIGGER)],
            "the abort report names the absorbance trigger"
        );
    }

    #[test]
    fn a_seq_len_change_mid_stream_is_rejected() {
        let mock = Arc::new(MockHidTransport::new());
        let error_code = Arc::new(AtomicU8::new(0));
        let slot_state = Arc::new(AtomicU8::new(2));
        script_device(
            &mock,
            vec![660],
            (0..8).collect(),
            Arc::clone(&error_code),
            Arc::clone(&slot_state),
        );
        let mut session = Absorbance96::with_transport(Arc::clone(&mock) as Arc<dyn HidTransport>)
            .expect("setup succeeds against the well-behaved script");
        mock.set_responder(move |packet| match report::report_id(packet) {
            report::ABSORBANCE_TRIGGER => vec![
                chunk_packet(0, 8, 660, &row_values(0)),
                chunk_packet(1, 4, 660, &row_values(1)),
            ],
            _ => Vec::new(),
        });
        let error = session
            .measure_absorbance(660)
            .expect_err("a shrinking sequence is a firmware fault");
        assert_eq!(
            error,
            Absorbance96Error::InconsistentSequence {
                expected: 8,
                found: 4,
            }
        );
    }

    #[test]
    fn non_zero_chunk_flags_surface_without_failing_the_measurement() {
        let (mock, mut session, _, _) = ready_session((0..8).collect());
        mock.set_responder(move |packet| match report::report_id(packet) {
            report::ABSORBANCE_TRIGGER => (0..8)
                .map(|seq| {
                    let mut chunk = chunk_packet(seq, 8, 600, &row_values(seq));
                    if seq == 3 {
                        chunk[60] = 0x08; // the undocumented flags byte
                    }
                    chunk
                })
                .collect(),
            report::STATUS => vec![status_packet(0, 2)],
            _ => Vec::new(),
        });
        session
            .measure_absorbance(600)
            .expect("undocumented flags never fail a measurement");
        assert_eq!(
            session.last_chunk_flags(),
            &[0, 0, 0, 0x08, 0, 0, 0, 0],
            "the flagged row is surfaced for diagnostics"
        );
    }

    #[test]
    fn the_wavelength_list_formats_as_a_sentence() {
        assert_eq!(format_nm_list(&[]), "none");
        assert_eq!(format_nm_list(&[600]), "600");
        assert_eq!(format_nm_list(&[450, 600]), "450 and 600");
        assert_eq!(format_nm_list(&[450, 600, 660]), "450, 600 and 660");
    }
}
