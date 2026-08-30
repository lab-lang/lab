# Facility-allocated Opentrons lowering

This tutorial records the boundary from explicit Lab source to independently reviewable OT-2 work. It is one use-case specialization, not another workflow language, a package target, or a second biological recipe model.

## Architectural boundary

The source declares plasmid and strain artifacts with checked properties, assigns built and bought designs exact `sbol_identity` values, keeps supplier order identifiers separate, and composes typed `realize`, `transform`, `recover`, `dilute`, and `plate` effects. Physical dependencies are `Material<T>` values flowing through those effects. The frontend resolves library operations, checks types and ownership, and preserves exact source ancestry without naming an instrument.

The production path has six mandatory boundaries:

1. Design and Intent LAIR contains declarative artifact identity and method-neutral typed material dataflow.
2. A validated Method registry refines reachable Intent operations into candidate Procedure graphs. Supported operations normalize into versioned canonical Procedure programs, and each program derives its exact fine-grained Capability formula.
3. A read-only LAIR analysis constructs one global planning problem. Facility planning selects Methods, exact active MaterialLots, CapabilityOfferings, Assets, adapter bindings, and dependencies together.
4. The allocation pass applies the complete solution to the same LAIR identities and produces verifier-valid Allocated Procedure LAIR with no unresolved Method choice.
5. `lab.adapter-invocations.v8` projects immutable selected Methods, exact value edges, and Procedure tasks plus exact Procedure implementation, requirement, offering, Asset, profile, material, inventory, and allocated-LAIR digests. An adapter receives only the tasks and requirements assigned to its exact invocation.
6. The OT-2 adapter validates each assigned task and its complete requirement set against the canonical program and checked operational profile, then emits one standalone reviewed Python protocol, invocation manifest, and operator document for that task.

The fixed Protocol dialect and the pre-facility Golden Gate selector no longer exist. OT-2 lowering cannot accept `CheckedModule`, portable LAIR, refined alternatives, or an entire source program. It therefore cannot select a scientific Method, re-query inventory, substitute an Asset, or reconstruct work that allocation did not assign to it.

## Exact Procedure semantics

The Golden Gate OT-2 vertical slice supports five pipetting operations and three thermal operations. `SetupGoldenGateReaction`, `PrepareChemicalTransformation`, `AddRecoveryMedium`, `SerialDilution`, and `PlateDilutedCulture` normalize to `PipettingProgramV1`. `CycleGoldenGateReaction`, `HeatShockTransformation`, and `IncubateRecoveryCulture` normalize to `ThermalProgramV1`. Transformation preparation and heat shock are separate tasks, as are recovery-medium addition and recovery incubation, so a facility can bind those operations independently even though the Golden Gate example assigns all eight operation classes to one OT-2.

The pipetting programs preserve exact transfers, distributions, mixes, logical vessels, input/output material states, order, volumes, and fluid-path policies. Portable aspiration and dispense strategies, air gaps, blowout, touch-tip, and bubble-clearing movements derive additional atomic capability requirements when present. The thermal programs preserve exact sample count, working volume, ordered stages, repeated plateaus, durations, lid setpoint, optional ramp rates, and final hold. Shared typed Procedure views require the canonical program, its exact derived clauses, parameter types, QUDT units, material roles, and selected material sources before OT-2-specific planning begins. Unsupported operations, missing requirements, and unpreserved values fail explicitly.

The adapter profile realizes portable technique requirements with facility-reviewed calibration. The example profile retains the PUDU-derived reduced aspiration rate, 10 mL conical source model, 10 mm meniscus offset, 3 mm floor, 20 percent low-volume fallback, eight-destination tracking chunk, 4 µL distribution disposal volume, 2 mm above-liquid dispense, 8 mm agar-surface offset, and calibrated touch-tip settings. Those values are not embedded in the Method or canonical Procedure program.

The adapter-owned task plan then assigns wells and validates deck constraints. Labware, slots, modules, pipettes, mounts, API level, and capacity come from the exact Asset's validated `lab.adapter-profile.v2` overlay. The profile cannot select an adapter or Asset; the manifest's exact Asset-to-driver binding and facility solution already made those decisions.

