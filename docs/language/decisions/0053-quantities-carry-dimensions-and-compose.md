# 0053: Quantities carry dimensions and compose

## Status

Accepted, partially implemented. Amends
[0025: A quantity type names the unit it is measured in](0025-quantity-types.md).

Unit safety in arithmetic and comparison holds, and `Mass` and `MassConcentration` exist as canonical
Procedure quantities. Dimensions, written conversion, and dimensional arithmetic do not.

## Context

0025 made `Quantity<uL>` a written type whose argument is a unit, and kept the unit check exact:
`20 mL` where microlitres are written is a diagnostic rather than a conversion, because a
thousandfold error on the bench is worth refusing. That is right for a field holding one measurement,
and it stays right.

It cannot express a recipe. LB is 10 g/L tryptone, 5 g/L yeast extract, and 10 g/L sodium chloride;
a transformation buffer is 50 mM calcium chloride. One component list cannot hold both, because a
field pins one unit and a recipe is not written in one unit.

Worse, a recipe states concentrations while a batch is a volume. Someone multiplies 10 g/L by 500 mL
and weighs out 5 g. That multiplication is done in a head or on a scrap of paper, and getting it
wrong is precisely the class of error the language exists to refuse. A design that states grams
instead is no longer a recipe: it is one batch size, and it stops being portable.

Below the language the dimension set is narrower still. `procedure/quantity/` carries duration,
length, temperature, and volume, and no mass or concentration at all, so a recipe has nothing to
lower into. `ng/uL` exists as a surface unit string with no counterpart underneath it.

## Decision

A quantity has a dimension as well as a unit. Mass, volume, amount, concentration, duration, length,
and temperature are dimensions, along with the products and quotients of them.

**A field may name a dimension instead of a unit.** `Quantity<any Volume>` accepts any volume unit,
reusing the `any` that already forgets a type argument. Each value still pins its own unit; the field
declines to pin one. A recipe holds `10 g/L` and `50 mM` in one list because the field asks for a
concentration rather than for millimolar.

**Conversion within a dimension is written.** `500 mL in uL` converts. Implicit conversion stays
refused, which is what 0025 was protecting, and a field that pins a unit still pins it.

**Quantities compose.** Multiplication and division compute the dimension of the result:

```lab
10 g/L * 500 mL      // 5 g
500 ng / 100 ng/uL   // 5 uL
```

Addition and subtraction require one dimension and yield the unit of the left operand, converting the
right exactly or refusing. A product or quotient yields the canonical unit of the derived dimension,
which `in` converts from.

**Exactness is preserved.** Conversion scales an exact decimal. A conversion that cannot be
represented exactly is a diagnostic rather than a rounding, so no quantity silently loses precision
on its way to a balance or a pipette.

`procedure/quantity/` gains mass and concentration with their QUDT identities, because a recipe that
cannot lower is not a recipe.

## Consequences

- A recipe states concentrations once and scales to any batch size, so a medium design is portable
  across laboratories that make different volumes of it.
- `draw 500 ng from prep` computes a volume from the prep's concentration, which is arithmetic that
  otherwise happens at the bench and is not recorded.
- 0025's refusal of `20 mL` where `Quantity<uL>` is written is unchanged. Nothing coerces.
- Dimensional errors are caught rather than propagated: a mass where a volume is required names the
  two dimensions instead of naming two unit strings that happen to differ.
- A unit is still not a name a signature can introduce or refer to. A dimension is a classification
  the compiler knows, not a type parameter a package declares.
