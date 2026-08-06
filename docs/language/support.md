# Language support

Support is tracked by compiler phase. `Lower` means verified portable module IR unless a row explicitly names a target. `Execute` distinguishes generated or legacy executable artifacts from the still-missing durable workflow runtime.

| Feature | Parse | Resolve | Type | Lower | Execute |
| --- | --- | --- | --- | --- | --- |
| Indented plasmid declaration | yes | yes | yes | portable module | specialized targets |
| Declarative plasmid properties with `:` | yes | expressions | inferred checked values | `CheckedProperty` | target-dependent |
| Mandatory workflow `(inputs) -> T` or `-> (name: T, ...)` signature | yes | yes | inputs, result arity, names, and types | yes | runtime pending |
| Quantity literals | acceptance subset | built-in units | dimension subset | yes | yes |
| `require` predicates | topology subset | yes | yes | yes | yes |
| `accept` predicates | sequence/concentration/volume | yes | yes | yes | yes |
| Bundled `std` module imports | yes | five modules | module values and contracts | yes | no runtime dispatch |
| Typed inventory constructors | yes | `std.bio.inventory` | nominal values | structured calls | no live inventory lookup |
| Heterogeneous list union inference | yes | symbols | e.g. `List<Plasmid | Part>` | yes | target-dependent |
| Project/package import graph | yes | module paths | imported module interfaces | yes | no |
| `lab.toml` manifest and source discovery | n/a | yes | n/a | yes | no |
| Path dependency resolution and lockfile | n/a | recursive path packages | imported module interfaces | `.lab/build/` index plus `lab.lock` | no |
| Registry dependency acquisition | n/a | rejected | no | no | no |
| Circuit declarations and applications | yes | yes | yes | yes | no |
| Top-level pure bindings | yes | yes | yes | yes | no |
| `record`, `material`, `observation`, `evidence`, and `event` | yes | yes | yes | yes | no |
| Biological `part` declarations | syntax pending | no | no | no | no |
| Tagged `outcome` declarations and constructors | yes | yes | yes | yes | no |
| Workflow declarations and calls | yes | yes | yes | yes | runtime pending |
| Pure workflow bindings | yes | yes | yes | yes | no |
| Explicit durable workflow `state` | yes | yes | yes | yes | no |
| Built-in durable operations with `<-` | yes | yes | yes | yes | no |
| Structured typed expression IR | n/a | yes | yes | yes | no |
| Action capability and ownership contracts | n/a | built-ins | built-ins | yes | no |
| Direct `return value, ...` and result checking | yes | yes | arity and per-result type | named result fields | no |
| `match` / `case` with continuing-branch bindings | yes | yes | yes | yes | no |
| `if` / `else` and `for` / `in` | yes | yes | yes | yes | no |
| `when every` / `when after` | yes | yes | yes | yes | no |
| Event emission | yes | yes | yes | yes | no |
| Affine material-flow checking in portable workflows | n/a | action ownership modes | yes | yes | no |
| Dependencies from `Material<Plasmid>` dataflow | yes | resolved `realize` operands | yes | initial OT-2 target | generated plans and bundles |
| OT-2 properties and operation sequence | yes | checked properties and actions | target validation | automation IR | generated protocols only |
| Human instruction package | n/a | n/a | target-validated | Markdown plus manifest | operator review required |
| Durable workflow runtime | no | no | no | no | no |

All complete source modules use the portable-module boundary. A backend may reject checked properties or operations it cannot preserve, but it cannot select a narrower source frontend.

`lab` resolves same-package modules and recursive path dependencies into one deterministic compilation order, detects package and module cycles, and checks an optional semver requirement against each path dependency's manifest. Each package compiles against the checked `ModuleInterface` values of its dependencies, so imported public symbols resolve and type-check across package boundaries. `lab build` writes portable module IR plus a package index under `.lab/build/` and a `lab.lock` recording each package's name, version, source, and dependency aliases. Registry dependencies fail closed: acquisition, integrity, caching, and visibility rules are unimplemented, and a manifest that declares one is rejected rather than silently ignored.

The separate OT-2 specialization accepts plasmid properties (`backbone`, ordered `components`, `restriction_enzyme`, `host`, `selection`, and batch controls) as checked symbol references plus workflows composed from bundled standard-library effects. `std.bio.inventory` constructors give external inventory identities typed source names; strings are not used as component references. The source selects `realize`, provision, transformation, recovery, dilution, and plating operations. Dependencies are typed material inputs to `realize`; the generic language does not encode assembly levels. The specialization emits a deterministic Lab manifest, human instructions, and OT-2 scripts, and explicitly rejects properties or operation sequences it cannot lower.

The target validates fixed reaction volumes and single-plate, rack, and tip capacity. It does not yet resolve SBOL, inventory lots, overhang compatibility, sequence redesign, concentration normalization, dependent-batch DNA preparation, or runtime acceptance evidence. Generated instructions and scripts require laboratory review and qualification before physical execution. The complete specialization boundary is documented separately in [`../integrations/opentrons-build.md`](../integrations/opentrons-build.md).

## Editor support

| Capability | Current support |
| --- | --- |
| Source-aware diagnostics | byte-spanned syntax, semantic, and material-flow diagnostics |
| Parse recovery | one syntax diagnostic; multi-error recovery pending |
| Outline | top-level declarations plus data fields/cases and workflow input/result fields |
| Completion and hover | keywords and open-document top-level declarations |
| Definition, references, rename | open documents; name-based fallback pending symbol identities/scopes |
| Semantic highlighting | parsed declaration kinds before lexical fallback; comments, keywords, strings, numbers, types, functions, values, operators |
| Formatting | trailing whitespace and final newline only |
| Native editor transport | LSP over stdio |
| Browser/embedded API | `wasm-bindgen` facade over the same `lab-ide` workspace |
