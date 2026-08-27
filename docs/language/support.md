# Language support

Support is tracked by compiler phase. `Lower` means verified portable module IR unless a row explicitly names a target. `Execute` distinguishes generated or legacy executable artifacts from the still-missing durable workflow runtime.

| Feature | Parse | Resolve | Type | Lower | Execute |
| --- | --- | --- | --- | --- | --- |
| Package-declared artifact kinds (`artifact Type:`) | yes | imported kinds | schema fields and `declares` | `CheckedDeclaration::ArtifactKind` and the interface schema | n/a |
| Artifact instances naming a package's word | yes | resolved against imported kinds | properties against the schema | portable module | specialized targets |
| `declares` completeness rules | yes | property names only | presence, not values | `CheckedPresence` | n/a |
| Optional schema fields (`name?:`) | yes | yes | required unless marked | `CheckedSchemaField.optional` | n/a |
| Bought-item properties (`buy`) | yes | yes | against the kind's schema, strictly | `CheckedDeclaration::Catalog.properties` | n/a |
| Quantity types (`Quantity<uL>`) | yes | yes | unit-exact | `CheckedType::Quantity` | n/a |
| `across N biological replicates` | yes | yes | count resolved, and evidence checked against the lineage it spans | `CheckedAcceptance.replicates` | n/a |
| Material lineage and replicate class | n/a | n/a | derived from action results | `provenance::lineage` | n/a |
| Provenance verbs (`build`, `buy`) | yes | yes | `require`/`accept` only on what is built; supplier identity only on what is bought; SBOL identity on either | `CheckedDeclaration::Artifact` and `Catalog` | manifest cross-check at build |
| Schemas contributed to by several modules | yes | yes | union of every kind declaration in scope | the merged interface schema | n/a |
| Reagent-owned chemistry with design override | yes | yes | a stated value wins over the item's | `CheckedDeclaration::Catalog.properties` | read by the lowerer |
| Declarative artifact properties with `=` | yes | expressions | inferred checked values | `CheckedProperty` | target-dependent |
| Quantity-valued chemistry properties | yes | yes | unit-checked at lowering | chemistry dictionaries | generated protocols |
| Mandatory workflow `(inputs) -> T` or `-> (name: T, ...)` signature | yes | yes | inputs, result arity, names, and types | yes | runtime pending |
| Quantity literals | any expression position | built-in units | dimension subset | yes | yes |
| `//` comments, `/** */` declaration and `/*! */` module documentation | yes | attached to the declaration below or to the module | n/a | module and declaration docs in the portable module and its interface | n/a |
| `require` predicates | topology subset | yes | yes | yes | yes |
| `accept` predicates | sequence/concentration/volume | yes | yes | yes | yes |
| Bundled `std` module imports | yes | eight modules | module values and contracts | yes | no runtime dispatch |
| Bundled `std` modules written in Lab | yes | `designs`, `golden_gate`, `parts`, `backbones`, `reporters` | compiled once at startup | resolved through `ModuleInterface` | n/a |
| Optional trailing action clauses | yes | contract-driven | omitted operand binds to the empty list | yes | n/a |
| Typed external identities (`buy`) | yes | against imported kinds | nominal values | `CheckedDeclaration::Catalog` | no live inventory lookup |
| Heterogeneous list union inference | yes | symbols | e.g. `List<Plasmid | Part>` | yes | target-dependent |
| Project/package import graph | yes | module paths | imported module interfaces | yes | no |
| `lab.toml` manifest and source discovery | n/a | yes | n/a | yes | no |
| `[workspace]` members and default member | n/a | member packages | n/a | shared `.lab/build/` and `lab.lock` | no |
| Path dependency resolution and lockfile | n/a | recursive path packages | imported module interfaces | `.lab/build/` index plus `lab.lock` | no |
| Multi-module program lowering | n/a | whole program | n/a | one Design/Workflow module | n/a |
| `targets/*.toml` site profiles | n/a | n/a | n/a | validated deck, labware, instruments | `lab build --target` or `[build] target` |
| Registry dependency acquisition | n/a | rejected | no | no | no |
| `role` declarations and `is` membership | yes | yes | bounds satisfied by role membership | `CheckedDeclaration::Role`, roles on type exports | n/a |
| Roles crossing a module boundary | n/a | `ExportKind::Role` | membership restored from the interface | yes | n/a |
| Ontology grounding | `role X = "SO:0000167"`, `artifact P is X` | role terms and kind membership | term shape checked where written | `Grounding` resolves a type to its terms | not yet read by a target |
| Designs read from SBOL | an SBOL document in place of `.lab` designs | catalogued declarations built and then checked | the same rules a written design meets | `lab-sbol` reads components, sequences, and ordered parts | file discovery pending |
| Circuit declarations and applications | yes | yes | yes | yes | no |
| Callable circuit signatures with `-> T` | yes | yes | yes | yes | no |
| Inline type parameters (`Promoter<S: Signal>`) | yes | yes | harvested in signature order, bounds checked at the call | `parameters` and `bounds` in the portable module and its interface | n/a |
| Header type parameters on data declarations | yes | yes | arity and bounds checked where the type is used | yes | n/a |
| Generic workflows and their calls | yes | yes | operands unify, results substitute | yes | runtime pending |
| Forgotten type arguments (`any Role`) | type-argument position only | yes | packing only where an annotation asks | `CheckedType::Any` | n/a |
| Diagnostics with secondary spans and help | n/a | n/a | n/a | `Diagnostic.related` and `.help` | rendered by `lab check` on one file, and by the language server |
| Top-level pure bindings | yes | yes | yes | yes | no |
| Named DNA values referenced by designs | `name: DNA = dna("...")` | references resolve across modules | DNA-typed design property | one reusable `design.dna_sequence` SSA value | target-dependent |
| `record` plus role membership (`is Event`, `is Evidential`) | yes | yes | yes | yes | no |
| Exact SBOL Component and supplier identities | `sbol_identity`, `supplier_identity` | declarations resolve normally | SBOL identity must be an absolute IRI | separate fields in `lab.portable-module.v5` | inventory binding pending |
| Biological catalog version and provenance chains | syntax pending | no | no | no | no |
| Tagged `record` declarations with `case` constructors | yes | yes | yes | yes | no |
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
| Dependencies from `Material<Plasmid>` dataflow | yes | resolved `realize` and `transform` operands | yes | initial OT-2 target | generated plans and bundles |
| OT-2 properties and operation sequence | yes | checked properties and actions | target validation | automation IR | generated protocols only |
| Multi-plate allocation across declared slots | n/a | n/a | n/a | plate-and-well addresses | generated protocols |
| Human instruction package | n/a | n/a | target-validated | Markdown plus manifest | operator review required |
| Durable workflow runtime | no | no | no | no | no |

