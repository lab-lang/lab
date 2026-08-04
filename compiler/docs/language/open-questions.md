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

The parser currently preserves the phrase following `<-` as source text. This
lets us test readable forms such as `sequence aliquot` and `pick 4 isolated
colonies from plate` without prematurely fixing how prepositions map to typed
arguments. We need one uniform signature and name-resolution model before these
phrases are lowered.

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

Whole-module `use` syntax is settled enough to parse. Package manifests,
versions, aliases, visibility, cyclic imports, and the exact boundary between
`std` and versioned biological catalogs remain to be specified and implemented.
