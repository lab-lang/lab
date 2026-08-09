# Generics

A type argument says what a value is *about*. `Material<Plasmid>` is physical
material of a particular design's type; `Promoter<Tetracycline>` is a promoter
answering to a particular signal; `Quantity<uL>` is a measurement in a
particular unit. Every generic form in Lab exists to let one declaration carry
that aboutness instead of restating it, and to let the compiler refuse a program
whose parts disagree about it.

This page ties the forms together. [`syntax.md`](syntax.md) records the surface
rules for each, and [`semantics.md`](semantics.md) records what the type system
does with them.

## Type parameters, bounded by roles

A **role** classifies types and has no values, so it may bound a type parameter
and may never be the type of anything ([0015](decisions/0015-roles-classify-types.md)).
A **type parameter** is introduced inline, inside the type of the argument that
determines it, and naming it twice is what links its occurrences
([0016](decisions/0016-callable-circuit-signatures.md)):

```lab
circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    coding
```

A workflow that takes both a design and the reagent it responds to uses one
name in both positions, and the compiler then refuses the wrong reagent. This
is how generics articulate *dependencies*: the relationship between two
arguments is written in their types, not enforced by convention.

## Generic kinds and their instances

An artifact kind may name a generic type. The kind's word then covers every
instantiation, because which arguments an instance has is a fact about the
instance ([0027](decisions/0027-provenance-is-stated-per-thing.md)):

```lab
artifact Promoter

buy promoter pTet: Promoter<Tetracycline>
buy promoter pBAD: Promoter<Arabinose>
```

`artifact Promoter` covers every promoter; each instance names its own type. An
instance may fill in its kind's type arguments and may not name some other
type — the word already said what kind of thing this is. A word whose kind
takes no arguments has already said the whole type, so repeating it says
nothing and the ascription is written only where the word cannot say.

## Fields read through arguments

A generic type's fields are read through its arguments: `Promoter<Tetracycline>`
states what a promoter for tetracycline states. This is how generics articulate
*properties* — a schema field declared once against the head type holds for
every instantiation, at the type each instantiation gives it.

## A union is a set

`List<Part | Plasmid>` and `List<Plasmid | Part>` describe the same values, so
each satisfies the other. A union argument arises naturally from a
heterogeneous list — a component list naming both a dependent plasmid and
ordinary parts infers `List<Plasmid | Part>` — and order never matters.

## Forgetting an argument

`any Role` deliberately forgets a type argument where a collection must hold
values that agree on everything except it
([0017](decisions/0017-forgotten-type-arguments.md)):

```lab
panel: List<Circuit<any Signal, GreenFluorescentProtein>> = [tet_reporter, ara_reporter]
```

The panel remembers what every member reports and forgets what triggers each
one. Forgetting is only meaningful in a type-argument position, and only where
an annotation asks for it.

## A unit is not a type

`Quantity<uL>` names the unit a measurement is in, and the argument is a unit
rather than a type: it is written exactly as `100 uL` writes it, one reader
serves both, and the two cannot drift apart
([0025](decisions/0025-quantity-types.md)). The unit check is exact — `20 mL`
where `Quantity<uL>` is expected is a diagnostic, not a conversion — so naming
the type changes where the error is caught, not what is checked.

## Where each form is allowed

| Form | Where |
| --- | --- |
| `Name<T: Role>` | a callable's parameter types, introducing `T` at first use |
| `record Name<T: Role>:` | a data declaration's header, because its parameter appears in field types |
| `artifact Name` (generic head) | a kind declaration, leaving the arguments to each instance |
| `buy word name: Name<Argument>` | an instance of a generic kind, naming its own type |
| `any Role` | a type-argument position, under an annotation that asks for it |
| `Quantity<unit>` | anywhere a type is written |

A role itself may appear in exactly one position: bounding a type parameter.
Writing one where a type belongs is an error that names both ways forward.
