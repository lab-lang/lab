# Open language questions

These choices are intentionally not hidden behind parser behavior. A parsed form is not a settled semantic decision.

## Concurrent state transitions

Durable workflow memory is now explicit with `state`, and ordinary bindings are immutable. The remaining question is the transaction model when multiple ready handlers read and update the same state: strictly journal-ordered handlers are the initial semantics, but conflict diagnostics and future safe parallelism still need design work.

## Effect action grammar

The parser preserves a phrase-shaped action syntax after `<-`. The module compiler resolves bundled actions through typed contracts that declare stable operation identities, phrase slots, operand ownership, result types, and a dispatch capability. We still need source syntax, visibility rules, and package metadata for declaring non-standard actions through the same registry interface.

## Effect expressions

The explicit `<-` boundary currently makes durable external work visible and keeps `=` deterministic. It remains open whether effectful operations should also become typed expressions that can be passed to higher-order combinators. Any such system must preserve capability checking, material ownership, journaling, idempotent replay, failure, and cancellation; merely changing `<-` to `=` would erase information the runtime requires.

## Concurrency and cancellation

Sequential effects and independent `when` handlers are represented. Syntax for starting several physical actions together, joining them, races, timeouts, and explicit cancellation is not settled. Cancellation must distinguish stopping a subscription from attempting to cancel an already-dispatched physical action.

## Declaring pure functions and action contracts in source

A standard module written in Lab can declare roles, membership, data types, artifact kinds, and catalogued items; `std.bio.designs`, `std.bio.golden_gate`, and `std.bio.parts` are written that way. Two things have no source declaration form, and a module needing either stays in Rust: pure functions such as `dna` and `sites`, and durable action contracts.

## Parts and biological catalogs

A catalogued item is declared with `buy` against an imported kind, states the fields of its type, and names its own type where its kind is generic — `buy promoter pTet: Promoter<Tetracycline>` — so the biological catalog is written in Lab.

What a kind *is* now travels with it: a role may name an ontology term and a kind plays roles, so `Plasmid` states that it is a nucleic acid and an engineered region ([`0039`](decisions/0039-roles-carry-ontology-terms.md)). A sequence can now be declared as a named `DNA` value and referenced from one or more designs ([`0043`](decisions/0043-sequences-are-first-class-design-values.md)). Exact identity is no longer ambiguous: `sbol_identity` names an SBOL Component and `supplier_identity` names a supplier order line. What remains open is the catalog record around that value: its provenance chain and version, whether its sequence was asserted or derived, and how biological catalogs expose those richer declarations without compiling changing catalog contents into `std`. The intended direction is recorded in [`sbol.md`](sbol.md).

## Target contracts

A kind now declares a schema, so the language states which properties an artifact
may hold and what each contains. What it still cannot state is which of them a
*target* consumes: the OT-2 backend reads `reaction_volume` and
`digest_temperature` by name, and a schema gives it something to validate against
without telling it what to expect. This is why moving `plasmid` into
`std.bio.designs` removes biology from the frontend and not from the toolchain.

Schema composition is also unresolved. A kind cannot extend or refine another, so
a target-specific chemistry schema has no way to say it adds to the design one.

## Property schemas and target contracts

Artifact properties are backend-neutral typed expressions, while the initial OT-2 specialization requires a documented property set. Packages still need a way to declare reusable property schemas, defaults, refinements, and target capability contracts. This should allow a target to state what it consumes without adding experiment-specific property names or diagnostics to the core checker.

Reaction chemistry is the sharpest case. A design states `reaction_volume: 20 uL`, and the OT-2 target interprets it, but nothing in the language says which properties a Golden Gate assembly requires or what their units must be. The unit check lives in the target's lowering rather than in a declared schema, so a target that wanted the same parameters would restate them.

## Target profiles and backend selection

A target profile configures one backend for one bench, and `lab build --target` resolves it by filename under `targets/`. The profile's `backend` field is validated but not dispatched on: there is one backend, and it is named concretely. A second backend needs a registry, a way for a profile to select among installed backends, and a rule for what a program may assume about a target it has not been compiled for.

Profile composition is also unresolved. Sites that share most of a layout have no way to express one profile in terms of another, and nothing distinguishes a capability a bench has from a choice its operator made.

## Inventory identity, availability, and provenance

Design identity and physical availability are now separate. `sbol_identity` names an exact SBOL Component; target planning loads a validated SBOLInventory document, restricts active MaterialLots to the selected facility, joins them through `sbol:built`, rejects ambiguity, and freezes the selected lot together with the facility and document hash. Quantity, expiration, containment, reservation, allocation policy beyond refusing ambiguity, trust policy, and asynchronous availability remain open.

## Package resolution

Whole-module `use` syntax and five bundled `std` modules resolve. Path dependencies resolve recursively, import their public symbols through checked module interfaces, diagnose cycles, honor a semver requirement, and produce a lockfile.

What remains unspecified is everything a registry implies: dependency acquisition, integrity verification, caches, version selection across a graph with conflicting requirements, and symbol visibility rules that let a package export less than everything it declares. The boundary between `std` and versioned biological catalogs is also unsettled, and it constrains the rest: a catalog that ships as an ordinary package needs the same visibility and versioning answers.
