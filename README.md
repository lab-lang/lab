# Lab 🧪

Lab is a programming language and compiler toolchain for describing biology and orchestrating work in the laboratory.

It brings biological designs, typed materials, evidence, durable actions, and reactive workflows into one expressive language. A Lab program should read like the scientific work it performs while remaining precise enough to type-check, resume safely, and verify the flow of every physical sample.

## Programmable biology needs programmable infrastructure

Lab's objective is to become a foundation for a world where laboratories operate more like data centers. Researchers should be able to describe the biological result and the constraints that matter, while software compiles, schedules, executes, and observes the work on the infrastructure that is actually available.

Today, laboratory protocols are commonly coupled to a particular site, operator, instrument, or collection of handwritten steps. That makes automation brittle and makes scientific intent difficult to preserve when work moves between a manual bench, a liquid handler, a cloud lab, or a different hardware fleet.

Lab separates those concerns. A program can say that DNA must be assembled, transformed, screened, sequenced, and accepted without prematurely fixing every well, transfer, deck position, instrument model, or clock time. The compiler progressively specializes that intent for a concrete laboratory target. A target may choose a different valid assembly method, combine automated and human work, or reject the program because it cannot satisfy the required capabilities and scientific acceptance contract.

The intended result is one portable biological program that can be adapted to many laboratory configurations without erasing what the scientist meant.

## The compiler

The core compiler project is a progressive lowering stack. Each layer answers a different question and retains the biological, physical, and evidentiary meaning needed by the layer below it.

| Layer | Question answered | Status |
| --- | --- | --- |
| Lab source and portable module IR | What biological artifact and durable workflow does the author intend? | initial implementation |
| Design IR | What target-independent artifact, requirements, and acceptance obligations must be preserved? | initial plasmid vertical slice |
| Protocol IR | Which biological procedure satisfies that intent using the target lab's capabilities and policies? | initial target-selected vertical slice |
| Workflow and resource lowering | Which materials, containers, inventory lots, locations, human tasks, quantities, and dependencies realize the procedure? | planned |
| Scheduling and hardware lowering | Which instruments perform each operation, with what labware, transfers, deck layouts, timing, batching, and concurrency? | planned |
| Execution operations | Which idempotent commands are dispatched, and which completions, measurements, failures, and provenance events are recorded? | planned |

This multi-layer representation is called **LAIR**, the Lab Automation Intermediate Representation. It is a family of dialects rather than one flat protocol format. High layers remain portable and expressive; lower layers become increasingly concrete until operations can be handed to device adapters, people, or external services.

A laboratory target will eventually describe more than a list of instruments. It must capture capabilities and preferences, supported operation primitives, labware and deck geometry, inventory and locations, capacity, scheduling constraints, site policy, and the adapters through which work is dispatched. Target selection and lowering should be deterministic and inspectable: users must be able to see why a method or resource was selected and why another target cannot run the program.

The durable runtime is the corresponding control plane. It records commands and events, resumes workflows without repeating completed physical actions, reacts to timers and observations, maintains material custody, and preserves the full lineage from high-level intent to measured outcome. The compiler decides what can and should run; the runtime makes that decision reliable in a physical, failure-prone world.

In the data-center analogy:

- Lab programs are portable workloads;
- laboratory profiles are execution targets;
- instruments, people, inventory, and space are heterogeneous resources;
- LAIR is the progressive planning and lowering boundary;
- the runtime is the scheduler and control plane;
- the event journal and evidence graph are the operational record.

Biology is not ordinary compute—materials are consumable, results are probabilistic, operations are slow, and failures can be irreversible. Those differences are precisely why this needs a language, compiler, and runtime built for the laboratory rather than a collection of device scripts.

## The Lab programming language

Lab keeps its kernel small and gives each construct a clear laboratory meaning:

| Form | Meaning |
| --- | --- |
| `circuit`, `plasmid` | reusable biological organization and artifact design |
| `material`, `observation`, `evidence`, `outcome` | laboratory values with distinct semantic laws |
| `workflow` | a durable, replayable orchestration |
| `state` | explicit durable memory owned by a workflow instance |
| `=` | deterministic evaluation or state transition |
| `<-` | a durable physical or external effect |
| `require` | a property that must hold before construction |
| `accept` | a claim that runtime evidence must support |
| `when every`, `when after` | reactive behavior driven by durable events |

Verbs such as `synthesize`, `assemble`, `sequence`, `store`, and `dispose` are typed library actions rather than keywords. Their contracts describe required capabilities, argument and result types, and whether each physical input is copied, borrowed, or consumed.

Physical `Material<T>` values are affine. They cannot be duplicated implicitly, lost on a terminating path, or used after an action takes ownership. This makes sample identity and custody part of ordinary compilation rather than a comment or runtime convention.

### Lab Example

