# Open language questions

These choices are intentionally not hidden behind parser behavior. A parsed
form is not a settled semantic decision.

## Durable workflow state

The reactive specimen updates `observations` with `=` inside a `when` handler.
We still need to decide whether captured workflow bindings become durable state
implicitly, or whether an explicit marker such as `state` or `remember` is
worth one additional keyword. The runtime semantics must make concurrent event
updates deterministic and replay-safe either way.

## Effect action grammar

The parser preserves the phrase following `<-` as source text. The portable
module compiler now resolves the standard plasmid-action phrases used by the
representative workflow and checks their operand and result types. We still
need one extensible signature grammar for package-defined actions rather than a
compiler-owned set of phrase shapes.

## Concurrency and cancellation

Sequential effects and independent `when` handlers are represented. Syntax for
starting several physical actions together, joining them, races, timeouts, and
explicit cancellation is not settled. Cancellation must distinguish stopping a
subscription from attempting to cancel an already-dispatched physical action.

## Parts and biological catalogs

`part` is part of the intended declaration vocabulary, but authoring syntax for
declaring a part's biological kind, sequence, external identity, and provenance
is still open. The compiler should not reduce all of these to untyped fields.

## Package resolution

Whole-module `use` syntax and the specimen's three built-in `std` modules now
resolve. Package manifests, filesystem modules, versions, aliases, visibility,
cyclic imports, and the exact boundary between `std` and versioned biological
catalogs remain to be specified and implemented.
