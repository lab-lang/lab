# Lab documentation

This directory records what Lab is becoming, what has been decided, and what the current implementation can honestly do.

If you are new to the language, start with the [repository introduction](../README.md), then read the two representative programs:

- [plasmid design](language/specimens/plasmid-design.lab) shows typed biological composition, requirements, and evidence-based acceptance;
- [plasmid construction](language/specimens/plasmid-build.lab) shows durable actions, explicit state, reactive timers, structured outcomes, and affine material flow.

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

## Implementation and embedding

- [LAIR overview](../crates/lab-compiler/src/ir/README.md) introduces the multi-layer intermediate representation used to lower biological intent toward laboratory execution.
- [Protocol IR](../crates/lab-compiler/src/ir/protocol/README.md) describes the current target-selected biological-procedure boundary and what deliberately remains for resource and hardware lowering.
- [Compiler internals](../crates/lab-compiler/README.md) describes the current compiler pipeline and developer commands.
- [Language frontend](../crates/lab-language/README.md) describes the source-preserving and checked frontend boundaries.
- [Project CLI](../crates/lab-cli/README.md) documents the current `lab` project loop.
- [VS Code and Cursor](../editors/vscode/README.md) documents editor extension development.
- [Rust SDK](../crates/lab-sdk) and [Python SDK](../crates/lab-python/README.md) expose experimental compiler APIs.

## Examples versus specimens

Files under `language/specimens/` are representative language programs used to drive syntax and semantic design. They are compiler-tested, but a specimen may describe runtime behavior that has not been built yet.

Files under [`examples/`](../examples/README.md) exercise currently runnable toolchain paths. In particular, the [plasmid acceptance example](../examples/plasmid-acceptance/README.md) walks through the narrower executable artifact pipeline and its current limits.
