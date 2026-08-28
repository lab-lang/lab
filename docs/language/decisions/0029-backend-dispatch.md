# 0029 — A profile's backend key selects its backend

## Status

Superseded by [0044: Facility graphs and capability binding replace workcell targets](0044-facility-graphs-replace-workcell-targets.md). This records the historical direct-target dispatch design.

## Context

0014 introduced target profiles and validated their `backend` field without
dispatching on it: there was one backend, named concretely at every call site.
It recorded the open question — a second backend needs a registry and a
selection rule.

The second backend now exists. The Opentrons Flex takes the same verified
Protocol LAIR the OT-2 takes, plans the same build, and emits JSON protocols
(protocol schema v8) instead of Python. The two robots disagree about nearly
everything a profile states — slot names, pipette models, module vocabulary,
where the trash is — so one profile schema cannot describe both benches.

## Decision

The `[target] backend` key in a profile selects both the profile schema and
the backend that compiles the build. The CLI peeks the key out of the TOML
before committing to a schema, the same way `lab.toml` parsing peeks for a
`workspace` table before deciding what kind of manifest it holds. An absent
key means `opentrons.ot2`, matching that profile schema's default, so every
existing project builds unchanged. An unknown backend is an error naming the
backends this toolchain provides.

Dispatch is a closed enum over the concrete profile/backend pairs at the CLI
boundary, not a registry. `Backend` has associated types, and with two
implementations the full cost of dynamic dispatch — type-erased programs,
boxed errors, a registration mechanism — buys nothing a two-armed match does
not. The registry question stays open until a backend arrives that cannot be
listed at compile time.

Backends are grouped by vendor family, matching the descriptor they already
publish: `backend/opentrons/` holds `ot2/` and `flex/`. Planning that names no
robot sits one level up, beside the backend contracts — provenance analysis
over Protocol LAIR, build-graph projection, SBS plate geometry and well
allocation, the labware groupings every bench profile declares, and the
dependency-report renderers. Where code lives states who owns it: a module
under a vendor family is that vendor's, and a module beside the contracts
holds for any liquid handler. Backend identity enters the common planning only
as a parameter, so a capacity error names the machine that planned the build.
Everything else — profile vocabulary, stage constraints, execution plans,
emitters — stays inside each machine's containment boundary.

A generated artifact is recognized as a robot protocol by the emitters'
naming convention, `*_protocol.py` or `*_protocol.json`, rather than by file
extension, because a Python file is not the only thing a robot runs.

## Consequences

`lab build --target <name>` compiles for whichever backend the named profile
declares; two profiles over the same program yield an OT-2 package and a Flex
package with the same wave structure, manifest schema, and manual-protocol
shape.

Each backend keeps its own spelling of every profile field it interprets, so
a Flex profile that names an OT-2 pipette fails validation with a sentence,
not a robot-side rejection.

Adding a third backend means adding an enum arm and a profile schema; the
sanctioned shape stays a match until that stops scaling.
