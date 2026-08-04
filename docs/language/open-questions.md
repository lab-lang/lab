# Open language questions

These choices are intentionally not hidden behind parser behavior. A parsed form is not a settled semantic decision.

## Concurrent state transitions

Durable workflow memory is now explicit with `state`, and ordinary bindings are immutable. The remaining question is the transaction model when multiple ready handlers read and update the same state: strictly journal-ordered handlers are the initial semantics, but conflict diagnostics and future safe parallelism still need design work.

## Effect action grammar

The parser preserves a phrase-shaped action syntax after `<-`. The module compiler resolves standard actions through typed contracts that declare phrase slots, operand ownership, result types, and a dispatch capability. We still need source syntax and package metadata for declaring non-standard actions without adding them to the compiler-owned registry.

## Concurrency and cancellation

Sequential effects and independent `when` handlers are represented. Syntax for starting several physical actions together, joining them, races, timeouts, and explicit cancellation is not settled. Cancellation must distinguish stopping a subscription from attempting to cancel an already-dispatched physical action.

## Parts and biological catalogs

`part` is part of the intended declaration vocabulary, but authoring syntax for declaring a part's biological kind, sequence, external identity, and provenance is still open. The compiler should not reduce all of these to untyped fields.

## Package resolution

Whole-module `use` syntax and the specimen's three built-in `std` modules now resolve. Package manifests, filesystem modules, versions, aliases, visibility, cyclic imports, and the exact boundary between `std` and versioned biological catalogs remain to be specified and implemented.
