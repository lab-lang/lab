# 0020 — Laws are declared roles the compiler enforces

## Status

Accepted. Supersedes the declaration-kinds paragraph of
[0001](0001-language-kernel.md).

## Context

Lab had six words for declaring data — `record`, `material`, `observation`,
`evidence`, `event`, and `outcome` — and 0001 justified them by saying that
laboratory declaration kinds "carry semantic meaning rather than acting as
decorative aliases for a generic `type` declaration."

That had stopped being true. Searching every use of `DataKind` outside the AST
and parser found exactly two: `Event` registered a type for `emit` and `when`,
and `Observation | Evidence` widened a synthesized evidence union.
`DataKind::Material` had **no semantic effect whatsoever**. Three of the six
words appeared in no `.lab` source anywhere in the repository.

So `material` had become precisely the decorative alias 0001 forbade, and the
principle needed a mechanism rather than a keyword.

## Decision

One declaration word, `record`, plus role membership. What each removed word
asserted becomes something the checker can read:

```lab
record Started is Event
record PlateReading is Evidential:
  count: Integer
```

A **law** is a role the compiler enforces a rule for. `Catalogued` is one:
types playing it may be declared with `catalog`. Laws are selectable but not
definable — a package may declare its type catalogued and may not invent a law,
because a law no checker reads would mean nothing.

There is therefore no source form for declaring a law, and that is the point:
the impossibility is what makes the closed set closed. Laws live in the prelude
and the generated reference marks them, because a reader cannot otherwise tell
`Catalogued` from `Signal` on the page.

`material` is deleted rather than translated. `Affine` is **not** introduced:
affinity is carried by the `Material<T>` wrapper, not by any declaration, so a
law of that name would either relabel something inert or add flow tracking no
program has asked for.

## Consequences

The evidence-union rule stopped being ad hoc. It had hand-rolled an existential
before existentials existed — widening `List<Evidence>` to a union of every
observation and evidence type in scope. It is now role membership, and
`Evidential` says what those words were reaching for.

`Evidence` remains a type as well as playing a role, because `quantify` returns
one. Splitting the type from the category is a larger change and is not made
here.
