# Modules, packages, and project organization

## Imports

`use` names a module, not a filesystem path and not a list of selected values:

```lab
use std.bio.parts
use std.lab.plasmid_actions
use my_lab.policies.plasmid_acceptance
```

The provisional rule is that a whole-module import makes that module's public
names available in the importing module. Ambiguous names must be diagnosed;
import order must not decide which declaration wins. Selective imports, aliases,
visibility, package manifests, and version resolution remain open decisions.

`std` is the language-owned standard library namespace. Biological catalogs,
laboratory integrations, and organization-specific policies should normally be
separate versioned packages rather than silently entering `std`.

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
- `programs` wires designs, policies, parameters, and workflows into runnable
  entry points.
- `.lab/runs` is generated runtime state and provenance, never hand-authored
  source.

These names are conventions rather than keywords. The module system should not
give a directory magical semantics merely because it is called `workflows`.
