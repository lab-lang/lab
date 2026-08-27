# Modules, packages, and project organization

## Imports

`use` names a module, not a filesystem path and not a list of selected values:

```lab
use std.bio.parts
use std.lab.plasmid
use my_lab.policies.plasmid_acceptance
```

The provisional rule is that a whole-module import makes that module's public names available in the importing module. Ambiguous names must be diagnosed; import order must not decide which declaration wins. Selective imports, aliases, visibility, and version resolution remain open decisions.

`std` is the language-owned standard library namespace. Biological catalogs, laboratory integrations, and organization-specific policies should normally be separate versioned packages rather than silently entering `std`.

## Bundled standard-library surface

The current frontend resolves a small bundled registry through the same conceptual boundary that future packages should implement:

| Module | Current role |
| --- | --- |
| `std.prelude` | implicitly imported foundational types, values, and pure operations used by every module |
| `std.bio.designs` | the artifact kinds — `plasmid`, `strain`, `part`, `backbone`, and the rest — and their schemas, written in Lab |
| `std.bio.golden_gate` | what Golden Gate assembly and heat-shock transformation need, contributed to the `plasmid` and `strain` schemas, written in Lab |
| `std.bio.parts` | catalogued demonstration parts and the roles they play, written in Lab |
| `std.bio.backbones` | catalogued demonstration backbones, written in Lab |
| `std.bio.reporters` | reporters and the readouts they produce, written in Lab |
| `std.bio.build` | typed artifact-realization effects |
| `std.lab.plasmid` | typed laboratory action contracts used by the workflow specimens |

Each bundled module owns one checked specification containing all of its exported types, values, pure-function signatures, and action contracts. Import resolution adds those exports to the generic checker scope and diagnoses ambiguous names. The checker does not maintain separate verb or constructor lookup tables, and adding a module does not add parser or AST cases.

`std.prelude` is the one explicit exception to source-level `use`: it supplies the foundational nominal types and currently unqualified operations such as `dna` to every module. Keeping that surface in a named module makes implicit language vocabulary inspectable and prevents it from accumulating as checker special cases.

The bundled catalog validates module paths, export uniqueness, stable operation identities, and action-contract structure when it is constructed. It remains an implementation bridge; changing catalogs and site-specific actions should move to ordinary versioned packages once package-defined public contracts exist.

### Standard modules written in Lab

A bundled module whose whole surface is expressible in Lab is written in Lab rather than in Rust, and resolves through the same checked `ModuleInterface` a package module does. `std.bio.designs`, `std.bio.golden_gate`, `std.bio.parts`, `std.bio.backbones`, and `std.bio.reporters` all are. Nothing in the checker distinguishes them from a Rust-defined module or from a package, and their own documentation comments become their reference entries, so the reference cannot drift from a second description of the same exports.

A module needing pure functions or durable action contracts stays in Rust, because neither has a source declaration form yet. [`open-questions.md`](open-questions.md) records what that blocks.

### Declaring kinds other packages use

A package declares a word and the schema its declarations are checked against:

```lab
artifact Plasmid:
  sequence: DNA
```

Any module importing that package may then write `build plasmid p_gfp:`. The parser
never learns the word — an unknown word followed by a name and a block is always
an artifact instance, and which kind it names is resolved while checking. That is
what keeps a lone file parseable and an editor able to recover from broken code,
and it is why no package may introduce a grammar production.

### Declaring roles other packages extend

A role is open. Declaring `role Signal` publishes a classification; any package that imports it may declare its own types as members with `is`, and every generic circuit or workflow bounded by that role then accepts them. A role therefore names a contract between packages rather than a closed enumeration its author must keep updating, which is why a role has no block listing its members.

Roles and types share one namespace, so a role cannot collide with a type of the same name, and both are registered before anything is lowered — a declaration may name a role declared further down its file.

## Project layout

An idiomatic project separates reusable intent from the runnable composition:

```text
lab.toml
targets/
  opentrons-ot2.toml
src/
  designs/
    parts.lab
    circuits.lab
    plasmids.lab
  policies/
    plasmid_acceptance.lab
  workflows/
    build_plasmid.lab
    colony_screening.lab
  programs/
    make_tet_reporter.lab
tests/
  build_plasmid.lab
.lab/
  build/
  runs/
```

- `designs` holds reusable biological intent.
- `policies` holds site- or project-specific scientific acceptance decisions.
- `workflows` holds reusable durable orchestration.
- `programs` wires designs, policies, parameters, and workflows into runnable entry points.
- `targets` holds site configuration: one file per bench a project compiles for.
- `.lab/` is generated output and runtime state, never hand-authored source.

These names are conventions rather than keywords, except `targets`, which `lab build --target <name>` resolves by path. The module system should not give a directory magical semantics merely because it is called `workflows`.

