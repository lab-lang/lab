# lab-byonoy

A typed Rust driver for the Byonoy Absorbance 96 plate reader, speaking
its raw USB HID protocol. There is no vendor library anywhere in the
stack: the wire protocol — 64-byte little-endian reports with routing
tags — is implemented directly over the OS HID layer.

## Layers

- **Report codec** (`report`) — pure, no I/O. Typed encode and decode of
  every report: triggers, queries, result chunks, abort, the LED bar.
  Every report's wire form is testable as bytes with no hardware; golden
  tests pin the encoders byte for byte, including the routing tags the
  firmware silently drops reports without. The Luminescence 96's reports
  (trigger `0x0340`, chunks `0x0600`) are covered at this layer only.
- **Transport** (`transport`) — a blocking `HidTransport` trait over
  64-byte packets, with a `hidapi`-backed implementation behind the `hid`
  cargo feature (default on; disable default features to encode and
  decode without linking the native HID library) and a scripted
  `MockHidTransport` for tests.
- **Session** (`session`) — an `Absorbance96` handle owning the
  transport: enumeration with serial-number disambiguation, open by
  platform path, the mandatory 660 nm reference measurement that
  initializes the photodiode reference, per-unit wavelength validation,
  the chunk-reassembling measurement engine (chunks are indexed into
  their sequence slot, so reordered packets still assemble correctly),
  the authoritative post-measurement status gate with typed firmware
  errors, and cross-thread abort. It implements
  `lab_instruments::PlateReader`.

## The replug caveat

HID access is exclusive per process on every operating system. A crashed
session can leave the device claimed until the USB cable is physically
replugged; the open-failure error says so.

## Hardware bring-up checklist

CI has no hardware; `tests/hardware.rs` is `#[ignore]`d and additionally
gated on an environment variable. With one reader plugged in:

1. `LAB_BYONOY_HARDWARE=1 cargo test -p lab-byonoy --test hardware -- --ignored --nocapture`
2. Confirm the open succeeds — it runs the reference measurement, so the
   slot should hold no sample plate.
3. Confirm the printed wavelength list matches the LEDs the unit shipped
   with.
4. Confirm slot sensing tracks a plate being seated and removed.
5. The full-plate read takes about 65 s; confirm A1 and H12 print
   plausible OD values.

## Provenance

The Byonoy HID protocol has no public specification. The knowledge
encoded here derives from
[PyLabRobot](https://github.com/PyLabRobot/pylabrobot)'s Byonoy
implementation (MIT License, Copyright (c) 2022 PyLabRobot), the de-facto
public specification. PyLabRobot is reference material only, not a
dependency.