The emitted Python file embeds the complete immutable OT-2 task plan. Rust renders a checked template for the supported operation and injects the canonical plan and API level. Every Python protocol is standalone, and its sibling `invocation_manifest.json` exposes the exact facility, Asset, offerings, requirement set, Procedure task and implementation, parameters, material bindings, deck, and profile digest that produced it. Selective-plating tasks additionally emit a static JSON and PDF plate map from the same checked allocation used to render the robot protocol.

## Inventory and dependency boundary

`lab-inventory` loads and validates the package's SBOLInventory graph and exposes an immutable exact-IRI snapshot. Global planning joins each source design's exact SBOL Component identity to active MaterialLots through `sbol:built`; it never matches declaration or display names. Ambiguous lots are rejected unless policy distinguishes them.

Material sources on Allocated Procedure tasks are either exact MaterialLot bindings or exact outputs of previously selected Method choices. Dependency order therefore comes from the same semantic graph adapters consume. The OT-2 implementation does not maintain a parallel build graph, infer assembly levels, or select generated nodes from another IR.

## Generated package

A facility-aware `lab build` writes the compiler evidence under `.lab/build/compiler/`, the exact facility solution and lowering manifest at the build root, one directory per selected Asset under `.lab/build/assets/`, and one directory per allocated task inside that Asset bundle. For an OT-2 the task directory contains:

- `automation_protocol.py`, the standalone reviewed Opentrons protocol;
- `invocation_manifest.json`, the exact immutable task and allocation projection;
- `manual_protocol.typ` and its rendered `manual_protocol.pdf`; and
- the shared typesetting support copied into the bundle.

A selective-plating task also contains `plate_map.json`, `plate_map.typ`, and `plate_map.pdf`. For the bundled two-plasmid/four-strain example, the facility route contains 28 automation protocols and 32 PDF documents: two assembly tasks per plasmid and six transformation-through-plating tasks per strain, plus one plate-map PDF per strain.

`plan.execution.json` references each independently executable protocol by its complete requirement set and digest. Runtime preflight validates the inventory, planning evidence, adapter profile, child documents, and dependency DAG before narration or dispatch. An offering's SBOLInventory qualification still determines whether planning, simulation, or live execution is allowed; the presence of generated Python does not promote it.

Run `scripts/check-opentrons-bundle.sh <bundle>` to byte-compile every emitted Python protocol. Set `LAB_OPENTRONS_SIMULATOR` and run `scripts/simulate-opentrons.sh <bundle>` to exercise them with the official simulator.

## Opening a protocol in the Opentrons app

Emitted protocols declare `robotType: "OT-2"`. Opentrons moved OT-2 support into a separate application at version 9, so a 9.x app rejects them with a message pointing at the OT-2 download. Use the 8.4.x app or the `Opentrons-OT2` build to inspect the deck.

## Current boundary

The specialization validates exact MaterialLot identity, supported Procedure semantics, reaction balance, transformation and plating replicate shape, dilution volume sufficiency, plate and source-rack capacity, tip capacity, thermocycler working volume, and physical well allocation. It does not query a live inventory service, reserve stock, select among equivalent lots without policy, reason over quantity or expiration, design compatible overhangs, normalize concentrations, or upload protocols to hardware. Generated instructions and robot code require facility-specific review and qualification before physical execution.

The adapter currently emits one reviewed protocol per allocated Procedure task. That preserves facility-wide allocation and review boundaries, but it does not yet fuse serial dilution and plating into PUDU's two-tip-per-culture execution schedule. Cross-task fusion must be an explicit, verified Procedure-plan optimization over shared physical locations and fluid-path constraints; it cannot be hidden in an OT-2 template. The [BuildCompiler and PUDU equivalence audit](buildcompiler-pudu-equivalence.md) records this remaining resource-equivalence gap and the validation boundary for the implemented behavior.
