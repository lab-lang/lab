# 0052: Material states are declared facets, not separate kinds

## Status

Accepted, partially implemented. Extends
[0006: Affine material flow in portable workflows](0006-affine-material-flow.md) and is governed by the
vocabulary rule in [0022: Fixed grammar, open vocabulary](0022-fixed-grammar-open-vocabulary.md).

Facets are declared, exported, and narrow a material. A facet state's declared fields are carried but
never required at a declaration, so a state cannot yet stand as an acceptance criterion.

## Context

`Culture`, `Clone`, and `Plate` are nominal prelude types with no fields. A culture cannot say which
organism is growing or which medium it grows in. A plate cannot say the medium it was poured from.
They are opaque because they are not kinds of thing: they are states some design is in, and there is
no design underneath them to ask.

LAIR already models this correctly. `MaterialType` is a set of material states carrying
`material-state#` identities, and the verifier enforces them: `TransformOp` refuses a `cells` operand
that is not `CompetentCells`. Transformation into cells nobody made competent is already a checked
error one layer below the language.

Because the source language has no state, the state is assumed at the lowering site rather than
derived. Every `provision` constructs `ProvisionOp::competent_cells`, so `provision chloramphenicol`
checks as `Material<Antibiotic>` at the frontend and arrives in LAIR as a value the IR believes is
competent cells. The frontend type and the IR state disagree and nothing reconciles them.

Encoding a state as a new kind does not fix this; it is the same mistake again. It would also make
provenance a type distinction, which [0027](0027-provenance-is-stated-per-thing.md) refuses: bought
competent cells and competent cells made from an overnight culture are the same thing in the same
state, differing only in where they came from.

## Decision

A material is `Material<T>` for a design kind `T`. What state it is in travels on the material.

A **facet** is a named classification of a kind's materials, declared at module scope:

```lab
/** Whether a chassis has been made competent, and how well. */
facet Competence on Chassis:
  naive
  competent:
    efficiency: Quantity<cfu/ug>

  naive -> competent
```

A facet lists its states in order, and the first is the state a newly established material is in
unless its declaration says otherwise. A state may carry fields, which is what `Culture` needed and
could not have. Transitions are written explicitly, and an action that establishes a state the facet
does not reach from the state it required is a diagnostic.

**Facets are orthogonal.** A kind may carry several, and a culture that is both diluted and grown
under selection is two facets rather than one flattened state. Flattening is what produced
`DilutedCulture` and `RecoveredCulture` in the IR, and it does not survive a third axis.

Facets are contributed to, exactly as schemas are under
[0028](0028-schemas-are-contributed-to.md). A package may declare a facet on a kind another package
declared, so a laboratory can track an axis the standard library does not.

A material type constrains a facet with `is`, the word that already means *plays this
classification*:

```lab
transform GVD_strain from dependencies into (cells: Material<Chassis is competent>)
```

An action contract states the facet state it requires of each operand and the state it establishes on
each result. That table already drives ownership checking under 0006; state checking joins it rather
than becoming a second pass.

Provenance stays orthogonal. `buy chassis DH5alpha: competence = competent` and a workflow that makes
its own competent cells produce the same type in the same state, and
[0027](0027-provenance-is-stated-per-thing.md) continues to say which is which.

Transitions are declared rather than inferred. Inference could be added later over a declared graph;
deriving the graph from usage first would mean the compiler agrees with whatever the protocol
happened to do.

## Consequences

- `Culture`, `Clone`, and `Plate` stop being types, once a cultivation facet exists to replace them.
  A culture becomes `Material<Strain>` in a state, so it knows its organism from its type argument
  and its medium from the state's fields. They remain fieldless prelude types until then, and the
  27 sites that name them span the Python SDK and the OT-2 adapter as well as the compiler.
- Provisioning stops minting competent cells for everything. The state of a provisioned material is
  named for the kind that was fetched, so an antibiotic no longer arrives as a value the IR believes
  is a tube of cells.

  This needed a second change, because a Method's ports name their state literally and the registry
  requires every candidate refining one Intent to share a signature. There was therefore exactly one
  provisioning signature and it said `CompetentCells`. Fetching a chassis and fetching a plasmid land
  in different states, so neither a second method nor one uniform state could express it. A port may
  now say that its state is the one its Intent asked for, resolved from the Intent result it is
  exported as. Only an output may say it, and only where a Method output exports it, since there is
  otherwise no result to read the state from; both are refused when a definition is validated.
- `MaterialType` carries an absolute IRI rather than one of a closed set, so it gains states without
  a change to its definition. The set was already too small: `method::standard` mints
  `AssemblyReaction`, `TransformationMixture`, and `RecoveryMixture`, none of which the enumeration
  admitted. Refinement carries the state across the dialect boundary unchanged, so the table that
  translated seven variants into seven IRIs is gone and cannot fall out of step with the states a
  package declares.
- Transforming into cells that were never made competent becomes a frontend diagnostic pointing at
  the operand, rather than a verifier failure in LAIR or nothing at all.
- A facet with no transition into a state makes that state unreachable, which is a diagnostic at
  declaration rather than a protocol that cannot be planned.
