# Modules, packages, and project organization

## Imports

`use` names a module, not a filesystem path and not a list of selected values:

```lab
use std.bio.parts
use std.lab.plasmid_actions
use my_lab.policies.plasmid_acceptance
```

The provisional rule is that a whole-module import makes that module's public names available in the importing module. Ambiguous names must be diagnosed; import order must not decide which declaration wins. Selective imports, aliases, visibility, and version resolution remain open decisions.

`std` is the language-owned standard library namespace. Biological catalogs, laboratory integrations, and organization-specific policies should normally be separate versioned packages rather than silently entering `std`.

## Bundled standard-library surface

The current frontend resolves a small bundled registry through the same conceptual boundary that future packages should implement:

| Module | Current role |
| --- | --- |
| `std.bio.parts` | fixed demonstration part values used by the design specimen |
| `std.bio.backbones` | fixed demonstration backbone values used by the design specimen |
| `std.bio.inventory` | pure constructors for typed external part, backbone, enzyme, strain, and antibiotic identities |
| `std.bio.build` | typed artifact-realization effects |
| `std.lab.plasmid_actions` | typed laboratory action contracts used by the workflow specimens |

Module resolution supplies values, pure-function signatures, and action contracts to the generic checker. It does not add module-specific parser or AST cases. The bundled registry is an implementation bridge; changing catalogs and site-specific actions should move to ordinary versioned packages once package-defined public contracts exist.

## Project layout

An idiomatic project separates reusable intent from the runnable composition:

```text
lab.toml
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
  runs/
```

- `designs` holds reusable biological intent.
- `policies` holds site- or project-specific scientific acceptance decisions.
- `workflows` holds reusable durable orchestration.
- `programs` wires designs, policies, parameters, and workflows into runnable entry points.
- `.lab/runs` is generated runtime state and provenance, never hand-authored source.

These names are conventions rather than keywords. The module system should not give a directory magical semantics merely because it is called `workflows`.

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

[dependencies]
parts = "1.2"
local-policies = { path = "../policies" }
```

Source modules are discovered recursively beneath `src`. Their names are the normalized package name followed by their relative path, so `src/workflows/build-plasmid.lab` becomes `tet_reporter.workflows.build_plasmid`.

The manifest parser models version, path, and registry dependencies, but the initial CLI rejects a package with dependencies rather than silently ignoring them. Dependency graph resolution, integrity, lockfiles, caches, and importing their public symbols must land as one coherent package-resolution milestone.
