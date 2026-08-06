# 0013 — Strains are artifacts, not plasmid properties

## Status

Accepted.

## Context

A plasmid declaration carried a `host` property naming the organism it would be
transformed into. That made the engineered organism a detail of the DNA design
rather than a thing in its own right, with three consequences:

- one plasmid could name exactly one host, so putting the same construct into a
  cloning strain and an expression strain was inexpressible;
- co-transformation, where one organism carries several plasmids, had no form;
- the organism had no name, so it could carry no acceptance criteria and appear
  in no dependency graph, even though it is the artifact a laboratory stores,
  ships, and hands to the next experiment.

The type name `Strain` was already taken by the inventory identity of a host
organism, returned by `strain("DH5alpha")`.

## Decision

`strain` is a first-class biological declaration alongside `plasmid`. Both share
one declaration shape — typed properties, `require`, and `accept` — and differ
in what they name and which nominal type their properties are checked against.

The inventory constructor becomes `chassis("DH5alpha")`, returning `Chassis`.
`Strain` names the declared artifact. A chassis is a catalogued host; a strain
is a chassis together with the plasmid designs it carries.

Transformation realizes a strain, so the operation that creates the physical
material is the one that establishes the artifact's identity:

```lab
transform <design: Strain> from <plasmids: List<Material<Plasmid>>>
  into <cells: Material<Chassis>> -> (strain: Material<Strain>, culture: Material<Culture>)
```

Two contracts simplify as a result. `realize` returns only
`product: Material<Plasmid>`; its second `construct` result existed solely to
feed the old `transform`. `assemble` returns `Material<Plasmid>` rather than
`Material<Construct>`, because a circular assembled construct is a plasmid, and
the `Construct` type is removed.

## Consequences

A build graph now spans both kinds. Plasmid artifacts contribute assembly;
strain artifacts contribute transformation, recovery, dilution, and plating. An
OT-2 batch emits a robot protocol only for the stages its artifacts reach.

One plasmid feeding several strains is ordinary dataflow: each strain workflow
takes the plasmid material as a typed input, and the compiler derives the wave
ordering from that. Nothing needs to duplicate a material.

Action dispatch resolves on the first word of a phrase, so a single `transform`
contract exists at a time. Replacing it was a breaking change to every workflow
that used the two-operand form.
