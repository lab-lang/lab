# Facility-allocated Opentrons lowering

This tutorial records the boundary from explicit Lab source to independently reviewable OT-2 work. It is one use-case specialization, not another workflow language, a package target, or a second biological recipe model.

## Architectural boundary

The source declares plasmid and strain artifacts with checked properties, assigns built and bought designs exact `sbol_identity` values, keeps supplier order identifiers separate, and composes typed `realize`, `transform`, `recover`, `dilute`, and `plate` effects. Physical dependencies are `Material<T>` values flowing through those effects. The frontend resolves library operations, checks types and ownership, and preserves exact source ancestry without naming an instrument.

The production path has six mandatory boundaries:

1. Design and Intent LAIR contains declarative artifact identity and method-neutral typed material dataflow.
2. A validated Method registry refines reachable Intent operations into candidate Procedure graphs. Supported operations normalize into versioned canonical Procedure programs, and each program derives its exact fine-grained Capability formula.
3. A read-only LAIR analysis constructs one global planning problem. Facility planning selects Methods, exact active MaterialLots, CapabilityOfferings, Assets, adapter bindings, and dependencies together.
4. The allocation pass applies the complete solution to the same LAIR identities and produces verifier-valid Allocated Procedure LAIR with no unresolved Method choice.
5. `lab.adapter-invocations.v1` projects immutable selected Methods, exact value edges, and Procedure tasks plus exact Procedure implementation, requirement, offering, Asset, profile, material, inventory, and allocated-LAIR digests. An adapter receives only the tasks and requirements assigned to its exact invocation.
6. The OT-2 adapter validates every assigned task and its complete requirement set against the canonical program and checked operational profile, constructs `lab.allocated-procedure-schedule.v1` with persistent physical locations and dependency-preserving execution groups, and emits one standalone reviewed Python protocol, manifest, and operator document for each group.

The fixed Protocol dialect and the pre-facility Golden Gate selector no longer exist. OT-2 lowering cannot accept `CheckedModule`, portable LAIR, refined alternatives, or an entire source program. It therefore cannot select a scientific Method, re-query inventory, substitute an Asset, or reconstruct work that allocation did not assign to it.

## Exact Procedure semantics

The Golden Gate OT-2 vertical slice supports five pipetting operations and three thermal operations. `SetupGoldenGateReaction`, `PrepareChemicalTransformation`, `AddRecoveryMedium`, `SerialDilution`, and `PlateDilutedCulture` normalize to `PipettingProgramV1`. `CycleGoldenGateReaction`, `HeatShockTransformation`, and `IncubateRecoveryCulture` normalize to `ThermalProgramV1`. Transformation preparation and heat shock are separate tasks, as are recovery-medium addition and recovery incubation, so a facility can bind those operations independently even though the Golden Gate example assigns all eight operation classes to one OT-2.

The pipetting programs preserve exact transfers, distributions, mixes, logical vessels, input/output material states, order, volumes, fluid-path policies, and cross-cutting source-temperature constraints. Portable temperature staging, aspiration and dispense strategies, air gaps, blowout, touch-tip, source mixing, final-path reuse, and bubble-clearing movements derive additional atomic capability requirements when present. The bundled `temperature-staged-golden-gate` Method expresses those preparation semantics through `PipettingProgramV1`; it does not change the thermal values authored on the experiment. The thermal programs preserve exact sample count, working volume, ordered stages, repeated plateaus, durations, lid setpoint, optional ramp rates, and final hold. Shared typed Procedure views require the canonical program, its exact derived clauses, parameter types, QUDT units, material roles, and selected material sources before OT-2-specific planning begins. Unsupported operations, missing requirements, and unpreserved values fail explicitly.

The adapter profile realizes portable technique requirements with facility-reviewed calibration. The example profile retains the PUDU-derived reduced aspiration rate, 10 mL conical source model, 10 mm meniscus offset, 3 mm floor, 20 percent low-volume fallback, eight-destination tracking chunk, 4 µL distribution disposal volume, 2 mm above-liquid dispense, 8 mm agar-surface offset, and calibrated touch-tip settings. Those values are not embedded in the Method or canonical Procedure program.

The adapter-owned schedule assigns persistent wells and validates deck constraints across the complete invocation. Labware, slots, modules, pipettes, mounts, API level, and capacity come from the exact Asset's validated `lab.adapter-profile.v2` overlay. The profile cannot select an adapter or Asset; the manifest's exact Asset-to-driver binding and facility solution already made those decisions.

