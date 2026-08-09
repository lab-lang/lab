# lab-hamilton-star

A typed Rust implementation of the Hamilton STAR/STARlet liquid-handling
robot's firmware command protocol and its USB transport.

## Layers

- **Protocol** (`framing`, `response`, `errors`, `commands`, `units`,
  `catalog`) — pure, no I/O. Typed command construction and response
  parsing for the STAR's ASCII firmware protocol: fixed-width parameter
  encoding, per-channel list encoding with the firmware's don't-care rules,
  schema-driven response parsing, and the complete firmware error and trace
  tables as typed errors. Every command's wire form is testable as a string
  with no hardware; golden tests pin the encoders byte for byte to wire
  strings verified against real machines.
- **Transport** (`transport`) — a blocking, message-oriented `Transport`
  trait with two implementations: the exact USB bulk-transfer discipline
  (zero-length-packet termination, short-packet message boundaries, connect
  drain) over `rusb`, and a scripted mock for tests.
- **Session** (`session`) — a `Star` handle owning the transport: a
  background reader thread correlating replies by command id, the
  per-module concurrency locks the firmware requires (violations answer
  trace 40), per-command read timeouts, typed firmware-error decoding with
  the automatic `VP` faulty-parameter follow-up, a session-scoped tip-type
  cache, and the documented setup choreography. The public API is
  synchronous.

## Coverage

Fully implemented (typed commands, session methods, tests): the
instrument/system commands, the 8-channel pipetting core (initialize,
tip-type definition, tip pickup/discard, aspirate, dispense, Z-safety
retract, movement and query commands, LLD probes), tip-type table
management, and complete error decoding.

Encode-only (typed command structs with golden wire tests, no session
choreography): iSWAP, CoRe 96 head, autoload/carriers, CoRe gripper, pumps
and heater-shakers.

## Safety posture

These commands move a heavy, fast, expensive machine. Every motion
parameter is either explicit or a named documented default — nothing is
guessed silently. Constructors reject out-of-range values with errors
naming the parameter, its unit, and its permitted range. Where the
firmware's real behavior diverges from its documentation (the 334.7 mm
channel Z ceiling, the broken `H0 DL` dispensing-drive home, the four-digit
`kf` EEPROM read width), the crate encodes the real behavior and documents
why.

## Provenance

The Hamilton STAR firmware protocol has no public specification. The
knowledge encoded here derives from
[PyLabRobot](https://github.com/PyLabRobot/pylabrobot)'s Hamilton STAR
implementation (MIT License, Copyright (c) 2022 PyLabRobot), the de-facto
public specification, whose test suite provides the verified wire strings
this crate's golden tests reproduce. PyLabRobot is reference material only,
not a dependency.
