# Facility-allocated Opentrons lowering

This tutorial records the boundary from explicit Lab source to independently reviewable OT-2 work. It is one use-case specialization, not another workflow language, a package target, or a second biological recipe model.

## Architectural boundary

The source declares plasmid and strain artifacts with checked properties, assigns built and bought designs exact `sbol_identity` values, keeps supplier order identifiers separate, and composes typed `realize`, `transform`, `recover`, `dilute`, and `plate` effects. Physical dependencies are `Material<T>` values flowing through those effects. The frontend resolves library operations, checks types and ownership, and preserves exact source ancestry without naming an instrument.

The production path has six mandatory boundaries:

1. Design and Intent LAIR contains declarative artifact identity and method-neutral typed material dataflow.
2. A validated Method registry refines reachable Intent operations into candidate Procedure graphs. Each generic `procedure.task` carries stable operation semantics, exact parameters and units, typed value edges, material inputs, and one or more first-class Capability requirements.
3. A read-only LAIR analysis constructs one global planning problem. Facility planning selects Methods, exact active MaterialLots, CapabilityOfferings, Assets, adapter bindings, and dependencies together.
4. The allocation pass applies the complete solution to the same LAIR identities and produces verifier-valid Allocated Procedure LAIR with no unresolved Method choice.
5. `lab.adapter-invocations.v6` projects immutable selected Methods and Procedure tasks plus exact requirement, offering, Asset, profile, material, inventory, and allocated-LAIR digests. An adapter receives only the tasks and requirements assigned to its exact invocation.
6. The OT-2 adapter validates each assigned task against its checked operational profile and emits one standalone reviewed Python protocol, invocation manifest, and operator document per supported requirement.

The fixed Protocol dialect and the pre-facility Golden Gate selector no longer exist. OT-2 lowering cannot accept `CheckedModule`, portable LAIR, refined alternatives, or an entire source program. It therefore cannot select a scientific Method, re-query inventory, substitute an Asset, or reconstruct work that allocation did not assign to it.

## Exact Procedure semantics

The current OT-2 vertical slice supports three open Procedure operation IRIs: setup of a Golden Gate reaction, thermal cycling of that reaction, and serial dilution. Shared typed Procedure views require the exact capability kind, parameter identity, scalar or list type, canonical QUDT unit, material role, and selected material source before OT-2-specific planning begins. Unsupported operations and unpreserved parameters fail explicitly.

The adapter-owned task plan then assigns wells and validates deck constraints. Labware, slots, modules, pipettes, mounts, API level, and capacity come from the exact Asset's validated `lab.adapter-profile.v2` overlay. The profile cannot select an adapter or Asset; the manifest's exact Asset-to-driver binding and facility solution already made those decisions.

The emitted Python file embeds the complete immutable OT-2 task plan. Rust renders a checked template for the supported operation and injects the canonical plan and API level. Every Python protocol is standalone, and its sibling `invocation_manifest.json` exposes the exact facility, Asset, offering, requirement, Procedure task, parameters, material bindings, deck, and profile digest that produced it.

## Inventory and dependency boundary

`lab-inventory` loads and validates the package's SBOLInventory graph and exposes an immutable exact-IRI snapshot. Global planning joins each source design's exact SBOL Component identity to active MaterialLots through `sbol:built`; it never matches declaration or display names. Ambiguous lots are rejected unless policy distinguishes them.

Material sources on Allocated Procedure tasks are either exact MaterialLot bindings or exact outputs of previously selected Method choices. Dependency order therefore comes from the same semantic graph adapters consume. The OT-2 implementation does not maintain a parallel build graph, infer assembly levels, or select generated nodes from another IR.

## Generated package

A facility-aware `lab build` writes the compiler evidence under `.lab/build/compiler/`, the exact facility solution and lowering manifest at the build root, one directory per selected Asset under `.lab/build/assets/`, and one directory per allocated task inside that Asset bundle. For an OT-2 the task directory contains:

- `automation_protocol.py`, the standalone reviewed Opentrons protocol;
- `invocation_manifest.json`, the exact immutable task and allocation projection;
- `manual_protocol.typ` and its rendered `manual_protocol.pdf`; and
- the shared typesetting support copied into the bundle.

`plan.execution.json` references each independently executable protocol by requirement and digest. Runtime preflight validates the inventory, planning evidence, adapter profile, child documents, and dependency DAG before narration or dispatch. An offering's SBOLInventory qualification still determines whether planning, simulation, or live execution is allowed; the presence of generated Python does not promote it.

Run `scripts/check-opentrons-bundle.sh <bundle>` to byte-compile every emitted Python protocol. Set `LAB_OPENTRONS_SIMULATOR` and run `scripts/simulate-opentrons.sh <bundle>` to exercise them with the official simulator.

## Opening a protocol in the Opentrons app

Emitted protocols declare `robotType: "OT-2"`. Opentrons moved OT-2 support into a separate application at version 9, so a 9.x app rejects them with a message pointing at the OT-2 download. Use the 8.4.x app or the `Opentrons-OT2` build to inspect the deck.

## Current boundary

The specialization validates exact MaterialLot identity, supported Procedure semantics, reaction balance, replicate and dilution bounds, plate capacity, source-rack capacity, and tip capacity. It does not query a live inventory service, reserve stock, select among equivalent lots without policy, reason over quantity or expiration, design compatible overhangs, normalize concentrations, or upload protocols to hardware. Generated instructions and robot code require facility-specific review and qualification before physical execution.
