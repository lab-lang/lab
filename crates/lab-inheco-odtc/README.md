# lab-inheco-odtc

A typed Rust driver for the Inheco ODTC (On-Deck Thermal Cycler),
speaking its SiLA 1.x SOAP protocol over plain HTTP. No vendor library is
involved: control is HTTP and XML end to end.

## Layers

- **Protocol** (`soap`, `methodset`) — pure, no I/O. SOAP 1.1 doc/literal
  envelope encoding and decoding for the command vocabulary (`Reset`,
  `Initialize`, `GetStatus`, `GetDeviceIdentification`, `OpenDoor`,
  `CloseDoor`, `SetParameters`, `ExecuteMethod`, `StopMethod`,
  `ReadActualTemperature`), the response and return-code model,
  device-event parsing with the canned acknowledgements, and a validated
  MethodSet builder rendering a `ThermalProgram` — this crate's own
  staged thermal-program type — into the vendor's XML dialect. Timestamps and method names are
  caller inputs — nothing in the protocol layer reads a clock — so every
  envelope and method document is pinned byte for byte by golden tests.
- **Transport** (`transport`) — a blocking `SoapTransport` trait carrying
  both directions of the asymmetric protocol: `ureq` for the
  client-to-device POSTs and a hand-rolled HTTP/1.1 listener on a
  dedicated thread for the device-to-client event POSTs, plus a scripted
  `MockSoapTransport` for tests.
- **Session** (`session`) — an `Odtc` handle owning the transport: the
  connect handshake (`Reset` with the callback URI, `Initialize`, poll to
  idle), door control, method upload and execution, temperature readout,
  and stop, all in the device's own vocabulary (`start_method`,
  `await_method`, `ActualTemperatures` with the Mount sensor named as
  the vendor names it). The device reports no run progress; nothing here
  pretends otherwise. This crate knows nothing outside the instrument it
  drives.

## The callback obligation, and the polling fallback

The ODTC answers most commands with return code 2 — "asynchronous
command accepted" — and completes them later by POSTing a
`ResponseEvent` to an HTTP URI the client registers with `Reset`. The
client is therefore also a server: this crate binds a listener on an
ephemeral port, and the listener must answer every incoming
`ResponseEvent`, `StatusEvent`, and `DataEvent` promptly with the canned
success reply, or the device stalls. `StatusEvent`s (state transitions)
are acknowledged and observed; `DataEvent`s carry live temperature
series during runs and are kept as optional telemetry.

Completion never depends on the callback alone: while waiting, the
session polls `GetStatus` at the configured interval, and a settled
state (`idle` or `standby`) resolves the wait even if no event ever
arrives — so a firewall dropping inbound connections degrades a run to
polling instead of wedging it. `ReadActualTemperature` is the exception:
its data travels only in the `ResponseEvent`, so a blocked callback
channel turns temperature reads into a typed error naming the listener
URI to check.

## Link-local addressing

The ODTC commonly lives on a link-local 169.254/16 network. The callback
URI must name the local interface that actually routes to the device — a
loopback or wrong-interface URI means callbacks never arrive. The
transport discovers that interface by "connecting" a UDP socket toward
the device address and reading the local address back; no packets flow.

## Bring-up: pin the WSDL

The device serves its own contract at `http://<ip>/odtc.wsdl`, including
the full vendor return-code table. Bring-up against a real unit should
fetch and pin that document; the codes this crate treats specially — 1
(synchronous success), 2 (asynchronous accept), 3 (asynchronous
success), 12 (success with warning) — are the empirically confirmed set,
and everything else surfaces as a typed failure carrying the device's
message.

## Validation

Profiles validate against the ODTC envelope before anything is uploaded:
block 4–99 °C, lid 30–115 °C, ramps at most 4.4 °C/s (unset ramps render
as the device maximum). Beyond the generic rules, the MethodSet builder
rejects profiles holding sub-ambient plateaus (below ~20 °C) for more
than two hours in total, the condensation limit.

## Provenance

The protocol knowledge encoded here derives from
[PyLabRobot](https://github.com/PyLabRobot/pylabrobot)'s ODTC backend and
Inheco SiLA interface (MIT License, Copyright (c) 2022 PyLabRobot) and
from the official Inheco ODTC user manual (document 900584). PyLabRobot
is reference material only, not a dependency.
