# Lab documentation

This directory records what Lab is becoming, what has been decided, and what the current implementation can honestly do.

The compiler has two frontends. Python is where most experiments are written, and its API is documented with the [Python SDK](../crates/lab-python/README.md). Lab is the native language, and it is what this directory describes: the specimens and design documents below are written in Lab because it names these ideas in the fewest words. Both frontends lower to the same checked module, so nothing downstream can tell which one a declaration came from.

If you are new to the language, start with the [repository introduction](../README.md), then read the representative programs:

- [plasmid design](language/specimens/plasmid-design.lab) shows typed biological composition, requirements, and evidence-based acceptance;
- [plasmid construction](language/specimens/plasmid-build.lab) shows durable actions, explicit state, reactive timers, structured outcomes, and affine material flow.
- [inventory-backed plasmid](language/specimens/inventory-plasmid.lab) shows typed external identities, declarative properties, and a realization workflow;
- [dependency build](language/specimens/dependency-build.lab) shows artifact ordering derived from typed material inputs rather than named assembly levels.

## Language guide

| Document | What it answers |
| --- | --- |
| [Language overview](language/README.md) | What are the major source-level concerns and compiler boundaries? |
| [Surface syntax](language/syntax.md) | What are the keywords, operators, blocks, bindings, and constructors? |
| [Core semantics](language/semantics.md) | What do materials, evidence, effects, ownership, acceptance, and replay mean? |
| [Modules and packages](language/modules.md) | How are imports, projects, source modules, policies, workflows, and programs organized? |
| [Implementation support](language/support.md) | Which features parse, resolve, type-check, lower, execute, or work in editors today? |
| [Open questions](language/open-questions.md) | Which important language choices remain deliberately unresolved? |

The support matrix is authoritative when a specimen or design document extends beyond the executable implementation.

## Design decisions

Decision records preserve the reasoning and status behind the language rather than leaving it implicit in parser code.

