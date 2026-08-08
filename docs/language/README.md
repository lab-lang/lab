# Lab Language Design

This directory records the intended Lab language independently from the subset that the compiler can execute today.

The documents have distinct jobs:

- `syntax.md` records accepted surface-language rules;
- `semantics.md` records the meaning of laboratory values and effects;
- `modules.md` records package imports and idiomatic source organization;
- `open-questions.md` keeps unresolved design choices visible;
- `support.md` records how far each feature has progressed through the compiler;
- `decisions/` records design decisions and their status;
- `specimens/` contains representative programs used to test the design.

A specimen is not necessarily executable. The runnable example is the [Golden Gate package](../../examples/golden-gate/README.md) under the repository-level `examples/` directory. The support matrix is the authoritative statement of what `labc` currently accepts, checks, lowers, and executes.

The language is organized around three source-level concerns:

1. **Design** describes reusable biological intent such as parts, circuits, and plasmids.
2. **Workflow** describes durable transformations and reactions involving physical materials, instruments, people, observations, and time.
3. **Program** composes concrete designs, policies, and reusable workflows into a runnable entry point.

Actual executions are runtime records, not source modules. A program may be run many times, with each run receiving an independent identity and event journal.

## Specimen guide

| Specimen | Language boundary exercised |
| --- | --- |
| [`plasmid-design.lab`](specimens/plasmid-design.lab) | circuits, typed composition, declarative plasmid properties, requirements, and acceptance |
| [`sensor-panel.lab`](specimens/sensor-panel.lab) | roles, inline type parameters, a generic characterization workflow, and a panel that forgets which signal triggers it |
| [`plasmid-build.lab`](specimens/plasmid-build.lab) | workflow signatures, durable effects, explicit state, reactive handlers, outcomes, and affine materials |
| [`inventory-plasmid.lab`](specimens/inventory-plasmid.lab) | typed inventory identities, heterogeneous component lists, target-neutral properties, and one realization workflow |
| [`dependency-build.lab`](specimens/dependency-build.lab) | dependencies expressed as `Material<Plasmid>` workflow inputs and resolved `realize` operands |

Specimens define provider symbols before declarations that depend on them. This is a readability convention, not an assembly-level system and not a replacement for name resolution.

Inspect any syntax specimen without semantic checking:

```sh
labc docs/language/specimens/plasmid-build.lab --emit source-ast
```

Compile any representative specimen into verified portable module IR:

```sh
labc docs/language/specimens/plasmid-design.lab --emit module-ir
labc docs/language/specimens/sensor-panel.lab --emit module-ir
labc docs/language/specimens/plasmid-build.lab --emit module-ir
labc docs/language/specimens/inventory-plasmid.lab --emit module-ir
labc docs/language/specimens/dependency-build.lab --emit module-ir
```

Portable module compilation resolves and checks the program but does not select a laboratory target, schedule work, or dispatch physical actions.

The latest accepted design records are:

- [`0009`](decisions/0009-declaration-properties-and-workflow-signatures.md): declaration properties and callable workflow signatures;
- [`0010`](decisions/0010-standard-library-contracts-and-inventory-identities.md): module-provided contracts and typed external identities; and
- [`0011`](decisions/0011-dependencies-from-material-dataflow.md): dependency graphs derived from checked material dataflow;
- [`0012`](decisions/0012-named-workflow-results.md): named typed workflow results and direct multi-value returns;
- [`0013`](decisions/0013-strain-artifacts.md): engineered organisms as first-class artifacts;
- [`0014`](decisions/0014-target-profiles-and-workspaces.md): target profiles for benches and workspaces for packages;
- [`0015`](decisions/0015-roles-classify-types.md): roles classify types, and a role is not a type;
- [`0016`](decisions/0016-callable-circuit-signatures.md): circuits declare callable signatures with inline type parameters;
- [`0017`](decisions/0017-forgotten-type-arguments.md): a type argument may be deliberately forgotten; and
- [`0018`](decisions/0018-standard-modules-written-in-lab.md): standard modules may be written in Lab.
