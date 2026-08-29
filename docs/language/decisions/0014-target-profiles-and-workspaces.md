# 0014 — Target profiles configure a bench; workspaces group packages

## Status

Partially superseded by [0044: Facility graphs and capability binding replace workcell targets](0044-facility-graphs-replace-workcell-targets.md). The workspace decision remains accepted; the independent target-profile selection described below is historical.

## Context

Every deck slot, labware load name, pipette model, mount, and capacity was a
constant in the OT-2 backend. A program therefore compiled for exactly one
bench, and the deck — the thing an operator actually looks at — was invisible to
the toolchain and unauthorable by the laboratory running the work.

Separately, `lab build` produced portable module IR and stopped. Only the
`labc` binary reached a backend, and only for a single file, so a project whose
designs and workflows lived in different modules could not be compiled into
robot protocols at all.

## Decision

Site configuration lives in a target profile: one TOML file per bench under
`targets/`, selected by `lab build --target <name>`. A profile describes
modules, labware, deck slots, instruments, mounts, and per-stage capacity. It
describes no science.

Reaction chemistry stays in `.lab` source as quantity-valued declaration
properties. The dividing line is whose claim it is: reagent volumes and thermal
profiles are claims about the science and travel with the artifact; deck slots
and labware are claims about a laboratory and travel with the profile.

Every profile field has a default matching the backend's reference bench, so a
profile states only what differs. Unknown keys are rejected rather than ignored,
because a misspelled slot falling back to a default is how a protocol ends up
aspirating from the wrong place.

A `lab.toml` is either a package manifest or a workspace manifest, never both. A
workspace root owns membership and nothing else; members stay ordinary
self-contained packages. Generated artifacts and `lab.lock` live at the
workspace root.

A target build lowers the default member and everything it depends on as one
program, so an artifact declared in one module or package may be realized by a
workflow in another.

## Consequences

The execution plan carries the profile it was allocated against, and every
emitted projection — JSON manifest, Markdown instructions, and Python protocols
— reads the deck from that plan rather than from a constant. Changing a profile
changes the deck the Opentrons app renders, with no source edit.

Declaring more than one slot for a plate raises the batch size a bench can hold.
Well addresses became plate-and-well pairs so allocation can spill across the
plates a profile declares.

The `backend` field in a profile is validated but not dispatched on; there is
one backend and it is named concretely. A second backend needs a registry and a
selection rule, which remain open.
