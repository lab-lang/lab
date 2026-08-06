# Lab-native Opentrons build specialization

This tutorial is a narrow lowering from explicit Lab source into one OT-2 use-case specialization. It is not an implementation of another workflow compiler or an adapter to another protocol format.

## Architectural boundary

The source declares plasmid and strain artifacts with checked properties, imports `std.bio.inventory` to associate typed symbols with external inventory identities, then composes typed `realize` and `transform` effects in workflows. Dependencies are `Material<Plasmid>` values flowing into those effects. The dependency planner derives graph roots and build waves from checked workflow IR without biological level names. Generic compiler responsibilities stop at resolving library operations, type checking, ownership checking, and preserving that dataflow.

`lab-language` does not contain an OT-2 recipe AST or a build-specific parser entry point. Source declarations are lowered into verifier-valid Design and Workflow LAIR before a concrete biological protocol is selected. The OT-2 backend accepts only `ProtocolLairProgram`; it has no API that can consume `CheckedModule` or `PortableLairProgram` directly.

The implementation has two mandatory target-neutral LAIR boundaries and one explicit robot plan:

1. Design LAIR contains declarative artifact identity, sequence, topology, copy, and acceptance intent. It contains no build recipe or procedure fields.
2. Workflow LAIR preserves source operations as typed material dataflow. `workflow.realize` owns abstract assembly inputs, artifact dependency identities, assembly policy, and reaction chemistry; `workflow.transform` realizes a strain from its chassis and carried plasmids; subsequent operations carry recovery, dilution, and plating intent on explicit use-def edges. Chemistry travels as a named dictionary rather than one attribute per reagent, so a recipe stays inspectable without the dialect growing a key per volume.
3. Protocol LAIR is produced by a Pliron dialect conversion. It selects synthesis, Golden Gate assembly, provision, transformation, recovery, serial dilution, and selective plating operations, replaces Workflow values with Protocol values, erases Workflow operations, verifies the resulting module, and runs material-linearity analysis.
4. `Ot2ExecutionPlan` is the backend-owned, validated, and resource-allocated robot plan, including source wells, reaction wells, DNA-plate wells, transformation mappings, dilution wells, and plating wells. It carries the target profile it was allocated against, so every projection reads one deck.

The JSON manifest, Markdown instructions, and all three Python protocols are projections of the same `Ot2ExecutionPlan`. This prevents an emitter from independently reconstructing or changing the robot plan. Robot-specific rendering stays under `crates/lab-compiler/src/backend/opentrons_ot2/`. Generic human rendering lives under `render/`, while `simulation/` consumes backend-neutral execution graphs.

Labware, deck slots, modules, pipettes, mounts, API level, and per-stage capacity come from a target profile rather than from constants. `profile.rs` parses and validates one: it rejects a slot an OT-2 does not address, a slot the installed thermocycler already occupies, two pieces of labware claiming one slot during a stage, and any key it does not recognize. Every field defaults to the reference bench, so a profile states only what differs.

Declaring more than one slot for a plate raises the batch size a bench holds. Well addresses are plate-and-well pairs, and allocation fills each declared plate in turn.

Robot behavior is maintained as a pinned Python project under `backend/opentrons_ot2/python/`. Its protocol modules import the shared `Ot2ExecutionPlan` `TypedDict` unconditionally and pass Ruff, strict mypy, and pytest checks. Rust does not assemble Python operations: it includes those checked source files, replaces the type-module import with the same marked type definitions, and injects the serialized execution plan. This deterministic bundling step produces the standalone Python file required by the robot without sacrificing normal Python tooling in the source tree or emitted package.

Artifact graph resolution is a separate compiler planning concern. The package compiler projects dependency edges and material requirements directly from verified Protocol operations; `crates/lab-compiler/src/planning/dependencies/` resolves roots, inventory hits, cycles, blockers, and build waves without knowing anything about plasmids, Golden Gate, or robots. The OT-2 planner then specializes only the generated nodes by selecting their Protocol artifact identities. It never constructs a parallel OT-2 biological recipe IR.

The OT-2 specialization selects the concrete realization used by this tutorial:

- Golden Gate assembly for `realize`;
- heat-shock transformation for `transform`, which realizes a strain;
- culture recovery for `recover`;
- serial dilution for `dilute`; and
- selective plating for `plate`.

If source omits or misorders a required material transition, Workflow verification fails before Protocol selection. Other laboratory profiles can provide another Workflow-to-Protocol conversion, while another robot backend can consume the same verified Protocol operations and implement its own execution plan.

## Generated package

Lab emits its own deterministic execution-plan manifest, consolidated human instructions, and standalone OT-2 Python protocols. A dependency-driven build adds a machine-readable graph, a human dependency report, and one self-contained directory per planning wave.

Artifacts in one wave have no ordering constraint between them, so a wave is a single robot run over a single deck. A wave emits a protocol only for the stages its artifacts reach: a wave that assembles plasmids and transforms none produces no plating protocol, rather than one that would fail on the robot over an empty well list.

The implementation validates each design's reaction balance against its own stated volume, replicate and dilution bounds, plate capacity across every declared slot, source-rack capacity, and tip capacity. Generated Python is exercised with the official Opentrons simulator.

Run `scripts/check-opentrons-target.sh <bundle>` to lint and typecheck the maintained Python target and every emitted protocol, followed by `scripts/simulate-opentrons.sh <bundle>` for Opentrons simulation.

## Opening a protocol in the Opentrons app

Emitted protocols declare `robotType: "OT-2"`. Opentrons moved OT-2 support into a separate application at version 9, so a 9.x app rejects them with a message pointing at the OT-2 download. Use the 8.4.x app or the `Opentrons-OT2` build to see the deck.

## Current boundary

This spike does not yet query a live inventory service, resolve inventory lots, ingest SBOL, design compatible overhangs, normalize source concentrations, prepare DNA between dependent waves, or attach runtime evidence to acceptance decisions. Generated instructions and robot code require laboratory review and qualification before physical execution.
