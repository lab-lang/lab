# Open language questions

These choices are intentionally not hidden behind parser behavior. A parsed form is not a settled semantic decision.

## Concurrent state transitions

Durable workflow memory is now explicit with `state`, and ordinary bindings are immutable. The remaining question is the transaction model when multiple ready handlers read and update the same state: strictly journal-ordered handlers are the initial semantics, but conflict diagnostics and future safe parallelism still need design work.

## Effect action grammar

The parser preserves a phrase-shaped action syntax after `<-`. Packages can declare and export actions with stable operation identities, phrase slots, operand ownership, result types, and result-lineage rules. A checked call retains the exact declaration identity and source provenance, and generic Intent lowering does not need an action-specific compiler operation. It remains open whether one scope may overload the same leading action word and, if so, which complete-phrase rules make resolution deterministic and diagnostics intelligible.

## Effect expressions

The explicit `<-` boundary currently makes durable external work visible and keeps `=` deterministic. It remains open whether effectful operations should also become typed expressions that can be passed to higher-order combinators. Any such system must preserve capability checking, material ownership, journaling, idempotent replay, failure, and cancellation; merely changing `<-` to `=` would erase information the runtime requires.

## Concurrency and cancellation

Sequential effects and independent `when` handlers are represented. Syntax for starting several physical actions together, joining them, races, timeouts, and explicit cancellation is not settled. Cancellation must distinguish stopping a subscription from attempting to cancel an already-dispatched physical action.

## Declaring pure functions and dependent actions in source

A standard module written in Lab can declare roles, membership, data types, artifact kinds, catalogued items, and concrete durable action contracts. Pure functions such as `dna` and `sites` still have no source declaration form. Action declarations also cannot yet quantify a type variable or state an open relationship such as “return `Material<T>` for the kind of the supplied design.” A module needing either contract stays in Rust; both are exposed through the same checked module interface as source declarations.

## Parts and biological catalogs

A catalogued item is declared with `buy` against an imported kind, states the fields of its type, and names its own type where its kind is generic — `buy promoter pTet: Promoter<Tetracycline>` — so the biological catalog is written in Lab.

What a kind *is* now travels with it: a role may name an ontology term and a kind plays roles, so `Plasmid` states that it is a nucleic acid and an engineered region ([`0039`](decisions/0039-roles-carry-ontology-terms.md)). A sequence can now be declared as a named `DNA` value and referenced from one or more designs ([`0043`](decisions/0043-sequences-are-first-class-design-values.md)). Exact identity is no longer ambiguous: `sbol_identity` names an SBOL Component and `supplier_identity` names a supplier order line. What remains open is the catalog record around that value: its provenance chain and version, whether its sequence was asserted or derived, and how biological catalogs expose those richer declarations without compiling changing catalog contents into `std`. The intended direction is recorded in [`sbol.md`](sbol.md).

## Procedure construction contracts

A kind declares the scientific properties an artifact may hold. An exact action identity selects applicable Methods, and each program-producing Method task explicitly names a Procedure construction contract. The builder consumes resolved typed task values and emits a complete canonical program before facility planning. An adapter sees that program, never the source artifact property bag, and compatibility does not depend on the biological action name.

Declarative templates cover Procedure programs that can be assembled by substituting checked values. The remaining construction question is how a package distributes and composes an algorithmic builder when the program requires iteration, arithmetic, or another transformation outside the template language, without making arbitrary compiler plugins part of package checking.

Schema composition is separately unresolved. A kind cannot yet extend or refine another, so packages still need a reusable way to compose scientific property schemas, defaults, and refinements. That composition belongs before Method refinement and must not become an adapter-specific schema.

Reaction chemistry illustrates the boundary. A design may state `reaction_volume = 20 uL`; the Method signature and Procedure construction validate and preserve it once. OT-2, Flex, and STAR then consume the same versioned program and supply only their device-specific realization.

## Facility configuration and allocation policy

Independent target profiles and backend selection have been removed from the package workflow. The open composition problem is now sharper: stable physical facts should be represented once in SBOLInventory, while private or runtime-only implementation configuration remains in the exact Asset-to-adapter overlay. The current liquid-handler adapters still accept detailed deck configuration that should move into typed Asset composition, positions, and offering parameters where the profile can represent it efficaciously.

Method and Asset pins now express exact, reviewable choices between eligible alternatives, while deterministic candidate ordering never acts as policy. Reservations, capacity sharing, scheduling, and optimization objectives remain unresolved and must not be smuggled into persistent facility facts.

## Inventory identity, availability, and provenance

Design identity and physical availability are now separate. `sbol_identity` names an exact SBOL Component; facility planning loads a validated SBOLInventory document, restricts active MaterialLots to the selected facility, joins them through `sbol:built`, and freezes the selected lot together with the facility and document hash. Active lots of the same Component are interchangeable: one is selected deterministically and the others remain review evidence. Quantity, expiration, containment, reservation, trust policy, and asynchronous availability remain open.

## Package resolution

Whole-module `use` syntax and five bundled `std` modules resolve. Path dependencies resolve recursively, import their public symbols through checked module interfaces, diagnose cycles, honor a semver requirement, and produce a lockfile.

What remains unspecified is everything a registry implies: dependency acquisition, integrity verification, caches, version selection across a graph with conflicting requirements, and symbol visibility rules that let a package export less than everything it declares. The boundary between `std` and versioned biological catalogs is also unsettled, and it constrains the rest: a catalog that ships as an ordinary package needs the same visibility and versioning answers.
