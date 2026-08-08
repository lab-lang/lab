# 0015 — Roles classify types

## Status

Accepted.

## Context

Type parameters could carry bounds — `circuit f<I: Signal>` parsed and checked —
but a bound was satisfiable only by a relationship hardcoded in Rust. Two
entries existed: `Tetracycline implements Signal` and
`GreenFluorescentProtein implements Protein`. A scientist could declare a new
sensor and no generic circuit would accept it, because nothing in the source
language could say what part that sensor plays.

Bounds on `data` declarations were worse: parsed, then discarded. `DataSignature`
had no field for them, so `record Sensor<T: Signal>` accepted any argument at
all.

## Decision

`role` declares a name that types can play. `is` asserts that a type plays one.

```lab
role Inducer

record Arabinose is Inducer
record Tetracycline is Inducer
```

`Signal` and `Protein` are roles the prelude declares; the example above is the
case the decision exists for, where a scientist publishes a classification of
their own.

A role has no block. Its members are declared by the types that play it, so a
package may classify its own types against a role it imported, and a role stays
open to types that do not exist yet.

`:` is unavailable for membership — `record Tetracycline:` already opens a field
block — so `is` is forced rather than chosen. Bounds keep `:`
(`Promoter<S: Signal>`). The two relations are spelled differently because they
are different: `is` asserts a fact about a specific type, `:` constrains an
unknown.

**A role is not a type.** It may bound a type parameter and may appear after
`any`; it may not be the type of anything. `lower_type` and `lower_bound` are
separate functions so that the one position where a role belongs is visible in
the call graph rather than decided by a flag.

Membership declared in source and membership built into the standard library
populate one table, so a bound is satisfied the same way whichever it came from.
Roles, and the roles a type plays, travel through `ModuleInterface` as part of a
type's public surface.

## Consequences

The bare-role error is where the concept is taught, and it states both ways
forward because which one is right depends on something the compiler cannot
know — whether the caller's choice still matters further down:

```
error: 'Signal' is a role, not a type
  = help: name it, and everything using that name must agree: <T: Signal>
  = help: or name a type that plays Signal: Arabinose, Tetracycline
```

Bound failures name the alternatives, which is most of the teaching value for
the price of inverting one map.

Roles live at the type level only. A record field cannot store one; a run that
wants to record which signal it used stores `Material<any Signal>` if it means
the physical bottle, or declares an ordinary tagged type if it means a label.
This is the same tradeoff as traits versus enumerations, with the same
workaround.

Starting strict is the only reversible choice. Relaxing "a bare role is an
error" later breaks nothing; tightening it later would break every program that
had taken advantage of it.