| Decision | Direction |
| --- | --- |
| [0001: Minimal language kernel](language/decisions/0001-language-kernel.md) | indentation for behavior, braces for data, `=` for pure work, `<-` for durable effects |
| [0002: Reactive durable workflows](language/decisions/0002-reactive-workflows.md) | deterministic state machines driven by recorded actions, timers, and events |
| [0003: Modules and packages](language/decisions/0003-modules-and-packages.md) | whole-module imports and conventional project organization |
| [0004: Portable module IR](language/decisions/0004-portable-module-ir.md) | a typed frontend boundary before target selection and execution |
| [0005: Explicit workflow state](language/decisions/0005-explicit-workflow-state.md) | immutable ordinary bindings and explicit durable mutation |
| [0006: Affine material flow](language/decisions/0006-affine-material-flow.md) | one owning place for each physical material, checked across control flow |
| [0007: Toolchain CLI boundary](language/decisions/0007-toolchain-cli-boundary.md) | `lab` for working with Lab; `labc` and `lab-opt` for compiler internals |
| [0008: Editor architecture](language/decisions/0008-editor-architecture.md) | shared language and IDE cores behind native LSP and WebAssembly hosts |
| [0009: Properties and workflow signatures](language/decisions/0009-declaration-properties-and-workflow-signatures.md) | `name: value` declaration properties and explicit workflow interfaces |
| [0010: Standard-library and inventory contracts](language/decisions/0010-standard-library-contracts-and-inventory-identities.md) | module-provided operations and typed external identities rather than domain grammar |
| [0011: Dependencies from material dataflow](language/decisions/0011-dependencies-from-material-dataflow.md) | build graphs derived from checked workflow values rather than biological level labels |
| [0012: Named workflow results](language/decisions/0012-named-workflow-results.md) | explicit named result fields and direct comma-separated returns without synthetic wrapper records |
| [0013: Strain artifacts](language/decisions/0013-strain-artifacts.md) | engineered organisms as first-class artifacts rather than a host property on a plasmid |
| [0014: Target profiles and workspaces](language/decisions/0014-target-profiles-and-workspaces.md) | benches configured by target profile, science stated in source, packages grouped by workspace |
| [0015: Roles classify types](language/decisions/0015-roles-classify-types.md) | types gain capabilities through declared roles rather than hardcoded bounds |
| [0016: Callable circuit signatures](language/decisions/0016-callable-circuit-signatures.md) | circuits declare callable signatures with inline type parameters |
| [0017: Forgotten type arguments](language/decisions/0017-forgotten-type-arguments.md) | a type argument may be deliberately forgotten with `any Role` |
| [0018: Standard modules in Lab](language/decisions/0018-standard-modules-written-in-lab.md) | standard modules may be written in Lab itself rather than Rust |
| [0019: Properties written with `=`](language/decisions/0019-properties-are-written-with-equals.md) | `name = value` for properties, disambiguating property from field |
| [0020: Laws are declared roles](language/decisions/0020-laws-are-declared-roles.md) | compiler-enforced laws as a closed, prelude-only set of roles |
| [0021: Typed external identities](language/decisions/0021-typed-external-identities.md) | catalogued identities as declarations rather than constructor calls |
| [0022: Fixed grammar, open vocabulary](language/decisions/0022-fixed-grammar-open-vocabulary.md) | domain nouns leave the parser; packages supply the vocabulary |
| [0023: Required fields and optional marks](language/decisions/0023-required-fields-and-optional-marks.md) | schema fields are required unless marked optional with `?` |
| [0024: Catalogued items carry properties](language/decisions/0024-catalogued-items-carry-properties.md) | a catalogued item states the fields of its type, like a datasheet |
| [0025: Quantity types](language/decisions/0025-quantity-types.md) | a quantity type names the unit it is measured in |
| [0026: Lineage and replicates](language/decisions/0026-lineage-and-replicates.md) | replicate class is lineage recovered from dataflow, not a property |
| [0027: Provenance is stated per thing](language/decisions/0027-provenance-is-stated-per-thing.md) | provenance is a fact about a thing, not about its type |
| [0028: Schemas are contributed to](language/decisions/0028-schemas-are-contributed-to.md) | several packages describe one artifact kind |
| [0029: Backend dispatch](language/decisions/0029-backend-dispatch.md) | a profile's `backend` key selects its backend; a registry stays deferred |
| [0030: Reviewed frames are the execution boundary](language/decisions/0030-reviewed-frames-are-the-execution-boundary.md) | the runtime interprets reviewed run documents and never plans |
| [0031: Workcell targets](language/decisions/0031-workcell-targets.md) | a workcell target composes stations; assignment is planning, not language |
| [0032: Provenance blocks](language/decisions/0032-provenance-blocks.md) | a provenance verb can open a block |
| [0033: Typeset protocol documents](language/decisions/0033-typeset-protocol-documents.md) | protocol documents are typeset PDFs emitted beside their sources |
| [0034: The simulator is an interpreter](language/decisions/0034-the-simulator-is-an-interpreter.md) | `lab simulate` interprets the same run documents `lab run` executes; the trace is the visualization contract |
| [0035: Facility files](language/decisions/0035-facility-files.md) | a facility is its own file under `facilities/`; the manifest carries at most a pointer |
| [0036: Photoreal projections](language/decisions/0036-photoreal-projections.md) | renderers are players of the scene and trace; assets are facility-owned references with box fallbacks |
| [0037: Robot learning as a physics projection](language/decisions/0037-robot-learning-is-a-physics-projection.md) | reviewed handoffs project to semantic robot tasks; embodiment and physics remain explicit simulator bindings |
| [0038: C3 as the primary compute provider](language/decisions/0038-c3-is-the-primary-compute-provider.md) | C3 runs finite training jobs behind provider-neutral lifecycle and artifact contracts; Isaac uses L40 capacity |

## Implementation and embedding

- [LAIR overview](../crates/lab-compiler/src/lair/dialect/README.md) introduces the multi-layer intermediate representation used to lower biological intent toward laboratory execution.
- [Protocol IR](../crates/lab-compiler/src/lair/dialect/protocol/README.md) describes the current target-selected biological-procedure boundary and what deliberately remains for resource and hardware lowering.
- [Compiler internals](../crates/lab-compiler/README.md) describes the current compiler pipeline and developer commands.
- [Language frontend](../crates/lab-language/README.md) describes the source-preserving and checked frontend boundaries.
- [Project CLI](../crates/lab-cli/README.md) documents the current `lab` project loop.
- [Compute control plane](../crates/lab-compute/README.md) documents the C3-first batch job boundary.
- [VS Code and Cursor](../editors/vscode/README.md) documents editor extension development.
- The [`lab-compiler`](../crates/lab-compiler/README.md) crate is the Rust embedding API; the [Python SDK](../crates/lab-python/README.md) exposes the same checked frontend through PyO3.
- [Lab-native Opentrons build specialization](integrations/opentrons-build.md) records the source, dependency, and hardware-lowering boundary for manual and OT-2 output.
- [Isaac Lab plate-transfer prototype](../integrations/isaac-lab/README.md) projects a checked workcell handoff into a manager-based RL environment without conflating workflow simulation and physics episodes.

## Examples versus specimens

Files under `language/specimens/` are representative language programs used to drive syntax and semantic design. They are compiler-tested, but a specimen may describe runtime behavior that has not been built yet.

The [Golden Gate example](../examples/golden-gate/README.md) is the end-to-end one: a package that compiles through every currently runnable toolchain path, from designs and workflows to the OT-2 protocols a robot application can open.
