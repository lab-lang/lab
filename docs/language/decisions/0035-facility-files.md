# 0035 — A facility is its own file, pointed to by the manifest

## Status

Accepted, partially implemented.

## Context

Simulation raises a question compilation never had to answer: what lab is
this experiment running in? A workcell target profile describes one bench;
the facility question is wider — which stations exist at all, what stock
sits in which fridge, what consumables are on the shelf, and how labware
travels between benches. Designing a new lab or a biofoundry is answering
exactly that question, so the answer needs to be a reviewable artifact.

The relationship between packages and facilities is many-to-many: one
package is simulated against several candidate facilities to compare them,
and one facility serves every package that runs in that lab. The two also
change on different cadences — a manifest versions with the experiment; a
facility changes when the lab buys a freezer, and its stock changes daily.

## Decision

A facility is described in its own TOML file under `facilities/`,
validated by the runtime: named stations in the same vocabulary workcell
profiles use, storage units with the stock they hold, consumables, and a
transport section whose `walk_seconds` states how long a human handoff
takes in this facility. `lab simulate --facility` checks that every
station a plan needs exists there by name and kind, and drives handoff
durations from the facility's transport time.

A single-facility package keeps the description at its root as
`facility.toml`, where the simulation commands find it by convention.
Packages comparing several candidate facilities keep them under
`facilities/`, selected by `--facility` or by the manifest's pointer:
`[build] facility = "main-bench"`, a bare name held to the same
no-path-escape rule as `[build] target`. The description never lives in
`lab.toml` itself, and station addresses stay runtime input either way.

Stock is inventory state, not a declaration (0026's companion rule):
storage units name materials and artifacts by their declared identities.
A build drawing on facility stock narrows it to the dependency graph's
demands first (`BuildInventory::restricted_to`), because resolution
deliberately rejects surplus stock in a package manifest and a facility
legitimately stocks a whole lab.

## Consequences

Comparing lab designs is running the same simulation twice with different
`--facility` files and reading two summaries. The facility file is the
seed for everything spatial that follows: room positions for stations,
scene generation, and the environment an arm policy trains in all attach
here, without touching packages or profiles. Wiring facility stock into
`lab build` dependency resolution remains open until the build command
grows a `--facility` flag of its own.
