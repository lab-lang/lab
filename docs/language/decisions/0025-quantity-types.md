# 0025 — A quantity type names the unit it is measured in

## Status

Accepted.

## Context

`Ty::Quantity` existed but only ever arrived from an expression: `20 uL` inferred
`Quantity<uL>`, and there was no way to write that type in a field declaration.
A schema could therefore not say that a field holds a volume, which left every
quantity-valued property untyped at its declaration site.

## Decision

`Quantity<uL>` is a written type whose argument is a unit rather than a type.

```lab
record Reagent:
  digest_temperature: Quantity<C>
  concentration: Quantity<ng/uL>
```

One reader serves both `100 ng/uL` and `Quantity<ng/uL>`, so a unit is written
the same way wherever it appears and the two cannot drift apart.

The unit check stays exact. `20 mL` where `Quantity<uL>` is expected is a
diagnostic, not a conversion, because a thousandfold error on the bench is worth
refusing. Naming the type did not change what is checked.

## Consequences

A dimension-based type that accepted any volume unit and converted would be a
different semantics, and is not this. `Quantity<uL>` pins the unit, which is what
the compiler already enforced before the type had a name.
