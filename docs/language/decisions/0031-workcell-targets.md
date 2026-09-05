# 0031 — A workcell target composes stations; assignment is planning, not language

## Status

Superseded by [0044: Facility graphs and capability binding replace workcell targets](0044-facility-graphs-replace-workcell-targets.md).

## Context

Every target so far is one machine plus an implicit human: whatever the
machine cannot do falls across the "execution boundary" into operator
prose. Real builds span several instruments — a liquid handler, a
thermocycler beside it, a plate reader — with a person carrying labware
between them. The STAR backend made the gap concrete: its assembly stage
ends with "thermocycle off-deck by hand" even on benches that own a
perfectly good cycler.

Two ways to model multiple machines were open. The language could name
devices — a protocol op stating where it runs — or the profile and planner
could own the split, leaving programs portable across benches.

## Decision

Protocol LAIR stays device-neutral; multiplicity enters below the language,
in three layers.

A **workcell** is a target whose profile declares stations: exactly one
liquid-handler station naming its own single-machine target profile, plus
instrument stations (`inheco.odtc`, `byonoy.absorbance96`) carrying only
bench properties, with `transport.between = "human"`. A station kind fixes
its capabilities as compile-time constants; assignment is deterministic
over kinds, not negotiated. The human is a station every workcell has.

Planning composes rather than replaces: the liquid handler's own planner
runs unchanged, and the workcell planner owns only the split. Each run's
thermal work is planned as structured data (a device-neutral thermal
profile shadowing its operator prose); a workcell with a thermocycler
station executes the profile and drops the prose, and one without keeps
the prose verbatim. Every plate movement between stations is a derived
**handoff** node the operator confirms — a custody event, not a stage
direction.

Each wave emits per-station packages under `stations/<name>/` and one
coordination plan, `plan.workcell.json` (`lab.workcell-run.v0`): ordered
nodes — station programs, handoffs, remaining manual steps — with explicit
dependencies. Station run documents carry no sequencing of their own in a
workcell build; the coordination plan is the single order of work, and it
is a reviewed artifact like every other. The runtime interprets compiled
plans and never plans: extending 0030, what the reviewer read is what the
workcell performs.

The instrument formats are device-neutral (`lab.thermocycle-run.v1`,
`lab.plate-read.v0`): the document states the program, the station's kind
selects the executor, and no document names a vendor.

## Consequences

- A bench upgrade is a profile edit: adding a cycler station moves thermal
  work off the operator without touching any program.
- Arm transport is a future station kind, not a new architecture; the
  handoff nodes it would execute already exist.
- `protocol.screen` has no consumer yet: the op screens a colony pool, not
  a plate, so wiring it to a reader station awaits the language-side story
  for where a plate read enters the material chain. Reader stations parse
  and validate today; no plan assigns them work.
- The per-backend wave packagers are now four structurally identical
  loops. The workcell is the pressure 0029 anticipated; consolidation is
  the next structural candidate, not part of this decision.
- One liquid handler per workcell for now; the count is validated, and
  lifting it is a planning problem (labware routing between handlers), not
  a profile problem.
