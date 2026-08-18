# 0036 — Photoreal renderers are players of the scene and trace

## Status

Accepted.

## Context

The simulation's first renderer drew schematic boxes, which serves the
scientist checking a run but not the designer of a new facility, who
needs to see the lab that does not exist yet, nor the robot-learning
work that needs scan-accurate environments. Photorealism could have been
built as its own pipeline with its own model of the experiment; that
shape invites the drift 0034 exists to prevent.

## Decision

Every renderer is a player of the same two documents, `scene.json` and
`sim-trace.json`, and computes nothing about the run: the web player for
daily use, the USD stage for Omniverse and Isaac Sim, and a headless
Blender harness (`lab render`) for batch photographic output. Realism is
layered onto the documents, never forked from them:

- **Assets are references, never embedded.** A facility's `assets/`
  directory maps identity keys (station kinds, labware catalog ids,
  `room`) to real meshes; `lab scene` bundles referenced files beside
  the scene and every consumer falls back to the dimensioned box when an
  asset is missing or fails. Vendor CAD is licensed to the facility's
  owner, so assets live with the facility and never in this repository.
- **The facility authors the space.** `[room]` and per-station
  positions and rotations in `facility.toml` are the floor plan; the kit
  room renders when no environment asset exists.
- **USD carries the animation.** One timecode per simulated second,
  written by `lab scene --animated`; Omniverse and usdview play the run
  with no integration code on either side.
- **The schematic tier stays first-class.** Without a facility, every
  command renders exactly the schematic scene; photorealism is a
  projection, not a replacement.

Environments may later be scans: a Gaussian-splat or mesh capture of a
real room composes with authored stations as the `room` environment
asset. That layer is also where robot-learning environments attach; it
changes no document format.

## Consequences

A biofoundry design review is `facility.toml` plus three commands, and
its fidelity grows file by file as assets arrive, with nothing blocking
on any of them. The renderers cannot disagree with the simulation or
with each other about what happened, only about how it looks. The cost
is honest too: the movie tier depends on a local Blender the toolchain
finds but never bundles, and asset preparation is a documented human
workflow (docs/integrations/photoreal-assets.md), not a build step.
