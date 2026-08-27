# 0043 — Sequences are first-class design values

## Status

Accepted. Refines the sequence ownership described in
[0041](0041-typed-sbol-authoring-separates-design-from-provenance.md).

## Context

Typed SBOL factories preserved the biological type of a promoter, coding
sequence, backbone, or plasmid, but their `sequence` argument was still a
string. The Python object then created a pySBOL3 `Sequence` as an implementation
detail owned by that component. The compiler made the same coupling in Design
LAIR: `design.plasmid` stored the bases in a string attribute.

That shape loses an SBOL distinction. A component and a sequence are separate
identified objects connected by a reference. One sequence may be named,
inspected, or reused independently of any one component. Copying its elements
into every design makes that sharing invisible and provides no typed boundary
between DNA and protein sequences.

Lab source already has the right general mechanism. A top-level binding can
name a typed pure value, and a design property can reference it:

```lab
reporter_sequence: DNA = dna("ACGTACGT")

build plasmid reporter:
  sequence = reporter_sequence
```

The missing part was preserving that distinction after checking and exposing
the same model through Python.

## Decision

A sequence is a first-class typed design value.

In Lab source, a named `DNA` binding is the canonical reusable form. An inline
`sequence = dna("...")` remains accepted as shorthand. Source lowering resolves
references to top-level DNA bindings and gives an inline value a synthetic
sequence identity.

Design LAIR represents DNA with `design.dna_sequence`. The operation owns the
sequence name and elements and returns `design.dna_sequence`. A
`design.plasmid` has a typed operand of that result type; it no longer stores
sequence elements as an attribute. Lowering emits one sequence operation for
each distinct source binding, so several designs may consume the same SSA
value. Verification checks the sequence at its defining operation and verifies
that every plasmid sequence operand comes from such an operation.

The Python API mirrors this graph:

```python
from lab import sbol

designs = sbol.Document(namespace="https://example.org/reporter")
reporter_sequence = designs.dna_sequence(elements="ACGTACGT")
reporter = designs.plasmid(sequence=reporter_sequence)
```

`Document.dna_sequence(...)` and `Document.protein_sequence(...)` return
different types. DNA components accept only `DnaSequence`; protein components
accept only `ProteinSequence`. A component and its sequence must belong to the
same document. All inputs remain keyword-only.

Sequences have their own optional identity, name, description, materialization,
and document membership. An anonymous sequence referenced by a design inherits
a stable identity when that design is resolved. Reusing an anonymous sequence
is refused because choosing the first consumer's identity would make identity
depend on construction order. An explicitly identified sequence may be reused
by several designs and is materialized once. A standalone anonymous sequence is
rejected at the document boundary because it has no stable SBOL identity.

When the Python frontend emits Lab source, it creates one typed module-level
`DNA` binding per readable SBOL sequence and makes each design reference that
binding. This applies to typed designs and to the raw pySBOL3 compatibility
path. Sequence identity conflicts with different elements are rejected.

The sequence relation says what sequence a design references. It does not say
whether the laboratory buys or builds the design. Provenance remains on the
artifact declaration under [0027](0027-provenance-is-stated-per-thing.md) and
[0041](0041-typed-sbol-authoring-separates-design-from-provenance.md).

## Consequences

Python type checking now rejects a protein sequence supplied to a plasmid, and
the same check exists at runtime. Sharing is explicit rather than reconstructed
from equal strings. SBOL materialization produces one top-level Sequence object
and ordinary component-to-sequence references.

Backends still receive the exact elements through the plasmid's use-def edge,
so this change does not weaken sequence-aware planning or acceptance. It moves
validation to the value that owns the elements and gives later design passes a
real graph edge to analyze.

This decision does not claim that every composite sequence can be derived from
its listed parts. Backbones, junctions, orientations, edits, and incomplete
registry records can make derivation a separate design operation. A caller may
state an exact sequence independently today; automatic derivation can later
produce the same typed value without changing the plasmid API or Design LAIR.