All complete source modules use the portable-module boundary. A backend may reject checked properties or operations it cannot preserve, but it cannot select a narrower source frontend.

`lab` resolves workspace members, same-package modules, and recursive path dependencies into one deterministic compilation order, detects package and module cycles, and checks an optional semver requirement against each path dependency's manifest. Each package compiles against the checked `ModuleInterface` values of its dependencies, so imported public symbols resolve and type-check across package boundaries. A package that declares a build entry must declare `workflow main` in that module; one that declares no entry is a library and is accepted without it. `lab build` writes portable module IR plus a package index under `.lab/build/` and a `lab.lock` recording each package's name, version, source, and dependency aliases. Registry dependencies fail closed: acquisition, integrity, caching, and visibility rules are unimplemented, and a manifest that declares one is rejected rather than silently ignored.

The separate OT-2 specialization accepts plasmid properties (`backbone`, ordered `components`, `restriction_enzyme`, replicate counts, and reaction chemistry) and strain properties (`chassis`, carried `plasmids`, `selection`, replicate counts, and transformation chemistry) as checked symbol references, plus workflows composed from bundled standard-library effects. `buy` declarations give external inventory identities typed source names; strings are not used as component references. The source selects `realize`, provision, transformation, recovery, dilution, and plating operations. Dependencies are typed material inputs to `realize` and `transform`; the generic language does not encode assembly levels. The specialization emits a deterministic Lab manifest, human instructions, and OT-2 scripts, and explicitly rejects properties or operation sequences it cannot lower.

Deck layout, labware, instruments, and per-stage capacity come from a target profile rather than from constants, and allocation spills across every plate a profile declares. The target validates reaction balance against each design's own stated volume, replicate and dilution bounds, plate capacity, source-rack capacity, and tip capacity. A batch emits a robot protocol only for the stages its artifacts reach, and artifacts sharing a planning wave share one run.

It does not yet read the ontology terms a kind is grounded in, nor resolve SBOL, inventory lots, overhang compatibility, sequence redesign, concentration normalization, inter-wave DNA preparation, or runtime acceptance evidence. Generated instructions and scripts require laboratory review and qualification before physical execution. The complete specialization boundary is documented separately in [`../integrations/opentrons-build.md`](../integrations/opentrons-build.md).

## Editor support

| Capability | Current support |
| --- | --- |
| Source-aware diagnostics | byte-spanned syntax, semantic, and material-flow diagnostics |
| Parse recovery | one syntax diagnostic; multi-error recovery pending |
| Outline | top-level declarations plus data fields/cases and workflow input/result fields |
| Completion and hover | keywords and open-document top-level declarations, with the documentation each declaration carries; a `use` path hovers the module it imports |
| Package-aware module names | manifest-derived; a `use` of an unopened package sibling resolves |
| Definition, references, rename | open documents; name-based fallback pending symbol identities/scopes |
| Semantic highlighting | parsed declaration kinds before lexical fallback; comments, keywords, strings, numbers, types, functions, values, operators |
| Formatting | trailing whitespace and final newline only |
| Native editor transport | LSP over stdio |
| Browser/embedded API | `wasm-bindgen` facade over the same `lab-ide` workspace |