A program's modules are lowered together, so an artifact declared in `designs` may be realized by a workflow in `workflows`, and either may come from a dependency package.

## Workspaces

A `lab.toml` is either a package manifest or a workspace manifest, never both. A workspace root owns membership and nothing else, so every member stays an ordinary self-contained package:

```toml
[workspace]
members = ["packages/catalog", "packages/golden-gate"]
default-member = "packages/golden-gate"
```

`default-member` names the package a command acting on one package operates on, and is required once a workspace has more than one member. Generated artifacts and `lab.lock` live at the workspace root; each member keeps its own `src/`, dependencies, and version.

## Target profiles

A target profile describes one bench. `lab build --target opentrons-ot2`, or a `[build] target = "opentrons-ot2"` in the manifest, reads `targets/opentrons-ot2.toml` and hands it to the backend the profile names:

```toml
[target]
backend = "opentrons.ot2"
api_level = "2.21"

[stages.plating.agar_plate]
labware = "nest_96_wellplate_100ul_pcr_full_skirt"
slots = ["5", "6"]
```

A profile's filename is its name, so the file does not state one and cannot disagree with the name a build resolved it by. Emitted plans carry that name, so an operator reading a protocol can see which bench it was compiled for. `backend` names the backend that consumes the profile, spelled the one way that backend spells itself; a profile written for another backend is rejected rather than compiled.

Every field has a default, so a profile states only what differs from the backend's reference bench. Unknown keys are rejected rather than ignored: a misspelled slot that silently fell back to a default is how a protocol ends up aspirating from the wrong place.

Within a module, examples conventionally put providers before consumers: imports first, then shared data types, inventory values, biological declarations, and finally workflows. Dependency correctness still comes from resolved symbols and typed dataflow rather than textual order, filenames, or names such as “level 1” and “level 2.”

## Initial package manifest

`lab` discovers a project through `lab.toml`:

```toml
[package]
name = "tet-reporter"
version = "0.1.0"
edition = "2026"

[build]
entry = "src/programs/main.lab"
target = "opentrons-ot2"

[inventory]
document = "inventory/facility.ttl"
# Required only when the document contains more than one facility:
facility = "https://example.org/facilities/tet-lab"

[dependencies]
parts = "1.2"
local-policies = { path = "../policies" }
```

`[inventory] document` names a package-relative SBOLInventory document in Turtle, RDF/XML, JSON-LD, or N-Triples. Lab validates the complete SBOL 3 and SBOLInventory Profile 0.2 graph before planning. If `facility` is omitted, the document must contain exactly one facility; otherwise the absolute Facility IRI selects one exactly.

Each required source declaration reaches the graph through its exact `sbol_identity`, and availability means one active MaterialLot in the selected facility whose `sbol:built` points to that exact local Component. No declaration name, display ID, supplier identifier, or IRI prefix is used for matching. Zero lots leaves the dependency blocked, one freezes a Component-to-MaterialLot binding in `lab.dependency-build.v1`, and several produce an allocation ambiguity instead of a silent first choice. A built artifact with one active lot is reused through the same rule.

The old `materials` and `artifacts` arrays remain as a mutually exclusive legacy form while existing examples migrate. They retain symbolic behavior and are identified as `legacy_symbols` in emitted dependency manifests; new packages should use `document`.

`[build] target` names the profile a plain `lab build` compiles for, so the command a laboratory runs every day produces the protocols its robots execute rather than intermediate IR. It names a profile under `targets/` and nothing else: a value carrying a path separator is rejected. `--target` compiles for a different bench and `--no-target` stops at portable module IR, so a package that declares a default keeps both. A package that declares no default builds module IR alone.

Source modules are discovered recursively beneath `src`. Their names are the normalized package name followed by their relative path, so `src/workflows/build-plasmid.lab` becomes `tet_reporter.workflows.build_plasmid`.

A declared entry module must itself declare `workflow main`. Naming an entry is what makes a package a program, and a program states the work it runs rather than only importing the modules that could run it. Build order still comes from material dataflow, so `main` is where that dataflow starts, not an ordering directive. A package that declares no entry is a library and owes no `main`.

The manifest parser models version, path, and registry dependencies. Path dependencies resolve recursively into one deterministic compilation order, and a dependency's optional semver requirement is checked against its manifest. Each package compiles against the checked module interfaces of its dependencies, so a dependency's public symbols are available under its manifest alias: a `parts` dependency exposing `parts.designs.promoters` is imported as `use parts.designs.promoters`. Package and module cycles are diagnosed rather than resolved. `lab build` writes `lab.lock` alongside the manifest, recording each package's name, version, source, and dependency aliases.

Registry dependencies fail closed. Acquisition, integrity verification, caching, and visibility rules are unimplemented, and a manifest that declares a registry dependency is rejected rather than silently ignored.
