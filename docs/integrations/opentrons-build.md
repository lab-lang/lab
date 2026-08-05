# Lab-native Opentrons build specialization

This tutorial is a narrow lowering from explicit Lab source into one OT-2 use-case specialization. It is not an implementation of another workflow compiler or an adapter to another protocol format.

## Architectural boundary

The source declares artifacts and checked properties, imports `std.bio.inventory` to associate typed symbols with external inventory identities, then imports `std.bio.build` and composes typed `realize` effects in workflows. Dependencies are `Material<Plasmid>` values flowing into those effects. The dependency planner derives graph roots and build waves from checked workflow IR without biological level names. Generic compiler responsibilities stop at resolving library operations, type checking, ownership checking, and preserving that dataflow.

`lab-language` does not contain an OT-2 recipe AST or a build-specific parser entry point. Field interpretation happens only while lowering a backend-neutral `CheckedModule` inside `lab_compiler::backend::opentrons_ot2`.

The implementation has two explicit target IRs:

1. `Ot2BuildIr` is the target-selected biological build derived from checked declarations, inventory identities, resolved effects, and material dependencies.
2. `Ot2ExecutionPlan` is the validated and resource-allocated robot plan, including source wells, reaction wells, transformation mappings, dilution wells, and plating wells.

The JSON manifest, Markdown instructions, and all three Python protocols are projections of the same `Ot2ExecutionPlan`. This prevents an emitter from independently reconstructing or changing the robot plan. OT-2 API versions, capacities, supported sequences, labware, modules, pipettes, deck slots, and robot-specific rendering stay under `crates/lab-compiler/src/backend/opentrons_ot2/`. Generic human rendering lives under `render/`, while `simulation/` consumes backend-neutral execution graphs.

Robot behavior is maintained as a pinned Python project under `backend/opentrons_ot2/python/`. Its protocol modules import the shared `Ot2ExecutionPlan` `TypedDict` unconditionally and pass Ruff, strict mypy, and pytest checks. Rust does not assemble Python operations: it includes those checked source files, replaces the type-module import with the same marked type definitions, and injects the serialized execution plan. This deterministic bundling step produces the standalone Python file required by the robot without sacrificing normal Python tooling in the source tree or emitted package.

Artifact graph resolution is a separate compiler planning concern. The OT-2 lowerer supplies graph nodes with dependencies, requested abstract steps, and material requirements; `crates/lab-compiler/src/planning/dependencies/` resolves roots, inventory hits, cycles, blockers, and build waves without knowing anything about plasmids, Golden Gate, or robots. The OT-2 package layer then specializes only the generated nodes.

The OT-2 specialization selects the concrete realization used by this tutorial:

- Golden Gate assembly for `realize`;
- heat-shock transformation for `transform`;
- culture recovery for `recover`;
- serial dilution for `dilute`; and
- selective plating for `plate`.

If source requests another step sequence, this specialization rejects it explicitly. Other hardware or laboratory profiles can lower the same abstract operations differently.

## Generated package

Lab emits its own deterministic execution-plan manifest, consolidated human instructions, and standalone OT-2 Python protocols. A dependency-driven full build adds a machine-readable graph, a human dependency report, and one self-contained batch directory per generated artifact.

The implementation validates reaction volume, replicate and dilution bounds, plate capacity, source-rack capacity, and tip capacity. Generated Python is exercised with the official Opentrons simulator.

Run `scripts/check-opentrons-target.sh .lab/full-build` to lint and typecheck the maintained Python target and every emitted protocol, followed by `scripts/simulate-opentrons.sh .lab/full-build` for Opentrons simulation.

## Current boundary

This spike does not yet query a live inventory service, resolve inventory lots, ingest SBOL, design compatible overhangs, normalize source concentrations, prepare DNA between dependent batches, or attach runtime evidence to acceptance decisions. Generated instructions and robot code require laboratory review and qualification before physical execution.
