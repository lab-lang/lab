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

## Parts and biological catalogs

`std.bio.inventory` now provides typed constructors for external part, backbone, enzyme, strain, and antibiotic identities. This is not yet authoring syntax for declaring a part's biological kind, sequence, provenance, version, or relationship to SBOL. It remains open how biological catalogs expose those richer declarations without reducing them to untyped properties or compiling changing catalog contents into `std`.

## Property schemas and target contracts

Plasmid properties are backend-neutral typed expressions, while the initial OT-2 specialization requires a documented property set. Packages still need a way to declare reusable property schemas, defaults, refinements, and target capability contracts. This should allow a target to state what it consumes without adding experiment-specific property names or diagnostics to the core checker.

## Inventory identity, availability, and provenance

Inventory constructors currently associate a typed source symbol with an external string. Stable identifiers, aliases, lots, quantities, locations, expiration, provenance, trust, and asynchronous availability are unresolved. A planner must distinguish “this design refers to an inventory identity” from “a suitable physical lot is available now.”

## Package resolution

Whole-module `use` syntax and five bundled `std` modules now resolve. Package manifests and filesystem discovery exist, but dependency acquisition, versions, aliases, visibility, cyclic imports, lockfiles, integrity, imported public symbols, and the exact boundary between `std` and versioned biological catalogs remain to be specified and implemented.
