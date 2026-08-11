# 0034 — The simulator is an interpreter, not a backend

## Status

Accepted.

## Context

A run document is already interpreted two ways: `lab run` executes it
against hardware, and `lab run --dry-run` validates and narrates it without
hardware. Simulation adds a third need: given the same documents, predict
how the work unfolds in time — total duration, when an operator must be
present, when they can walk away — and record the physical state changes a
visualization can play back.

Two architectures could provide this. A simulation *backend* would compile
the checked program into its own execution model; a simulation
*interpreter* consumes the run documents the real runner consumes. The
backend shape invites drift: two lowerings of the same experiment can
disagree, and the simulation stops being evidence about the artifact the
operator will actually approve.

## Decision

`lab simulate` is a third interpreter of the emitted run documents. It
loads exactly what `lab run` loads, executes the same node walk against
simulated stations and a virtual clock, and adds nothing but time and
recorded state. It emits no run documents and compiles nothing, so it is
not a backend and puts no pressure on the closed backend dispatch of 0029.

The run-document schemas move to their own crate, `lab-runfmt`, shared by
the emitters in `lab-compiler` and every interpreter. Loaders there check
each document's `format` string once, in one place.

The simulator's output is a trace document, `lab.sim-trace.v0`: virtual
timestamps, node lifecycle, labware movements, instrument state, and the
intervals that require an operator. The trace is the contract for all
visualization; a viewer plays traces and computes nothing. Durations come
from a stated model — thermal profiles are computed exactly from the
document, robot steps and human steps are estimates the model labels as
such — never from hidden assumptions inside a renderer.

## Consequences

Simulation and execution cannot disagree about what the experiment *is*,
only about how long it takes; the timing model is data, so measured run
ledgers can calibrate it later. Any new run-document format gains
simulation by gaining an interpreter arm, not a parallel model. The trace
format is versioned like the run formats: a change to what an event means
is a new version, not an edit.
