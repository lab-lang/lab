# 0017 — A type argument may be deliberately forgotten

## Status

Accepted.

## Context

A panel of sensors is a collection of circuits that respond to different
inducers and report the same thing. Nothing could describe it. Inference gave a
union — `Circuit<Tetracycline, R> | Circuit<Arabinose, R>` — which preserves the
alternatives and therefore says the members are different, when the point of a
panel is what they share.

Subtyping alone cannot express this. Given only "Tetracycline is a kind of
Signal", a signature reading `(circuit: Circuit<Signal, R>, inducer:
Material<Signal>)` typechecks when handed a tet-responsive circuit and a bottle
of arabinose. Saying "these two must be the same signal" needs a name for the
unknown; saying "and here I am deliberately not naming it" needs the dual.

## Decision

`any Role` is a type argument whose identity has been discarded:

```lab
panel: List<Circuit<any Signal, GreenFluorescentProtein>> = [tet_reporter, ara_reporter]
```

`Circuit<Tetracycline, R>` flows into `Circuit<any Signal, R>` whenever
`Tetracycline` plays `Signal`. The reverse never holds. `any Role` is legal only
as a type argument: a value cannot *be* a signal, only carry one, so
`x: any Signal` is rejected and `Material<any Signal>` is not.

Two rules in the type relation carry it. Packing is one arm — a concrete type
fits where a role is expected if it plays that role — and because arguments
compare recursively, `Circuit<Tetracycline, R>` reaching `Circuit<any Signal, R>`
needs no separate rule. The second is independently correct and was missing: a
union fits wherever every one of its alternatives fits.

Type arguments stay **invariant** everywhere else. Widening is available only by
writing `any`, never by accident.

**Packing happens only where an annotation asks for it.** `common_type` is
untouched, so an unannotated list still infers a union. Forgetting is a
deliberate act the author writes down.

The load-bearing rule is in `unify`: a type parameter may not be inferred from a
forgotten type. Without it, `S` binds to `any Signal`, `Material<S>` accepts any
inducer that plays `Signal`, and the wrong-reagent error this system exists to
produce silently stops firing.

## Consequences

The two halves of the idea are one sentence: `any Signal` is some signal you
will never learn the name of; `S: Signal` is some signal you have named so you
can point at it again.

Lab now carries both ways of describing a mixed collection, with different
rules, in the same file. A union preserves provenance and can be matched on; an
existential discards it and cannot. Most people never learn those are different
things.

Refusing to un-forget is not an awkwardness. A homogeneous list of circuits with
different triggers has, by construction, no inducer that works for all of them,
so the type system is reporting a fact about the bench:

```
error: 'S' cannot be inferred from a forgotten type
  = help: 'any Signal' means some Signal, deliberately not recorded
  = help: there is nothing here for the other uses of 'S' to be matched against
```

Deciding compatibility now needs the role table, so `compatible`, `common_type`,
`comparable`, and `unify` moved onto the checker's context.

`CheckedType` gains an `Any` variant, so the portable module IR is
`lab.portable-module.v2`. A consumer that has not been updated cannot read a
type it has no variant for.

Packing does not launder ownership: `Material<Tetracycline>` widened to
`Material<any Signal>` is still a material, and the affine checker is unaffected.