```lab
use std.bio.parts
use std.bio.backbones

circuit regulated_expression<I: Signal, O: Protein>:
  input promoter: Promoter<I>
  input coding: CDS<O>
  output Circuit<I, O>

  layout:
    promoter
    B0034
    coding
    B0015

tet_reporter = regulated_expression(pTet, sfGFP)

plasmid p_tet_reporter:
  backbone = p15A_kan
  cargo = tet_reporter

  require topology == circular
  require sites(BsaI) == 0

  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
```

```lab
workflow build_plasmid:
  input design: Plasmid
  output Accepted<Plasmid> | Rejected<Plasmid>

  fragments <- synthesize design
  construct <- assemble fragments
  cells <- provision competent_ecoli
  culture <- transform construct into cells
  plate <- plate culture on kanamycin

  colony_result <- await_colonies plate

  match colony_result:
    case TimedOut:
      <- dispose colony_result.plate
      return Rejected{
        material: None,
        reason: no_colonies,
        evidence: colony_result.observations,
      }

    case Ready:
      candidates <- pick 4 isolated colonies from colony_result.plate
      <- dispose colony_result.plate
      screening <- screen candidates against design
```

## Why a language?

Lab is not intended to be a thin laboratory API embedded in a general-purpose language. The compiler needs to understand concepts that ordinary function calls do not capture:

- biological intent and pre-construction requirements;
- future acceptance claims and the evidence needed to evaluate them;
- physical identity, ownership, and custody across control flow;
- durable effects that must not repeat when a workflow resumes;
- timers, observations, instrument results, and human decisions as recorded events;
- portable workflow meaning before a laboratory or hardware configuration is selected;
- progressive lowering decisions that remain explainable and scientifically auditable.

The goal is a language that is fun to write and easy to read, but unusually serious about what happened to the actual material.

## Try the current prototype

Development requires Rust 1.95 or newer.

`lab` is the tool for projects and laboratory workflows. Install `lab` with:

```sh
cargo install --path crates/lab-cli --locked
```

Cargo installs `lab` into its binary directory, normally `~/.cargo/bin`. Rust's standard installer usually adds this directory to `PATH`. While developing the CLI, reinstall the latest local version with `cargo install --path crates/lab-cli --locked --force`.

Once you have `lab` installed, you can use it to create and build projects:

```sh
lab new my-lab-project
lab check my-lab-project
lab build my-lab-project
```

You can also check and build the repository's starter package:

```sh
lab check examples/starter-package
lab build examples/starter-package
```

Build output lives under `.lab/build/` and contains a deterministic package index plus typed portable module IR.

Compiler contributors can inspect individual frontend and lowering boundaries with `labc`:

```sh
labc docs/language/specimens/plasmid-build.lab --emit module-ir
```

## Editor support

The VS Code/Cursor extension combines a TextMate grammar with semantic editor support from `lab-language-server`: diagnostics, completion, hover, definitions, references, rename, document symbols, semantic highlighting, and conservative formatting.

The editor engine itself is protocol-neutral. Native editors use it through LSP, while browser and embedded desktop editors can use the same API through WebAssembly.

See [editors/vscode](editors/vscode/README.md) for local development.

## Project status

Lab is an early prototype and its syntax, semantics, IRs, packages, and runtime will continue to evolve. Today the frontend parses and checks the representative plasmid design and reactive construction workflows, produces typed portable module IR, verifies action contracts and affine material flow, and provides the first editor APIs. The narrower executable plasmid pipeline already demonstrates Design IR, capability-aware method selection, target-selected Protocol IR, material-linearity verification, plan export, and symbolic simulation.

There is not yet a durable workflow runtime. Package dependency acquisition, imported public-symbol checking, multi-error parser recovery, fully scoped IDE navigation, incremental analysis, and a syntax-aware formatter are also still in progress. Unsupported behavior is kept explicit rather than silently approximated.

The [documentation index](docs/README.md) separates accepted language decisions, current compiler support, representative specimens, and open questions.

## Repository map

| Path | Responsibility |
| --- | --- |
| `crates/lab-language` | syntax, AST, checking, action contracts, material flow, diagnostics |
| `crates/lab-compiler` | lowering pipeline, backend IR, passes, `labc`, and `lab-opt` |
| `crates/lab-package` | manifests, source discovery, and module graphs |
| `crates/lab-cli` | the `lab` project and workflow experience |
| `crates/lab-ide` | protocol-neutral editor intelligence |
| `crates/lab-language-server` | native Language Server Protocol adapter |
| `crates/lab-ide-wasm` | browser and embedded editor bindings |
| `editors/vscode` | VS Code and Cursor integration |
| `crates/lab-sdk`, `crates/lab-python` | experimental embedding APIs |
| `docs`, `examples` | language design, decisions, specimens, and runnable examples |
