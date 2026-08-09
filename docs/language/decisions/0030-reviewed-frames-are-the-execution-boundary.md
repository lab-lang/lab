# 0030 — Reviewed frames are the execution boundary

## Status

Accepted.

## Context

The Hamilton STAR is the toolchain's first execution target without an
offline protocol format. The Opentrons backends emit files another vendor's
software validates and runs; the STAR runs a live USB firmware session, and
its "protocol" is the sequence of ASCII command frames the machine is sent.
A backend for it must answer what `lab build` produces, and what stands
between the produced thing and a moving machine.

The driver crate `hamilton-star` already separates the pure protocol
(typed, golden-tested frame encoders) from the transport and session. The
compiler consumes only the pure layer.

## Decision

`lab build --target <star bench>` emits `lab.star-run.v0` documents: per
stage, an ordered list of id-less firmware frames built by the driver
crate's validated encoders, each carrying an operator-facing description,
with the manual steps (thermal work the base machine cannot do) interleaved
between run documents. The document is the review boundary: `lab run`
replays the frames verbatim, adding only the command ids the session
protocol requires. Nothing is planned, derived, or decided at run time —
what the reviewer read is what the machine receives.

Planning is deterministic to make that review meaningful. Deck coordinates
come from a vendored, attributed catalog of Hamilton carriers and labware;
liquid heights come from per-well volume tracking over measured
volume-to-height models, with the safety margins stated as named constants;
liquid-class corrected volumes come from the driver crate's water tables.
Capacitive level detection is a per-bench opt-in that adds a runtime check
on top of the planned heights, never a substitute for them.

`lab run` is the toolchain's first hardware-touching command, so its safety
posture is part of this decision: a dry run that validates and prints every
frame without hardware, an explicit confirmation before any motion, an
operator confirmation at every manual step, and on any firmware error a
Z-safety retract followed by an abort naming the failed step — no automatic
retry, no resume.

The backend lives at `backend/hamilton/star`, the second vendor family
under the dispatch rule of 0029; profiles declare `backend =
"hamilton.star"`.

## Consequences

A protocol reviewer reads real firmware frames, and golden tests pin them
byte for byte — the same discipline the driver crate applies to its
encoders. The run format is versioned like a wire format: a change to what
frames mean is a new format version, not an edit. The compiler builds
everywhere without libusb (the driver crate's `usb` feature stays off in
the workspace dependency and on in the CLI), and executing a plan requires
passing through the emitted, reviewable document — there is no
compile-and-run path that skips the artifact.
