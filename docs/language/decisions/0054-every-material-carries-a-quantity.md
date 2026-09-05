# 0054: Every material carries a quantity, and divisible materials are drawn from

## Status

Accepted, unimplemented. Extends
[0006: Affine material flow in portable workflows](0006-affine-material-flow.md) and depends on
[0053: Quantities carry dimensions and compose](0053-quantities-carry-dimensions-and-compose.md).

## Context

0006 gives every `Material<T>` one owning place, verified with the `copy`, `borrow`, and `take` modes
a resolved action contract supplies. That is the right model and this does not change it.

What a material does not have is a size. `split` is the only way to divide one: it is typed to
`Plasmid`, it yields exactly two, and it says nothing about how much went each way. A shelf holding
250 mL of LB that three protocols draw 100 mL from cannot be written at all, and neither can the
ordinary observation that a source vessel must hold more than a protocol dispenses from it.

A divisible resource is not in tension with affine ownership. It is the case affine ownership was
built for: the whole is consumed and the parts are introduced, so no place is ever used twice. The
machinery is already present. `split` has the two-result shape, and `culture <- recover culture for
1 h` already consumes a place and introduces one under the same name.

The accounting also already exists below the language. Vessels carry `initial_volume_each`,
`working_capacity_each`, and `dead_volume_each`, and serial dilution reasons about medium source and
dead volumes. The quantity is real and is tracked; it is simply invisible where the protocol is
written, and a bulk reagent reaches it as an opaque symbol in a Method definition.

## Decision

**Every material carries a quantity.** The kind declares the dimension that quantity is measured in
and whether the material is divisible:

```lab
artifact Medium is ...:
  measured in Volume, divisible
```

A divisible material is drawn from, and the draw is an ordinary affine consumption:

```lab
lb, aliquot <- draw 100 mL from lb
```

The draw takes the material and introduces two: the remainder and the aliquot. Rebinding the
remainder under the same name is the existing pattern, so 0006 needs no exception and
`material_flow.rs` needs no new concept. `split` becomes a draw of half, or retires.

An indivisible material is counted rather than measured, and `draw 0.4 from plate` is refused by the
kind rather than by a special case. A count is a quantity like any other under
[0053](0053-quantities-carry-dimensions-and-compose.md).

Because quantities compose, a draw may be stated in any dimension the material's quantity converts
to. `draw 500 ng from prep` is a volume computed from the prep's concentration.

Where a quantity is statically known, over-drawing is a diagnostic naming the material and the
shortfall. Where it is not, it resolves against an inventory lot during planning, and the plan
records the lot it bound as [0051](0051-interchangeable-resources-resolve-without-a-pin.md) requires.
Dead volume is expressible at the source, so a protocol can state that a source must retain more than
it dispenses.

Loops over collections containing materials stay refused under 0006, so a draw inside a loop remains
inexpressible until a consuming iterator contract exists.

## Consequences

- Bulk reagents are written where the protocol is written. A medium stops being a string in a Method
  definition resolved out of sight of the person reading the protocol.
- Affine flow is unchanged. A draw is one take and two introductions, and the existing analysis
  verifies it without learning a new rule.
- The quantity a workflow needs and the quantity a lot holds become the same question, so inventory
  resolution can refuse a lot that is too small rather than discovering it at the bench.
- A medium lot drawn on by two cultures is visible as a shared input, which is what lets lineage
  analysis treat a shared batch as the confounder it is rather than as two independent preparations.
- Kinds must state a dimension and divisibility, so an existing kind that states neither is
  incomplete and says so at its declaration.
