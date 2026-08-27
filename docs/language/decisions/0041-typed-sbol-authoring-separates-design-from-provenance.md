# 0041 — Typed SBOL authoring separates design from provenance

## Status

Accepted. Extends [0027](0027-provenance-is-stated-per-thing.md) to the Python
frontend.

## Context

The Python frontend originally accepted an ordinary pySBOL3 `Component` as the
first positional argument to `ArtifactKind.build`. This preserved SBOL as the
design interchange, but made the common authoring path operate directly on a
mutable RDF object graph. Constructing a plasmid meant separately constructing
its component, sequence, subcomponents, roles, topology type, and ordering
constraints before Lab could read it.

Utility functions can hide that graph manipulation, but a helper that always
returns `tuple[sbol3.Component, sbol3.Sequence]` loses the biological type that
selected the helper. A promoter, coding sequence, terminator, backbone, and
plasmid all become the same tuple even though callers and type checkers know
they are not interchangeable. The tuple also provides no typed relationship
between the two independently identified SBOL objects, leaving synchronization
and reuse to convention.

The first typed API repeated another fact. An anonymous plasmid was written as
`designs.plasmid("reporter", ...)` and then assigned to
`reporter = Plasmid.build(...)`. The SBOL identity and Lab declaration name
usually have the same local name, so requiring both creates disagreement
without adding information.

Finally, an SBOL identity says which design an object denotes. It does not say
whether this laboratory buys that object or builds it. Treating every registry
IRI as an implicit purchase would move provenance into the design layer and
contradict [0027](0027-provenance-is-stated-per-thing.md): the same plasmid
design may be assembled locally or ordered from a supplier.

## Decision

The Python frontend provides a typed, lazy SBOL authoring layer under
`lab.sbol`. A `Document` owns one namespace and returns a distinct design type
for each biological kind:

- `part` returns `DnaPartDesign`;
- `promoter` returns `PromoterDesign`;
- `rbs` returns `RibosomeBindingSiteDesign`;
- `cds` returns `CodingSequenceDesign`;
- `terminator` returns `TerminatorDesign`;
- `engineered_region` returns `EngineeredRegionDesign`;
- `backbone` returns `BackboneDesign`;
- `plasmid` returns `PlasmidDesign`; and
- `protein` returns `ProteinDesign`.

The same document creates independent `DnaSequence` and `ProteinSequence`
objects. A design references a sequence of the appropriate type rather than
receiving bare elements or owning a hidden sequence. This relationship is
specified further in
[0043](0043-sequences-are-first-class-design-values.md). The public document
and factory inputs are keyword-only, including `namespace`, `identity`,
`elements`, `sequence`, and `components`. This makes calls readable at the
point of use and allows a factory to grow without changing the meaning of a
positional argument.

A local design may omit `identity`. When a Lab declaration is emitted, its name
becomes the anonymous design's local SBOL identity under the document namespace.
An explicit relative identity is resolved under that namespace, while an
absolute identity preserves a registry IRI. Reusing one anonymous design under
different declaration names is refused; a reusable design must state an
explicit identity.

The design factories say what the biology is. The artifact declaration says
how this laboratory obtains it:

```python
import lab
from lab import sbol
from lab.bio.designs import CDS, Part, Promoter
from lab.bio.golden_gate import Plasmid

module = lab.Module("reporter.designs")
designs = sbol.Document(namespace="https://example.org/reporter")

J23101 = Promoter.buy(
    design=designs.promoter(identity="https://registry.example/J23101"),
)
B0034 = Part.buy(
    design=designs.rbs(identity="https://registry.example/B0034"),
)
GFP = CDS.buy(
    design=designs.cds(identity="https://registry.example/GFP"),
)

reporter_sequence = designs.dna_sequence(elements="ACGTACGT")
design = designs.plasmid(
    components=[J23101, B0034, GFP],
    sequence=reporter_sequence,
)
reporter = Plasmid.build(design=design)
```

`build` and `buy` also take only keyword arguments. Both attach a design with
`design=`, but retain different result types: `BuildDeclaration[K]` and
`BuyDeclaration[K]` for the artifact kind `K`. A build may state requirements,
acceptance claims, and build ordering. A buy may state a supplier identity and
properties, but cannot carry build claims.

A typed composite does not infer the source of its children from their
identities. Each child is first attached to an explicit build or buy
declaration, and that declaration is passed in `components`. The typed SBOL
graph retains the child design while the emitted Lab module retains its chosen
provenance. A bare typed child with no declaration is refused when the module is
emitted.

Materialization is delayed until module emission, when the declaration name is
known. The document then creates ordinary independent pySBOL3 sequences,
components which reference them, subcomponents, topology types, and `meets`
constraints, and validates the resulting document before Lab source is checked.
Materialization is idempotent, identity collisions and cross-document
composition are refused, and invalid generated SBOL is reported at this
boundary.

The abstraction is not a replacement for pySBOL3. Typed designs expose their
materialized `sbol3_component`, and a document exposes its `sbol3_document`.
An existing raw pySBOL3 component remains accepted through
`ArtifactKind.build(design=raw_component)` or `buy(design=raw_component)`. When
an unannotated third-party SBOL graph references child components, the raw
compatibility reader retains its catalogued-buy fallback because no Python
declarations exist from which to recover another provenance choice.

## Consequences

Factory result types preserve biological distinctions during normal Python type
checking: a protein is not a valid DNA component, and a backbone does not become
a plasmid merely because both are circular DNA. The Lab boundary also checks
the design kind at runtime, so a promoter design cannot be supplied to a
plasmid declaration. Declaration return types preserve whether an artifact is
built or bought instead of collapsing both into one generic declaration.

Sequence result types preserve molecule distinctions too: a `ProteinSequence`
cannot be supplied where a DNA design expects `DnaSequence`, and a named
sequence may be shared without duplicating its elements or SBOL object.

The common API no longer exposes tuple bookkeeping or requires a repeated local
identity. In the example above, `reporter` resolves to
`https://example.org/reporter/reporter` when the module is emitted.

An absolute registry IRI no longer silently asserts procurement. This makes the
Python frontend express the same provenance distinction as Lab source and
keeps the design reusable by laboratories that obtain it differently.

Errors that require the complete identity graph or pySBOL3 validation occur at
module emission rather than at the factory call. Errors that need no graph,
such as placing a protein in a plasmid or passing a promoter design to a
plasmid declaration, are rejected earlier.

Passing a raw design positionally is no longer supported. The explicit
`design=` spelling puts typed and raw designs through one boundary and keeps the
rest of the declaration's keyword properties unambiguous.

This remains an incremental Python authoring boundary. The frontend currently
lowers the typed document through the existing Lab module path; a future direct
`lab-sbol` bridge may replace that lowering without changing the authoring or
provenance model decided here.