Each emitted Python file embeds the complete immutable OT-2 run plan. Rust renders a checked template for the scheduled execution group and injects the canonical programs, persistent locations, profile, and API level. Every Python protocol is standalone, and its sibling run manifest exposes the exact facility, Asset, offerings, requirement set, Procedure tasks and implementations, parameters, material bindings, schedule digest, deck, and profile digest that produced it. The plating group additionally emits a static JSON and PDF plate map from the same checked allocation used to render the robot protocol.

## Inventory and dependency boundary

`lab-inventory` loads and validates the package's SBOLInventory graph and exposes an immutable exact-IRI snapshot. Global planning joins each source design's exact SBOL Component identity to active MaterialLots through `sbol:built`; it never matches declaration or display names. Ambiguous lots are rejected unless policy distinguishes them.

Material sources on Allocated Procedure tasks are either exact MaterialLot bindings or exact outputs of previously selected Method choices. Dependency order therefore comes from the same semantic graph adapters consume. The OT-2 implementation does not maintain a parallel build graph, infer assembly levels, or select generated nodes from another IR.

## Generated package

A facility-aware `lab build` writes the compiler evidence under `.lab/build/compiler/`, the exact facility solution and lowering manifest at the build root, and one directory per selected Asset under `.lab/build/assets/`. For the complete Golden Gate invocation, the OT-2 Asset bundle contains:

- `execution_schedule.json`, the versioned execution groups, dependencies, and persistent physical-location ledger;
- `assembly_protocol.py`, `transformation_protocol.py`, and `plating_protocol.py`, the three standalone reviewed Opentrons protocols;
- `assembly_manifest.json`, `transformation_manifest.json`, and `plating_manifest.json`, the exact immutable group and allocation projections;
- one Typst source and rendered operator PDF per run; and
- `plate_map.json`, `plate_map.typ`, and `plate_map.pdf`, static plating evidence generated from the reviewed allocation.

For the bundled two-plasmid/four-strain example, the facility route contains three automation protocols and four PDF documents. The assembly protocol batches both setup/cycle pairs, the transformation protocol batches all preparation/heat-shock/recovery chains, and the plating protocol batches all dilution/plating pairs. The scientific tasks, their individual requirements, and their provenance identities remain distinct inside the reviewed runs.

`plan.execution.json` references each independently executable run by the union of its exact task requirement sets and by its document digest. Runtime preflight validates the inventory, planning evidence, adapter profile, child documents, schedule, and dependency DAG before narration or dispatch. An offering's SBOLInventory qualification still determines whether planning, simulation, or live execution is allowed; the presence of generated Python does not promote it.

Run `scripts/check-opentrons-bundle.sh <bundle>` to byte-compile every emitted Python protocol. Set `LAB_OPENTRONS_SIMULATOR` and run `scripts/simulate-opentrons.sh <bundle>` to exercise them with the official simulator.

## Opening a protocol in the Opentrons app

Emitted protocols declare `robotType: "OT-2"`. Opentrons moved OT-2 support into a separate application at version 9, so a 9.x app rejects them with a message pointing at the OT-2 download. Use the 8.4.x app or the `Opentrons-OT2` build to inspect the deck.

## Current boundary

The specialization validates exact MaterialLot identity, supported Procedure semantics, reaction balance, transformation and plating replicate shape, dilution volume sufficiency, plate and source-rack capacity, tip capacity, thermocycler working volume, and physical well allocation. It does not query a live inventory service, reserve stock, select among equivalent lots without policy, reason over quantity or expiration, design compatible overhangs, normalize concentrations, or upload protocols to hardware. Generated instructions and robot code require facility-specific review and qualification before physical execution.

The adapter recognizes the complete allocated Golden Gate Procedure graph and emits three dependency-ordered runs. Serial dilution and plating share one explicit execution group, preserving PUDU's two-tip-per-culture schedule: the first path seeds dilution two before contacting agar and then plates dilution one, while a fresh path plates dilution two. A partial or different Procedure graph falls back to independently reviewable task protocols rather than being silently forced into this specialization. The [PUDU workflow equivalence audit](pudu-workflow-equivalence.md) records the executable output comparison and validation boundary.
