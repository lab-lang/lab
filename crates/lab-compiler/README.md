# Compiler implementation notes

`crates/lab-compiler/` builds the experimental `labc` compiler and `lab-opt` IR tool. These are deliberately compiler-development interfaces. The standard Lab workflow is exposed through the repository's `lab` binary. None is a stable interface yet.

`CheckedModule` is the portable source-compilation boundary, and verified LAIR is the mandatory backend boundary. Source lowering constructs a `PortableLairProgram` containing Design and Workflow LAIR. Protocol selection consumes that program and returns a `ProtocolLairProgram`; typed robot backends consume only that verified Protocol boundary and cannot accept a checked source module directly. Artifact emission remains a separate operation. Output selection therefore does not choose a different parser or semantic pipeline or bypass LAIR.

This is the first vertical slice of a larger progressive lowering stack. LAIR preserves high-level biological and workflow intent while later dialects select laboratory methods, bind materials and resources, schedule work, and finally produce target-specific operations for instruments, people, and services. A laboratory profile describes available capabilities and policy preferences; a backend implements an execution target such as a robot family.

The current Protocol IR is target-selected but not hardware-level. Containers, inventory lots, locations, timing, scheduling, deck geometry, device commands, and durable dispatch belong to later lowering and runtime layers.

The source tree follows semantic ownership and dependency direction:

- `src/lair/` contains Design, Workflow, and Protocol dialects; the Workflow-to-Protocol dialect conversion; material-linearity analysis and pass; stage contracts; and the textual IR session;
- `src/planning/protocol/` defines and validates backend-neutral scientific plans;
- `src/planning/dependencies/` resolves artifact graphs against inventory without robot knowledge;
- `src/simulation/` owns the symbolic execution graph and interprets it into shared lab state and event traces;
- `src/backend/` defines backend contracts and contains concrete robot implementations;
- `src/artifact/` defines generated files independently of filesystem persistence;
- `src/render/` contains human projections of compiler-owned representations; and
- `src/bin/labc/` and `src/bin/lab-opt/` contain developer-facing command orchestration.

The dependency direction is language model → planning/LAIR → backend → artifacts, with simulation consuming planning models independently and command-line applications owning filesystem writes. LAIR and generic planning do not depend on concrete robots.

`labc --emit` can expose the source AST, checked module IR, or an artifact emitted by a backend. `lab-opt` separately parses, verifies, transforms, and prints textual LAIR without acting as another source frontend. Source-to-LAIR lowering lives under `src/lair/`; no production backend module imports `lab-language`. Design LAIR contains only declarative artifact identity, sequence, topology, copy, and acceptance intent. Workflow LAIR preserves `realize`, `provision`, `transform`, `recover`, `dilute`, and `plate` as typed material operations with explicit SSA use-def edges. A Pliron dialect conversion replaces that Workflow dataflow with verifier-valid Protocol operations and then eliminates every Workflow operation.

The OT-2 backend lives entirely under `src/backend/opentrons_ot2/` and accepts `ProtocolLairProgram`, not source IR or an OT-2-specific copy of the biological recipe. Its planner analyzes Protocol operations and their use-def chains directly, validates target constraints, and allocates an `Ot2ExecutionPlan`. The manifest, manual protocol, and three OT-2 protocol stages are rendered from that one execution plan. Robot constants, deck capacities, labware choices, Python generation, and robot-specific package text do not live in generic rendering, planning, or the language frontend. Robot behavior is maintained in the backend-local `python/` project and checked with Ruff, strict mypy, pytest, byte compilation, and Opentrons simulation; Rust bundles those Python modules and injects the execution plan.

`dependency-plan` and `full-build-bundle` first use the target-neutral resolver in `src/planning/dependencies/` to resolve source-declared artifact dependencies against a JSON inventory. The OT-2 package layer then compiles each successful graph node. A full-build bundle includes one consolidated human protocol in dependency-safe execution order, a separate dependency report, and standalone human/robot artifacts for each generated batch. Backends return an `ArtifactBundle`; `labc` is responsible for writing it to disk.

See the [plasmid acceptance example](../examples/plasmid-acceptance/README.md) for runnable commands and current limitations.
See the [Opentrons build example](../../examples/opentrons-build/README.md) for the end-to-end Golden Gate, transformation, and plating bundle.
