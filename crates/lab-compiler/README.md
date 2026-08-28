# Compiler implementation notes

`crates/lab-compiler/` builds the experimental `labc` compiler and `lab-opt` IR tool. These are deliberately compiler-development interfaces. The standard Lab workflow is exposed through the repository's `lab` binary. None is a stable interface yet.

`CheckedModule` is the portable source-compilation boundary, and verified LAIR is the mandatory backend boundary. Source lowering constructs a `PortableLairProgram` containing Design and Workflow LAIR. Protocol selection consumes that program and returns a `ProtocolLairProgram`; typed robot backends consume only that verified Protocol boundary and cannot accept a checked source module directly. Artifact emission remains a separate operation. Output selection therefore does not choose a different parser or semantic pipeline or bypass LAIR.

This is the first vertical slice of a larger progressive lowering stack. LAIR preserves high-level biological and workflow intent while later dialects select laboratory methods, bind materials and resources, schedule work, and finally produce device-specific operations for instruments, people, and services. An SBOLInventory facility graph describes installed capability offerings, while an adapter implements planning, lowering, simulation, or execution for exact Assets selected by reviewed plans.

The current Protocol IR is method-selected but not hardware-level. Containers, inventory lots, locations, timing, scheduling, deck geometry, device commands, and durable dispatch belong to later facility, adapter, and runtime layers.

The source tree follows semantic ownership and dependency direction:

- `src/lair/` contains Design, Workflow, and Protocol dialects; the Workflow-to-Protocol dialect conversion; material-linearity analysis and pass; stage contracts; and the textual IR session;
- `src/planning/` resolves artifact graphs against inventory without robot knowledge;
- `src/backend/` defines the adapter registry and concrete single-device compilers, grouped by vendor family under `opentrons/` and `hamilton/`;
- `src/artifact/` defines generated files independently of filesystem persistence;
- `lab-runfmt` defines the reviewed documents the `lab` runner interprets, including `lab.execution-plan.v1`, `lab.simulation-run.v1`, `lab.star-run.v0`, `lab.thermocycle-run.v0`, and `lab.plate-read.v0`; and
- `src/bin/labc/` and `src/bin/lab-opt/` contain developer-facing command orchestration.

The dependency direction is language model → planning/LAIR → backend → artifacts, with command-line applications owning filesystem writes. LAIR and generic planning do not depend on concrete robots.

`lab.adapter-catalog.v1` is the machine-readable implementation contract. Each stable adapter ID declares exact SBOLInventory capability-kind and control-mode IRIs, implementation features, accepted and emitted run-document formats, configuration schema, and truthful planning, simulation, and runtime support. Semantic capabilities and implementation features are deliberately separate. A driver is selected only by an explicit binding to an exact Asset IRI, never by manufacturer or model inference.

`labc --emit` can expose the source AST, checked module IR, or an artifact emitted by a backend; its developer-only `--adapter` and `--adapter-profile` arguments choose one explicit low-level implementation without allowing the profile to select code. `lab-opt` separately parses, verifies, transforms, and prints textual LAIR without acting as another source frontend. Source-to-LAIR lowering lives under `src/lair/`; no production backend module imports `lab-language`. Design LAIR contains only declarative artifact identity, sequence, topology, copy, and acceptance intent. Workflow LAIR preserves `realize`, `provision`, `transform`, `recover`, `dilute`, and `plate` as typed material operations with explicit SSA use-def edges. A Pliron dialect conversion replaces that Workflow dataflow with verifier-valid Protocol operations and then eliminates every Workflow operation.

The OT-2 backend lives entirely under `src/backend/opentrons/ot2/` and accepts `ProtocolLairProgram`, not source IR or an OT-2-specific copy of the biological recipe. Its planner analyzes Protocol operations and their use-def chains directly, validates adapter constraints, and allocates an `Ot2ExecutionPlan`. The manifest, manual protocol, and three OT-2 protocol stages are rendered from that one execution plan. Robot constants, deck capacities, labware choices, Python generation, and robot-specific package text do not live in generic rendering, planning, or the language frontend. Robot behavior is maintained in the backend-local `python/` project and checked with Ruff, strict mypy, pytest, byte compilation, and Opentrons simulation; Rust bundles those Python modules and injects the execution plan.

`dependency-plan` and `full-build-bundle` first use the facility-independent resolver in `src/planning/` to resolve source-declared artifact dependencies against supplied inventory evidence. The OT-2 package layer then compiles each successful graph node. A full-build bundle includes one consolidated human protocol in dependency-safe execution order, a separate dependency report, and standalone human/robot artifacts for each planning wave. Artifacts in one wave have no ordering constraint between them, so a wave is a single robot run over one deck. Adapters return an `ArtifactBundle`; `labc` is responsible for writing it to disk.

`labc` compiles one source file, so the modules it accepts are self-contained; a multi-module package is `lab build`'s job. The single-module sources these commands are exercised against live in [`tests/fixtures/`](tests/fixtures/). For the end-to-end package — designs, SBOLInventory facility, exact adapter binding, and the OT-2 protocols a robot application can open — see the [Golden Gate example](../../examples/golden-gate/README.md).

The end-to-end test runs every generated protocol through the official Opentrons simulator when `LAB_OPENTRONS_SIMULATOR` points at the executable. It stays optional so ordinary CI does not download the large robotics runtime:

```sh
uv venv .lab/opentrons-venv --python 3.12
uv pip install --python .lab/opentrons-venv/bin/python 'opentrons>=8.4.1,<9'
LAB_OPENTRONS_SIMULATOR=.lab/opentrons-venv/bin/opentrons_simulate \
  cargo test -p lab-compiler --test opentrons_build
```
