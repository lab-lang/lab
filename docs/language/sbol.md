# SBOL in Lab

Ontology grounding is implemented, and so are the first four rungs of reading
designs from SBOL: recognizing a kind from its terms, atomic designs, composite
designs, and a checked module from a document. The rest is a design under
construction. Each section says which it is.

The settled shape: **a laboratory writes its designs in Lab or in SBOL, and its
workflows in Lab.** The section on what can be written in SBOL explains why that
line falls where it does.

## The problem

Lab knows more about a design than it can say. The checker knows `J23101` is a
promoter that answers to a signal, that `chloramphenicol` is a selection agent
rather than a nucleic acid, that `composite_plasmid_1` is an ordered composition
of four parts in a backbone, and that two strains built by separate
transformations are independent biological entities. None of that leaves the
compiler in a vocabulary anyone else reads.

Two spikes bracket the gap.

`marpaia/labop` puts a standard *below* Lab as an export target. Its own header
states that information flows one way "into a weaker representation", and it
ships an `omissions.md` enumerating what it dropped: no loop construct, no
material linearity, no lineage. Two of Lab's central claims, affine material
flow ([0006](decisions/0006-affine-material-flow.md)) and replicate class as
lineage ([0026](decisions/0026-lineage-and-replicates.md)), cannot survive the
projection. Every emitted `sbol:Component` is typed `SBO:0000241`, the
general-purpose functional entity term, with the comment "the build does not
know whether a named item is DNA or a reagent".

The build does know. It has no way to say so.

`marpaia/python` puts a foreign host language *above* Lab. Python objects render
Lab source text downward and receive serialized `CheckedModule` JSON upward. The
type information is flattened to display strings on the way out and to
`dict[str, Any]` on the way in. The branch needs 134 lines of source-map
machinery for one reason: errors are reported against generated text the author
never wrote.

Both treat a standard as a format at an edge. Neither makes it a participant in
the type system. `docs/language/open-questions.md` already names the
consequence: authoring syntax for a part's sequence, provenance chain, version,
and relationship to SBOL is missing, and the open question is how catalogs
expose those "without reducing them to untyped properties".

## The thesis

SBOL 3 and Lab's type system are the same model at two altitudes. Lab states a
design in terms a person writes; SBOL states it in terms a machine resolves. The
correspondence is close enough that the compiler can move in both directions,
and where the correspondence is exact it produces new compile-time checks rather
than new output formats.

So the work is not "add an SBOL backend". It is: give the type system a way to
name ontology terms, and then every layer of the compiler has something to say.

`sbol-rs` 1.0.0 is published on crates.io, is edition 2024 with MSRV 1.93
against Lab's 1.95, has no feature flags to reason about, forbids unsafe, and
embeds its ontology facts as a pinned TSV with no runtime network access. It is
a dependency Lab can take without compromising hermetic builds.

## Layer 0: ontology grounding

**Implemented.**

Everything else depends on one function: given a Lab type, what ontology terms
does it stand for.

Lab already had the mechanism for attaching meaning to a type without touching
the parser. A role classifies types, membership travels with the type that
declares it, and a package may classify its own types against a role it
imported. What roles lacked was any external identity. They have one now:

```lab
// std.bio.ontology
role NucleicAcid = "SBO:0000251"
role SimpleChemical = "SBO:0000247"
role EngineeredRegion = "SO:0000804"
role PromoterRegion = "SO:0000167"
role CodingSequence = "SO:0000316"
```

Grounding a kind is ordinary role membership:

```lab
// std.bio.designs
use std.bio.ontology

artifact Antibiotic is SimpleChemical
artifact Promoter is NucleicAcid, PromoterRegion

artifact Plasmid is NucleicAcid, EngineeredRegion:
  sequence?: DNA
  backbone?: Backbone
  components?: List<any NucleicAcid>
```

Two frontend changes, no new production. `parse_role` gained an optional
`= "term"` tail, in the shape
[0021](decisions/0021-typed-external-identities.md) first proposed for catalog
items. `parse_artifact_kind` gained a call to `parse_roles_clause`, which
already existed for records and already returned empty when `is` is absent. A
kind names the type its instances have, so the clause classifies that type and
lands in the existing role table beside every other membership.

No package introduces a grammar production, so
[0022](decisions/0022-fixed-grammar-open-vocabulary.md) holds. A chemistry
package grounds its own kinds in ChEBI without the compiler learning chemistry.

The alternative was to model terms as contributed schema properties under
[0028](decisions/0028-schemas-are-contributed-to.md), which needs no parser
change at all. It is worse: grounding belongs to a kind, and a property forces
every instance to restate that a plasmid is DNA. The altitude is wrong.

Note the constraint from [0020](decisions/0020-laws-are-declared-roles.md): laws
are a closed set with no source form, deliberately. An ontology-grounded role is
an ordinary open role, not a law. Nothing about it is enforced by a bespoke
compiler rule; what the compiler does with it is read it.

### Two things the implementation settled

**Roles and types share one namespace**, so a role cannot be named after a kind
that already exists. `role Promoter` collides with `artifact Promoter`, and the
collision is reported as "'Promoter' is already a type". The vocabulary
therefore names the region rather than the part: `PromoterRegion`,
`CodingSequence`, `RibosomeEntrySite`. This is a naming cost, not a design flaw,
and it is worth paying rather than splitting the namespace.

**Compact identifiers expand on the way in.** `SO:0000167` and
`https://identifiers.org/SO:0000167` are one term written two ways, so the
checker stores the expanded IRI and a test asserts the two spellings agree.
Otherwise a type grounded one way would silently fail to match a document
written the other.

### Where the term is checked

Term *shape* is checked in `lab-language`, at the line the term is written on,
with no dependencies: a term is an absolute IRI or a compact identifier with a
recognized prefix, and anything else is a diagnostic naming what was expected. A
term from a vocabulary the compiler has never heard of is written as a full IRI
and accepted, which keeps the mechanism open.

Term *meaning* stays outside the frontend. The obvious move is to put
`sbol-ontology` in `lab-language`, and it is wrong: that crate depends
unconditionally on `clap` and `ureq`, which is a CLI parser and an HTTP stack in
the crate `lab-ide-wasm` builds on. Membership, branch, and conflict checks
therefore belong in `lab-sbol`, which is also where the document they validate
is built. Feature-gating the ontology cache path so the bundled facts are usable
alone is upstream work worth doing.

The split has a second benefit: a single file can be checked for a malformed
term without resolving a package, and the questions that genuinely need a whole
program are asked when there is one.

### What it looks like

```
error: 'engineered region' is neither an IRI nor a compact identifier
  = help: write a term as "SO:0000167" or as the IRI it stands for
  = help: a role with no term classifies types without naming any ontology
```

## Layer 1: identity that resolves

`CheckedDeclaration::Catalog` carries an optional exact `sbol_identity` separately from its `supplier_identity`, which defaults to the declared name. `CheckedDeclaration::Artifact` carries the same optional `sbol_identity`. The source lowerer uses only the supplier identifier for existing device-specific manifests; inventory resolution uses only the SBOL Component IRI.

Two changes.

**A package declares its namespace.** This is `sbol3::Namespace`, validated by
rule sbol3-10301.

```toml
[package]
name = "golden-gate"
version = "0.1.0"
namespace = "https://synbiohub.org/user/marpaia/golden-gate"
```

Every declaration mints a stable identity `{namespace}/{displayId}`. The
displayId encoder from `marpaia/labop`'s `identity.rs` moves out from under
`backend/` into the frontend where it belongs. It already solves the collision
that `sbol3::design::sanitize_display_id` gets wrong, keeping `pUC19-A` and
`pUC19_A` distinct, and carries a test asserting the encoding still satisfies
rule sbol3-10201.

**A design identity is separate from an order line.** The two meanings have distinct fields:

```lab
buy:
  part J23101:
    sbol_identity = "https://synbiohub.org/public/igem/BBa_J23101/1"

  restriction_enzyme BsaI:
    supplier_identity = "NEB-R0535"
    digest_temperature = 37 C
```

`sbol_identity` is an absolute IRI naming the SBOL Component represented by either a `build` or `buy` declaration. `supplier_identity` is available only on `buy`, defaults to the declaration name, and names something to order. The legacy `identity` spelling remains an alias for `supplier_identity` during migration. The compiler carries both meanings separately, which restores the distinction [0021](decisions/0021-typed-external-identities.md) collapsed.

Where an identity resolves, the local declaration is checkable against the
registry record. A part declared `Promoter<Tetracycline>` whose SynBioHub record
carries `SO:0000316` is a diagnostic. This is a genuinely new class of error and
it is one biologists actually make.

Resolution stays hermetic. Fetching is an explicit step that writes a vendored,
hash-locked SBOL document into the project, exactly as `lab.lock` does for
packages. `sbol3` ships `CachingHttpResolver` and `FileResolver`; the compiler
proper reads only the vendored file. `lab.lock` already carries a
`[packages.source] kind` discriminator with room for another kind.

This also addresses the failure mode [0021](decisions/0021-typed-external-identities.md)
records in its Consequences, where a renamed symbol silently turned "use stock"
into "build it". An identity that resolves cannot be renamed into nothing.

## Layer 2: sequences stop being opaque

`DNA` is a nominal type with one constructor, `dna(String)`, and no structure.
The design dialect's verifier checks that a sequence is non-empty, uppercase,
and `ACGT` only. There are no IUPAC codes, no coordinates, no features, no
strand, no circular origin.

Backed by `sbol3::Sequence` and `sbol_utilities::compute_sequence`, three things
move to compile time.

**A composite's sequence is derived, not restated.** `composite_plasmid_1`
currently states a full `sequence` *and* `components = [J23101, B0034, GFP, B0015]`,
with nothing checking they agree. `compute_sequence` builds a sequence from
ordered sub-components chained head-to-tail with `meets` constraints. So a
design stating both must have them match, and a design stating only components
gets its sequence computed. The disjunction in

```lab
declares sequence or (backbone and components) or (backbone and cargo)
```

stops hiding a possible inconsistency and becomes a real derivation.

Two constraints to design around. `compute_sequence` requires the features to
form exactly one unambiguous linear chain of `meets` constraints, with a single
head and full coverage, and each part must resolve to a Component carrying
exactly one sequence; anything else is rejected. That is a fine fit for an
ordered `layout:`, and a poor fit for anything branching.

More importantly, SBOL locates features with `Range { start, end }` validated as
`end >= start` and `end <= length`. Topology is only a type term, and no
coordinate machinery is aware of it. So a feature spanning the origin of a
circular sequence is a hard validation error, not a representable thing. Every
plasmid in the examples is circular, and `require topology == circular` is
written on each one, so this will be met early rather than as an edge case.

**Restriction sites are counted.** `docs/language/specimens` and the extended
example already write

```lab
require sites(BsaI) == 0
```

and `sites(RestrictionEnzyme) -> Integer` is declared in the prelude with no
implementation that reads a sequence. A Golden Gate design with an internal BsaI
site is a bench failure the language already knows how to describe and the
compiler cannot yet evaluate.

Real sequences make it evaluable, but not for free: sbol-rs has no subsequence
search, no reverse complement, and no restriction-site machinery. What SBOL
supplies is the sequence, its encoding, and the enzyme's identity; the search is
Lab's to write, including the two cases that make it non-trivial, matching the
reverse strand and matching across the origin of a circular topology. That is a
small, well-specified piece of work, and a candidate to contribute upstream
rather than keep private.

**External sequences become input.** `sbol-genbank` and `sbol-fasta` mean a
plasmid can name a `.gb` file and the compiler reads its features into
sub-components. An existing GenBank library becomes Lab input without retyping.

## Layer 3: circuits are interaction graphs

This is the deepest correspondence, and the one that makes SBOL output
scientifically interesting rather than merely structurally complete.

```lab
circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    B0034
    coding
    B0015
```

The type parameters are not decoration. `Promoter<Tetracycline>` says this
promoter responds to tetracycline. `CDS<GreenFluorescentProtein>` says this
coding sequence produces GFP. Those are SBOL Interactions:

| Lab | SBOL 3 |
| --- | --- |
| `circuit ... layout:` | `Component` roled `SO:0000804`, ordered `SubComponent`s joined by `meets` `Constraint`s |
| `Promoter<S>` | `Interaction` typed `SBO:0000170` (stimulation) where `regulation` is `induced` and `SBO:0000169` (inhibition) where it is `repressed`; `Participation` of `S` as `SBO:0000459` (stimulator) or `SBO:0000020` (inhibitor), promoter as `SBO:0000598` |
| `Both<A, B>` | one `Interaction` per combined signal, each participating in the same promoter |
| `Operon<A, B>` | one genetic-production `Interaction` per product of the unit |
| `CDS<P>` | `Interaction` typed `SBO:0000589` (genetic production); CDS as `SBO:0000645` (template), `P` as `SBO:0000011` (product) |
| `Circuit<S, P>` | the composite's `Interface`: `S` an input, `P` an output |
| `any Signal` | a `VariableFeature`, or a `Collection` of variants |

`sbol3::constants` already exports every one of those IRIs as a zero-cost
`Iri::from_static`.

Two consequences.

**Export.** Lab designs land in SynBioHub as queryable regulatory networks
rather than opaque blobs. The ecosystem compatibility is earned by the type
system instead of bolted on beside it.

**Import.** The inverse is a type inference problem the compiler can solve.
Given a Component with a genetic-production Interaction whose product is a GFP
Component, infer `CDS<GreenFluorescentProtein>`. `lab import <iri>` turns a
registry part into a typed Lab declaration a person can read.

A convenient accident: `layout:` is parsed, type-checked, and preserved into the
checked IR as `CheckedSection`, and nothing downstream reads it. An artifact-level
section is explicitly rejected with "section has no semantics yet". The ordered
structure SBOL needs is already there, already checked, and currently inert.

One limit to plan around: `sbol3::design::Design`, the arena, mints only
Component, Sequence, SubComponent, and Constraint. Interactions, Participations,
and Interfaces need `Interaction::builder(...)` and a single
`Document::from_objects` at the end. The emitter is the arena for structure plus
direct builders for the functional layer.

## Layer 4: provenance is PROV-O

This is the layer that argues SBOL rather than LabOP is the right foundation.
`marpaia/labop` had to write "LabOP records no lineage" and "material linearity
is not represented" into an omissions report. SBOL 3 includes PROV-O natively,
and `sbol-rs` models it as first-class typed classes: `Activity`, `Agent`,
`Plan`, `Association`, `Usage`, with `wasDerivedFrom` and `wasGeneratedBy` on
every Identified.

The correspondence with `provenance.rs` is exact:

| Lab | SBOL 3 and PROV-O |
| --- | --- |
| `build plasmid p`, the design | `Component` |
| the realized material | `Implementation` with `built -> Component` |
| `Origin(usize)`, a lineage beginning | `prov:Activity` |
| `Provenance::From({o})` | `prov:wasGeneratedBy -> o` |
| `Provenance::From({a, b})` | two `wasGeneratedBy`, which is how a reader recomputes independence |
| `EachFrom(o)`, the colonies of one pick | one Activity generating a `Collection` of Implementations |
| `across 3 biological replicates` | three Implementations with distinct `wasGeneratedBy` |
| `accept concentration >= 100 ng/uL` | `ExperimentalData` and an OM `Measure`, gathered in an `Experiment` |
| the workflow that built it | `prov:Plan`, with `Association.hadRole = DBTL_BUILD` |
| the allocated Asset and its execution adapter | `prov:Agent` |

The property that matters: **independence survives the round trip.** A third
party reading Lab's output can recompute which samples are biological replicates
and which are technical, because `wasGeneratedBy` carries exactly what `Origin`
carries. Lab's headline scientific claim becomes checkable by someone else's
tool, on a published document, in a standard vocabulary. Nothing else in the
ecosystem publishes that. The pseudo-replication check stops being a property of
one compiler and becomes a property of the artifact.

## Layer 5: combinatorial designs get a form

`examples/golden-gate` is a two-by-two design, two promoters against two chassis,
written as four near-identical plasmid declarations, four near-identical strain
declarations, four near-identical workflows, and four lines of `main`. Roughly
two hundred lines describing four points in a design space that is never stated.

SBOL has exactly this. `CombinatorialDerivation` with `VariableFeature`s
carrying cardinality, and `sbol_utilities::expand_derivations` to expand it.

A Lab form needs no grammar production, because a pure function in a property is
already how the language composes values:

```lab
build panel reporter_panel:
  template = composite_plasmid
  variants = [
    vary(promoter, [J23101, J23106]),
    vary(chassis, [DH5alpha, BL21]),
  ]
```

That lowers to a `CombinatorialDerivation`, expands before planning, and the
expansion is what the build graph sees. The compiler reports the panel is four
strains before anyone pipettes, and the published document states the design
space rather than four accidents that happen to resemble each other.

`expand_derivation` currently rejects templates carrying interactions or
interfaces, so circuit templates are out at first. Since sbol-rs is maintained
in-house that is a contribution rather than a wall.

## How deep this goes

The layers above put SBOL at the frontend and at emission. The question worth
asking is whether it belongs in the middle too, and the answer is yes in five
places and no in three. The line between them is not arbitrary:

> SBOL owns what a thing is and where it came from. Lab owns what may be done
> with it, in what order, and on what evidence.

Every case for going deeper sits on the first side of that line. Every case
against sits on the second. This is why the two compose rather than compete:
SBOL cannot express affine material flow or refuse pseudo-replication, and Lab
should not be reinventing sequence identity or the Sequence Ontology.

### Identity, from the checker onward

This is the deepest change and the one the rest depend on.

Lab already agrees that identity is a pair of scope and name:

```rust
pub struct ModuleId(String);

pub struct DefinitionId {
    pub module: ModuleId,
    pub local: String,
}
```

That is the same shape as `sbol3::SbolIdentity { namespace: Namespace, display_id: DisplayId }`.
The difference is that one resolves and the other does not.

The sharper finding is that this identity is already minted and already
threaded, and nothing consumes it. `CheckedExpression::Reference` carries a
`DefinitionId` beside its path; `ModuleExport` carries one for every export.
Both are written by the checker. No pass in `lab-lair`, `lab-ide`, or
`lab-language-server` reads either, and the only references outside the checker
are in tests. Every real consumer takes `path.first()` and works with the bare
word instead.

`DefinitionId::source`, the constructor meant for scoped declarations, is never
called at all. Its doc comment describes a scheme ("the declaration name plus
its byte offset so future scoped declarations cannot collide") that no code
uses. A byte offset is in any case a placeholder for a scope, and SBOL specifies
the thing it is approximating: a top-level object is `{namespace}/{displayId}`,
a child is `{parent}/{displayId}`, so a scoped declaration is a child of its
scope. `marpaia/labop`'s per-parent counter already implements that convention.

So this is not a new identity concept and not even a new field. Lab designed a
structured identity, decided its shape, and then routed every consumer around it
through bare strings. The work is finishing the wiring and making the result
resolvable. What follows from it:

- Editor navigation stops being lexical. `Workspace::definition` finds the first
  declaration whose `name` string matches, and `Workspace::references` matches
  identifier text across every open document with no scope or module awareness.
  `rename` is built on `references`, so today a rename rewrites every
  textually-identical identifier in every open file. The support matrix calls
  this "name-based fallback pending symbol identities/scopes"; resolvable
  identity is the thing it is pending on.
- The map from an SBOL object back to a source span becomes a byproduct of
  lowering rather than separate bookkeeping, so any SBOL-side diagnostic can
  point at source.
- A run record can name the same thing the design names, which is what
  closes the loop below.
- A build becomes content-addressable. There is no digest API in sbol-rs, but
  `normalized_triples()` sorts and dedups into a deterministic total order,
  which is a usable canonical form as long as no blank nodes appear. Note that
  `Graph::write` serializes the unsorted vector, so a digest has to sort first
  and serialize itself rather than hashing `write(NTriples)`.
- Two imports may export the same word. Today they cannot:
  `insert_imported_name` rejects a collision outright with "imported name 'x' is
  ambiguous between 'a' and 'b'", because a bare word is the only identity
  available to disambiguate with. That restriction is a symptom, not a policy,
  and it gets worse as soon as third-party part catalogs are dependencies.

One caution about scope. `LineageMap` keys on binding names within a workflow
body, and `material_flow.rs` tracks ownership as `Place(Vec<String>)`, a dotted
access path. Those are *local* names, not declarations, and an IRI is the wrong
tool for them: what they want is SSA value identity, which LAIR already has.
Making declaration identity resolvable and leaving binding identity alone is the
correct split, and conflating the two would be the easiest way to make this
change much larger than it needs to be.

### Design LAIR becomes an SBOL document

The first design dialect did not use pliron for the relationship between a
design and its sequence. It copied the sequence into an attribute:

```rust
#[pliron_op(
    name = "design.plasmid",
    attributes = ( artifact_name: StringAttr, sequence: StringAttr, ... ),
    interfaces = [NOpdsInterface<0>],
    results = (design: DesignType)
)]
pub struct DesignPlasmidOp;
```

Decision [0043](decisions/0043-sequences-are-first-class-design-values.md)
removed that particular flattening. `design.dna_sequence` now defines a typed
SSA value containing the sequence identity and elements, and `design.plasmid`
consumes it as an operand. Named `DNA` source bindings lower once and can feed
several designs; inline `dna("...")` is represented by the same operation with
a synthetic name.

`design.strain` and most other design relationships still use a flat attribute
bag. The workflow dialect consumes each design value,
`workflow.realize` declaring `operands = (design: DesignType)` and
`workflow.transform` declaring `operands = (design: DesignType, cells: MaterialType)`,
and those operands carry the use-def edges `MaterialLinearityAnalysis` walks to
enforce affinity. So the design layer is a source in the dataflow graph.

But the remaining design-to-workflow edge is still rebuilt during lowering from
a string map:

```rust
let mut designs = BTreeMap::new();
// ... designs.insert(artifact.name().to_owned(), design);
let design = designs[&name];
```

including the raw index, which panics rather than diagnoses if the name does not match. And the edges that actually describe the design space are not SSA at all: `realize_components`, `realize_dependencies`, and `strain_plasmids` are `VecAttr` of `StringAttr`. Current Method refinement turns those exact source parameters into typed Procedure values and material bindings, but the Design layer still lacks SBOL identities on its SSA values.

So a graph is being encoded as strings inside a system that already has a graph.
The rest of the design dialect is a hand-maintained re-encoding of what SBOL
specifies, including a bespoke verifier that checks a sequence is "non-empty,
uppercase, and unambiguous DNA".

One concrete blocker to note before starting: both design ops verify
`pliron::identifier::Identifier::try_from(artifact_name)`, which constrains an
artifact name to a bare identifier and would reject any IRI. Carrying identity
into LAIR needs an attribute type for it rather than reuse of `StringAttr`.

`IrStage::Design` is already a named, standalone, verifier-valid stage, so the seam exists. Make that stage an `sbol3::Document` and reduce the pliron design op to a reference carrying an IRI. Workflow, Method, Procedure, Capability, and Allocation LAIR are untouched.

Two payoffs beyond deleting code. The bespoke verifier is replaced by 109
machine-checkable spec rules. And ADR 0022's unfinished half comes within reach:
`program/lowering.rs` currently matches `"plasmid"` and `"strain"` by name and
reads fields by name, but a Component carrying type and role terms is generic,
so the special-casing has somewhere to go.

### SBOL validation as a checker pass

Not an output gate. `compile_parsed_module` is two passes today:

```rust
let checked = checker::check_module(module_id, environment, module)?;
material_flow::verify_module(&checked, environment)?;
```

An SBOL verification pass is a peer of the second, and belongs beside it for the
same stated reason: it "runs after semantic checking so it never has to
reinterpret source syntax".

Rules carry stable string ids and a closed `Hint` enum, so rendering them is
mechanical rather than prose-scraping. But two shipped facts constrain where
this pass can run, and both need stating plainly because they contradict the
obvious design.

**Rule selection does not work in SBOL 3 as shipped.** `ValidationConfig`
advertises `complete`, `compliant`, `types_in_uri`, and `keep_going`, and it
reads as though per-module checking could run with `complete: false` and the
program boundary with `complete: true`. It cannot. Not one of the 149 rules in
`crates/sbol3/rules.toml` carries a gate, so `complete: false` skips nothing;
`types_in_uri` and `keep_going` are never read on the SBOL 3 path at all. Only
`best_practice` has an effect, and by a different mechanism, suppression at emit
time. Per-rule `allow` is available but explicitly does not save work: the check
runs and the diagnostic is discarded.

The machinery is real, just unwired here. `crates/sbol2/rules.toml` gates 70
rules and its validator branches on them. Making the two-tier split work is an
upstream change, adding gates to the SBOL 3 catalog, not a Lab-side one.

**There is no partial validation.** `Validator` is `pub(crate)`, every entry
point hangs off `Document`, and rules resolve references through the document.
So a validation pass must materialize a full `Document`, and
`Document::from_objects` costs roughly two deep copies: it lowers every object
to triples, then re-parses those triples back into a property-bag map. A
`Document` itself holds its data three times over, as triples, as a property
map, and as typed objects, with `Iri` backed by `Cow` rather than a refcount and
no interning anywhere.

That is affordable once per module compile. It is not affordable per keystroke,
which is a second reason the pass belongs in `lab-project` rather than in the
path `lab-ide-wasm` drives. Cheap per-object checks that the editor can run
belong on `sbol_core::syntax` and the `DisplayId` and `Namespace` newtypes,
which validate at construction with no document at all.

### A package can be an SBOL document

`ModuleInterface` is serde, self-describing, and injected through one function:

```rust
pub struct ModuleInterface {
    pub module: ModuleId,
    pub documentation: String,
    pub exports: BTreeMap<String, ModuleExport>,
}

pub fn insert(&mut self, visible_name: impl Into<String>, interface: ModuleInterface)
```

Synthesizing one from an SBOL document is mechanical. A `Component` becomes a
`ModuleExport`; its `roles` map back to Lab role names through the same term
table Layer 0 establishes; its sequence and type become fields.

This answers the open question directly. A parts catalog stops being Lab source
compiled into `std` and becomes a vendored, hash-locked SBOL document that a
registry can export and a project can depend on. `open-questions.md` asks how
catalogs expose sequences, provenance, and versions "without reducing them to
untyped properties or compiling changing catalog contents into `std`". This is
how.

### Facility catalogs and execution records have distinct owners

SBOLInventory supplies the persistent facility graph that core SBOL 3 deliberately does not define. Its extension classes and properties express the stable catalog and ledger facts:

```text
a facility contains zones
zones locate assets and material lots
assets expose capability offerings
workflows require capabilities
plans bind requirements to offerings and assets
runs record material changes and evidence
```

The first three statements and the persistent MaterialLot catalog belong to SBOLInventory. Workflow requirements belong to compiler IR. Allocation and scheduling belong to the Lab planner. The reviewed plan freezes every requirement-to-offering-to-Asset binding, exact MaterialLot binding, adapter profile, document digest, and dependency edge without translating the facility graph into a private TOML inventory model.

The runtime then appends standard SBOL and PROV structure to a new inventory document. One completed plan becomes an `Activity`; exact Assets and input MaterialLots become qualified `Usage` entities with SBOLInventory roles; the reviewed plan, ledger, adapter profiles, and child documents become hashed `Attachment` evidence; and each live output MaterialLot is an `Implementation` carrying `prov:wasGeneratedBy` and exact lineage. The source inventory is never modified in place.

The reviewed execution DAG remains a versioned Lab document rather than pretending that RDF is an ordered dispatch language. `Execute`, `MoveMaterial`, and `Manual` nodes need explicit dependencies, replay rules, confirmations, and a durable event ledger. The runtime is keyed by the reviewed plan digest and never re-plans from the graph.

This boundary also preserves claim strength. A catalog offering may be `Described`, `Plannable`, `Simulatable`, `Executable`, or `Qualified`, and the planner may bind it only when the workflow's minimum qualification is met. An adapter declaration does not promote catalog qualification, and catalog product metadata never selects a driver. Simulation and live execution use incompatible ledger state, and only live execution may mint physical output MaterialLots.

The resulting chain is one identity-preserving graph: a design Component is realized by an exact MaterialLot, consumed by an Activity through an exact Asset offering, and related to any generated MaterialLots and evidence. Lab keeps compiler and dispatch state private while emitting the durable laboratory record through the SBOLInventory profile.

### Where it should not go

**Workflow, Method, Procedure, Capability, or Allocation IR as RDF.** Triples are an unordered set with no use-def edges. `material_flow.rs` tracks ownership as `Place(Vec<String>)` over ordered statements, and `MaterialLinearityAnalysis` walks SSA use-lists. `marpaia/labop` already ran this experiment and reported the result: Golden Gate cycling written out one incubation at a time because "a LabOP activity has no loop construct", and "material linearity is not represented" filed in its omissions report.

**`CheckedModule` holding sbol3 types.** There is no serde anywhere in sbol-rs,
and the portable module IR is versioned and must stay self-describing across
compiler versions. IRIs travel there as strings and objects are rebuilt at the
SBOL boundary.

**Ontology-derived subtyping.** This one is not available, and the reason is
worth recording because the API makes it look available.

Role membership is a flat, non-transitive lookup today:

```rust
fn plays_role(roles: &RoleTable, actual: &Ty, role: &str) -> bool {
    matches!(actual, Ty::Named(name, arguments)
        if arguments.is_empty()
            && roles.get(name).is_some_and(|played| played.contains(role)))
}
```

`sbol-ontology` exposes `is_descendant`, a genuine recursive walk over each
term's parents, so the natural design is to close
`type_roles: HashMap<String, BTreeSet<String>>` over the ontology once when the
semantic context is built, leaving `plays_role` untouched and confining the
ontology's influence to one construction step.

The bundled snapshot will not support it. It carries 15,951 terms, of which
**106 are SO**, and the SO hierarchy in it is flattened: `SO:0000167` (promoter)
has a single parent, `SO:0000110` (sequence_feature). The real intermediate
terms are simply absent, so a descendant query against anything but
`SO:0000110` returns false, silently, because the ancestor is unknown. Promoter,
CDS, RBS, terminator, operator, and engineered region are all reparented the
same way. The snapshot is a validation aid for the terms the SBOL spec needs,
not a taxonomy.

The extension path does not rescue it either. `Ontology::from_tsv_path` is
public and `build_extension_tsv` does preserve within-subtree parent chains, but
`parse_namespace` accepts a closed set of seven ontologies and `parse_role` a
closed set of seven SBOL-facing roles, so a `LAB:` namespace or a
compiler-specific term role is rejected outright. And `extend_with` uses
`or_insert`, so bundled facts win: a supplied full-fidelity SO cannot repair the
flattened parents. Forking the bundled TSV is the only local option, which is
the wrong shape for a dependency.

So: ontology-derived subtyping is upstream work in `sbol-ontology`, a fuller SO
import that keeps real parent chains plus open namespace and role registration.
It is feasible, since sbol-rs is maintained in-house, but it is a separate piece
of work with its own risk and should not be assumed by anything on the Lab side.
Until then the ontology validates terms and does not classify types, which is
also the conservative place to start.

Two details worth carrying forward whenever it does land. `has_ancestor` is
unmemoized and has no cycle guard, so a closure pass should compute its own
rather than call it per query. And if the type system ever does depend on the
snapshot, the snapshot becomes a dependency and belongs pinned in `lab.lock`;
`sbol-ontology` already carries a `TSV_FORMAT_VERSION` and per-source
`raw_sha256` to pin against.

One detail worth copying rather than fighting: `terms_conflict` returns
`Option<bool>`, where `None` means one of the terms is not in the snapshot. That
is the same discipline `provenance.rs` applies when it refuses to guess, and a
check reading it should stay silent for the same reason.

## Can a Lab program be written in SBOL?

**Design under construction.** Three different questions hide in that one, and
they have different answers.

### The three questions

**Can SBOL be an authoring format for designs?** Yes, and this is where the
value is. A `Component` with a sequence, ordered sub-components, roles, and
types is exactly what `build plasmid` and `circuit ... layout:` say. Designs
authored in SynBioHub, Benchling, or a GenBank file become Lab declarations with
nothing invented.

**Can a whole Lab program round-trip through SBOL?** Technically yes, by
defining `lab:Workflow`, `lab:Action`, and `lab:Requirement` as
`IdentifiedExtension` top-levels and hanging the rest off extension triples,
which sbol-rs preserves faithfully. What you get is Lab's AST encoded in RDF. No
other tool understands the workflow half, so you pay RDF's ergonomics for the
part SBOL already covered and gain nothing for the part it did not.

**Can the whole language be authored in SBOL?** No, for the program half, and
the reason is a category difference rather than a missing feature.

### Why the program half cannot move

SBOL describes biological structure and history. As a language it is unordered,
has no binder, no expression language, no type variables, and no notion of a
value being consumed. Lab's program half is exactly those five things.

The nearest thing SBOL has to a predicate is `Constraint.restriction`, and it is
a fixed sixteen-value vocabulary of spatial relations between features within
one component: `meets`, `precedes`, `contains`, `overlaps`, `sameOrientationAs`
and the rest. It relates two features. It cannot say `sites(BsaI) == 0` or
`concentration >= 100 ng/uL`, because there is nowhere in the model for an
operator, a function call, or a comparison to live. `CheckedExpression` has
eleven forms including `Call`, `Unary`, and `Binary`; SBOL has none of them.

Generics are the sharpest case. `CombinatorialDerivation` with `VariableFeature`
looks like a type parameter and is not: it enumerates concrete variants for a
slot. `Promoter<Trigger: Signal>` is a function over types with a bound, and
`Circuit<any Signal, GreenFluorescentProtein>` is an existential that
deliberately forgets its witness while pinning the product. Neither has any
analogue, and neither is reachable by extension without inventing a type theory
in RDF.

### Feature by feature

| Lab construct | In SBOL |
| --- | --- |
| `artifact X is NucleicAcid, EngineeredRegion` | `Component.types` and `.roles`. Native, implemented |
| `build plasmid p: sequence, components` | `Component` + `Sequence` + `SubComponent` + `meets` `Constraint`s. Native |
| `buy part J23101` | `Component` at a registry identity. Native |
| `circuit ... layout:` (structure) | ordered `SubComponent`s. Native |
| `require topology == circular` | `SO:0000988` as a type term. Native, because topology *is* a term |
| a combinatorial panel | `CombinatorialDerivation` + `VariableFeature`. Native |
| lineage, replicate independence | `prov:wasGeneratedBy`. Native |
| `Quantity<uL>` | OM `Measure` and `Unit`. Native, minus the missing constants |
| `record` with `case` constructors | extension only |
| `artifact` field schemas | extension only; SBOL has no schema language |
| `Part \| Plasmid` unions | extension only |
| `declares sequence or (backbone and components)` | extension only |
| `require` / `accept` predicates | extension only, as opaque text |
| `across 3 biological replicates` | extension only |
| `Promoter<Trigger: Signal>`, `any Signal` | **not expressible** |
| `workflow`, `x <- action` | **not expressible** |
| affine material flow | **not expressible**: no ordering, no use-def |
| `state`, `when every 30 min` | **not expressible** |
| `if`, `match`, `for`, `emit` | **not expressible** |

The bottom group is not an accident, and it is worth saying plainly: those are
the features that justify Lab existing. If SBOL could express affine material
flow, lineage-based replicate class, and the type-level link between an inducer
and the circuit it induces, Lab would be a syntax for SBOL and little else. The
ceiling is real, and it is in the right place.

### What is worth building instead

Split the package rather than the language. **Designs move to SBOL; workflows
stay in Lab.**

That is not a consolation prize. In `examples/golden-gate-extended` the designs
are 302 lines against 163 of workflows and 47 of program, so the design half is
the larger one, and it is also the half that varies between laboratories, the
half that already exists in registries, and the half SBOL covers natively. A
Lab package whose `designs/` directory is an SBOL document and whose
`workflows/` directory is Lab source is most of the way to the goal.

Concretely, `use` resolves an SBOL document as a module. The rungs:

1. **Recognizing a kind from terms.** *Implemented*, as `lab_sbol::KindIndex`.
   `Grounding` maps a type to its terms; this inverts it, so a `Component` typed
   `SBO:0000251` and roled `SO:0000167` is a `Promoter`. A candidate is a kind
   whose every term the object also states, and among candidates one that
   another strictly contains is discarded, so the most specific kind wins and an
   object may be more specific than the vocabulary reading it.
2. **A reader for atomic designs.** *Implemented*, as `lab_sbol::read_designs`.
   Every `Component` becomes a `CheckedDeclaration::Catalog` carrying the
   registry's own IRI as its identity, so an import stays resolvable back to
   where it came from.
3. **Composite designs.** *Implemented.* Sub-components become `components` and
   `Sequence` objects become `sequence`, the same `std.bio.dna` call an author
   writes, so a design read from a document and one written by hand reach a
   backend identically. Each element carries the kind of the part it names, so a
   plasmid used as a component is a `Plasmid` rather than silently a `Part`.
4. **A checked module from a document.** *Implemented*, as
   `lab_sbol::read_module`. The reader builds declarations and hands them to the
   checker rather than forging checked IR, so a design read from a document is
   subject to every rule a design typed by hand is, with one implementation of
   those rules and no second one to drift.
5. **Resolvable identity for the other direction**, so a Lab-authored design
   mints an IRI rather than only consuming one.
6. **Interaction to type-argument inference.** An `Interaction` typed
   `SBO:0000589` whose template is a CDS and whose product is a GFP component
   yields `CDS<GreenFluorescentProtein>`. This is the hard and interesting part,
   because it is what recovers the type parameters Lab's checking runs on. It
   will work for the standard patterns and will not be total, so it needs to
   fail loudly rather than guess.
7. **Discovery.** *Implemented.* A `.ttl`, `.nt`, `.jsonld`, or `.rdf` file
   under `src/` is discovered, named, ordered, and compiled the way a `.lab`
   file is, so a package's designs can be written in either language. This is
   the rung that delivers the goal end to end: `examples/golden-gate-extended`
   now keeps its DNA parts in `src/designs/parts.ttl` and builds robot
   protocols from them.

### What the first two rungs settled

**Terms under-determine the kind, and that is not a defect to engineer around.**
`Backbone` and `Plasmid` both ground as an engineered region of nucleic acid,
and the bundled Sequence Ontology snapshot has no `plasmid` or `vector` term to
separate them. Nor should SBOL have one: the difference is not biological, it is
which part each plays in an assembly, and the vector you cut open is a plasmid.
So a term set names one kind, several, or none, and the reader reports which:

```
'https://.../pSB1C3' is a Backbone or a Plasmid, and nothing in the document says which
```

A document Lab wrote settles it by stating `lab:kind` in Lab's own namespace,
which is honoured ahead of inference, so a round trip recovers what the author
meant instead of re-deriving it from terms that cannot carry the distinction.
Nothing else needs that predicate, and a third-party reader ignores it.

**An imported design is catalogued, not built.** Whether a laboratory builds a
plasmid or orders it is a fact about that laboratory rather than about the
design ([0027](decisions/0027-provenance-is-stated-per-thing.md)), and a
registry has no opinion. Reading an import as `buy` is the honest default,
because a registry listing something is exactly the claim that you can obtain
it.

**One unreadable component does not cost the document.** A registry export is
large and partly outside any one program's vocabulary, so components that cannot
be read are collected beside the ones that could and the caller decides which
problems are fatal. Refusing the whole file over one unrecognized term would
make the feature unusable against real registry data.

**There is no residue channel, because widening the mapping is the better
answer.** An imported component states more than a Lab kind declares, and the
obvious response is a side-channel carrying whatever did not fit. The better one
is to make the kinds cover what SBOL actually specifies, so there is less that
does not fit. [0028](decisions/0028-schemas-are-contributed-to.md) is already
the mechanism: several packages describe one kind, so a module can contribute
the fields an SBOL statement needs without touching the kind that declared it.

Most of the specified content maps once you look. A component's sub-components
are `components`; its sequence is `sequence`; its types and roles are grounding;
its measures are quantity properties; its name and description are
documentation. `meets` constraints and list order are the same fact stated twice,
so the chain is walked once on the way in and the coordinates it implies are
recomputed rather than carried. That is normalization, not loss.

What genuinely cannot be covered by any fixed schema is the open-ended part:
annotations other tools attach in their own namespaces, `Attachment` files, and
`Model` references to external SBML. Those are unbounded by construction, and no
Lab kind can anticipate their union. So the question shrinks from "what do we do
with everything that did not fit" to "what do we do with third-party
annotations", which is a much smaller question and can be answered later without
blocking anything.

The rule the reader holds meanwhile: **it emits no property a schema does not
declare.** That is not a discipline it keeps, it is a fact the checker enforces,
because the reader builds declarations and hands them to the checker rather than
forging checked IR. A test drives it by adding a `sequence` to an antibiotic and
asserting the compile fails.

### What discovery settled

**Imports are derived, not configured.** A document names the terms its
components stand for and never says which Lab package describes them, so
requiring a sidecar or a manifest key to state the imports would make every
registry export need a hand-written companion. Instead `Grounding` records which
module declares each kind, and a document imports the modules declaring exactly
the kinds it turned out to use. A registry export is readable as it comes, and a
document that grows a new kind of part needs nothing written for it.

**A document depends on nothing inside its own package.** It states components
and terms, never a sibling module, so it is ready to compile before any Lab
module and the existing dependency ordering needs no special case.

**`.xml` and `.json` are deliberately not recognized.** Either could be several
things, and guessing wrong produces a parse error that blames the document
rather than the guess. An SBOL document names its serialization in its
extension or it is not discovered.

**Moving designs into SBOL does not change what an order names.** An imported Component carries its registry IRI as `sbol_identity`; a bought declaration separately carries `supplier_identity`, defaulting to its Lab symbol when no order identifier is stated. Inventory-backed planning follows only `sbol_identity -> sbol:built -> MaterialLot`, while device manifests may still use the supplier identifier.

### Widening the mapping found three real modelling gaps

Running the checker over what the reader built immediately rejected a composite:

```
plasmid property 'components' expects List<Part | Plasmid>, found List<Promoter | Part | CDS>
```

That was correct, and the schema was wrong. A design is assembled from promoters
and coding sequences as readily as from bare parts, and `List<Part | Plasmid>`
admitted neither. Enumerating the kinds would have produced a list that every new
kind of part has to be added to, so the field is now:

```lab
components?: List<any NucleicAcid>
```

The role that makes this expressible is the ontology grounding from Layer 0. A
term introduced so designs could be *exported* in a shared vocabulary turned out
to be the right bound for a Lab type, which is the clearest evidence so far that
the two models really are the same model at two altitudes. Hand-forged checked IR
would have accepted the bad list silently; the checker is what found it.

The second gap was quieter and would have been worse. `Part`, `Promoter`, `CDS`,
and `Backbone` declared no `sequence` field at all, so importing any registry
part that publishes its sequence would have been rejected. That is nearly every
real part. Each of those kinds now declares `sequence?: DNA`; none of them
carried prelude fields before, so nothing was displaced.

The third was a desync the widening caused rather than revealed.
`program/lowering.rs` reads components with a hardcoded
`&["Part", "Plasmid"]`, so a design with a promoter component would have checked
and then failed to lower with a generic invalid-field error. The golden-gate
examples use only bare parts, so no test would have caught it. The list is
widened to match, with a comment saying it has to track the schema. Naming
kinds where the grounding is already available is
[0022](decisions/0022-fixed-grammar-open-vocabulary.md)'s unfinished business
showing up in a third place.

### The honest way to close the loop

For the protocol half, do not pretend. An emitted SBOL document can carry the
workflow that produced it as an `Attachment`, with `source`, `format`, and a
`hash` alongside a `prov:Plan` that names it. The document then says "this
design was built by that protocol, and here is its digest" without claiming RDF
understands the protocol. That is cheap, true, and enough for provenance.

Attempting more is what produced `marpaia/labop`'s omissions report.

## Where it lands in the compiler

The workspace keeps RDF and SBOL objects outside both `lab-language` and
`lab-lair`. The current ownership split is:

**`lab-language`** owns the RDF-free identity, type, role-grounding, and
ontology-validity semantics used by both native and embedded frontends. It has
no RDF or network dependency and stays light enough for `lab-ide-wasm`.

**`lab-sbol`** sits beside `lab-language` and owns the SBOL correspondence. Its
implemented direction resolves grounded kinds and imports SBOL designs or a
checked module from an `sbol3::Document`. The inverse checked-module-to-SBOL
emitter remains work for this crate rather than for LAIR or a device adapter.

**`lab-project`** owns filesystem and package orchestration. It parses and
validates selected SBOL documents, calls `lab-sbol`, and passes the resulting
checked modules into `lab-lair`. A bare `lab-language::compile_module` call and
the textual `labc` inspection tool remain independent of the RDF stack.

**`lab-adapters`** owns `ArtifactBundle` only for generated device and operator
artifacts. That type is not the representation of an SBOL design document and
does not move SBOL emission into the adapter layer.

### The IR change, shallow and deep

Two versions, and they are a sequence rather than a choice.

**Shallow.** `design.plasmid` today carries `artifact_name`, `topology`,
`copies`, and two acceptance thresholds as attributes. Its sequence is already
a `design.dna_sequence` operand under
[0043](decisions/0043-sequences-are-first-class-design-values.md).
The design gains an identity IRI and type and role term IRIs, while the sequence
gains its encoding term. That is what lets a backend stop guessing
`SBO:0000241`, and it is a small enough change to land early.

**Deep.** Once terms are attributes rather than op identity, `design.plasmid` and `design.strain` stop being distinct ops, and the stage's representation is what should change rather than its attribute list. `IrStage::Design` becomes an `sbol3::Document` and the pliron design op degenerates to a reference carrying an IRI, as argued above. Workflow, Method, Procedure, Capability, and Allocation LAIR are untouched, because that is where pliron's SSA and linearity analysis earn their place.

Both versions push on the same wall: `program/lowering.rs` matching `"plasmid"`
and `"strain"` by name and reading fields by name. That is the unfinished half
of [0022](decisions/0022-fixed-grammar-open-vocabulary.md), "this removes
biology from the frontend only", and a Component carrying roles is what it needs
in order to become generic.

### Diagnostics

`ValidationIssue` carries a severity, a stable `rule: &'static str`, a subject
Resource, an optional property, and a closed `Hint` enum. Rendering it as a Lab
diagnostic needs a map from SBOL identity back to the source span that minted it.

That map is a new capability rather than a migration, and it is worth being
clear about why. Nothing below `CheckedModule` carries a source position, by
design: "source text is deliberately absent: later compiler passes must not
reinterpret syntax". Spans went with the syntax.

pliron does give every operation a `Location`, and `Operation::new` hard-defaults
it to `Location::Unknown`, which renders as `?`. `set_loc` is called zero times
in this repository, so all thirty or so `verify_err!(self.loc(ctx), ...)` sites
in the dialects produce a message with no position at all rather than a wrong
one. Today a lowering or backend error is reported by interpolating the artifact
name into a message string and cannot be underlined in an editor.

This is a population problem, not an infrastructure problem, and pliron already
ships more of the model than is needed:

```rust
pub enum Location {
    SrcPos { src: Source, pos: SourcePosition },
    Fused { .. },
    Unknown,
}
```

`Fused` is the interesting one. A single LAIR op is often lowered from more than
one declaration, a `workflow.transform` combining a strain's declaration site
with the realizing workflow's action site, and `Fused` expresses exactly that
rather than forcing a choice between them.

So the shape is: a `BTreeMap<DefinitionId, Span>` side-table on `CheckedModule`,
threaded through `BuildArtifactIntent` in `program/lowering.rs`, which currently
carries only a name, and `set_loc` called at roughly fifteen construction sites
in `program/mod.rs`. No new location machinery.

That is worth doing for its own sake, and SBOL validation is what makes it pay
for itself: it is the first pass that produces many precise, structured findings
about specific objects.

`Hint::SuggestedTerm { iri, label }` renders as a term suggestion, which is the
diagnostics bar this repository holds itself to.

Gate `lab build` on `Document::check_complete()`. A Lab program that compiles
emits valid SBOL by construction, or it does not compile.

## What this does to the Python problem

`marpaia/python` answers "how do I write Lab from Python". That is the wrong
question, and needing a source map to report errors is the symptom.

The right question is "how does a Python lab work with Lab designs", and SBOL
answers it with no Lab-specific bridge at all:

- Lab emits SBOL. Python reads it with `sbol`, the `sbol-py` bindings, which
  wrap **the same Rust core the compiler used to write it**. No serialization
  mismatch, no second model, no generated mirror to keep current, no codegen
  staleness test.
- Analysis, plotting, LIMS integration, and notebook work happen against
  `sbol.Document`, an API that already exists, is already documented, and
  already covers SBOL 2, GenBank, and FASTA.
- The interchange is a validated standard document rather than a JSON dump of
  `CheckedModule`, so the same file works in libSBOLj3, pySBOL3, SynBioHub, and
  Benchling.

What remains for a `lab` Python package is small and honest: run the compiler,
get diagnostics, get the emitted `sbol.Document`. A subprocess wrapper and a
re-export, not 1,900 lines of expression AST and frame introspection.

The `sbol-py` README already states the principle this repository holds about
SDKs: "The API is idiomatic Python, not a clone of pySBOL3's mutable graph."
Mapping concepts onto the host language rather than mimicking a foreign syntax
is exactly what the Lab Python SDK failed to do, and it did not fail for lack of
effort. It failed because it had nothing to map onto. SBOL gives it something.

If authoring Lab from Python is still wanted later, it should generate SBOL
rather than Lab source and let the compiler import it. Errors then point at
objects, and the source-map problem does not exist.

The current Python package now takes the first half of that step. Its
`lab.sbol.Document` lazily builds an ordinary pySBOL3 document through typed
`Promoter`, `CodingSequence`, `Terminator`, `Backbone`, and `Plasmid` handles;
ordered DNA layouts create their `meets` constraints, and independent typed
`DnaSequence` and `ProteinSequence` values are referenced by designs rather
than hidden inside them. Registry IRIs remain references rather than copied
objects. Its public inputs are keyword-only, and a local design may omit its
identity: `Plasmid.build(design=...)` resolves the Lab declaration name,
materializes that identity under the document namespace, and validates the
document as part of module emission. The return types retain provenance too:
`build` returns `BuildDeclaration[Plasmid]`, while `buy` returns
`BuyDeclaration[Plasmid]`.

Typed composition does not infer provenance from an SBOL IRI. A child design is
first attached explicitly with `Promoter.buy(design=...)` or
`Promoter.build(design=...)`, and the resulting declaration is placed in the
composite. This mirrors the Lab source distinction between what a design is and
how a laboratory obtains it. Reading an unannotated third-party SBOL document
still uses the catalogued default described above, because that import has no
Python declarations from which to recover a more specific local choice.

The existing source-emitting frontend can read the resulting document today.
This is an incremental authoring boundary, not the final bridge: moving the
document through `lab-sbol` directly and retiring generated Lab source remains
the end state described here. [Decision 0041](decisions/0041-typed-sbol-authoring-separates-design-from-provenance.md)
records the Python authoring and provenance contract independently of that
future bridge.

`sbol-py` is currently `publish = false`, excluded from the workspace, and on
edition 2021 while everything else is 2024. Making this story real means
publishing it. That is a decision, not a blocker.

## Sequence of work

Ordered so that each step is useful on its own and none depends on a later one.

1. Ontology grounding. The identity tail on `role`, the `Ty` to term resolver,
   and the `sbol-ontology` checks. Small, self-contained, and everything else
   depends on it.
2. Identity. Package namespace in `lab.toml`, `identity.rs` salvaged into the
   frontend, `DefinitionId` made resolvable and actually read by its consumers,
   IRI identities distinguished from catalog numbers. The dead `name@offset`
   constructor is replaced by SBOL's child-naming convention. One schema bump.
3. The span side-table. `BTreeMap<DefinitionId, Span>` on `CheckedModule`,
   threaded through `BuildArtifactIntent`, and `set_loc` populated at the
   fifteen or so LAIR construction sites, using `Location::Fused` where an op
   derives from more than one declaration. Independently useful: it is what lets
   any lowering or backend error be underlined at all.
4. SBOL emission from Design LAIR: Components, Sequences, SubComponents,
   Constraints. Gate on `check_complete()`. The `SBO:0000241` fallback goes away.
5. Validation as a pass rather than a gate, run from `lab-project::compile`.
   Whole-document only, once per module compile, not in the editor loop. The
   per-module and per-program split waits on rule gates landing upstream.
6. Editor navigation on resolved identity. `definition`, `references`, and
   `rename` consult `CheckedModule` instead of matching identifier text. This
   needs nothing from SBOL beyond step 2 and fixes a rename that today rewrites
   every matching word in every open file.
7. Sequence computation and cross-checking. GenBank and FASTA import. `sites()`
   gets an implementation.
8. Interactions and Interfaces from circuit types, reading `CheckedSection`.
9. PROV-O from the lineage analysis: Implementation, Activity, Experiment,
   ExperimentalData.
10. Import. SBOL to typed Lab declarations, `lab import <iri>`. A dependency may
    be an SBOL document rather than Lab source.
11. The run ledger and coordination plan emit PROV-O against the same
    identities, closing design to run to material.
12. Combinatorial derivations.
13. `IrStage::Design` becomes an `sbol3::Document`. This is last because it is
    the only step that is purely a simplification: by then everything it would
    carry is already being produced.
14. Retire the Python source generator. Publish `sbol-py`. Ship a thin `lab`
    package.

Steps 2, 3, and 6 are worth doing whether or not the SBOL work continues. That
is a useful property for the first third of a plan this size.

Four items are upstream work in sbol-rs rather than Lab work, and they gate
parts of the above. Rule gates in the SBOL 3 catalog, so validation can be tiered.
OM unit constants, so quantities emit as `Measure` values. A fuller SO import
with real parent chains plus open namespace registration, if terms are ever to
classify types. And origin-aware coordinates, if circular features are to be
located rather than only counted. Since sbol-rs is maintained in-house these are
schedulable rather than blocking, but they are a second track and should be
planned as one.

LabOP stays as a secondary emission derived from the same SBOL document, with
its omissions report intact. It is a projection of the output, not a peer of it.

## Open questions and risks

**Portable SBOL identities are strings by design.** `CheckedModule` is serde-serialized under `lab.portable-module.v8`, so pySBOL3 and sbol-rs objects do not ride inside portable compiler IR. `sbol_identity` carries the exact absolute Component IRI as a string, while typed SBOL objects remain behind the authoring and inventory boundaries.

**No OM unit constants in sbol-rs.** Lab has `Quantity<uL>`, `Quantity<C>`, and
`Quantity<min>`, and emitting them as OM `Measure` values needs unit IRIs that
`sbol3::constants` does not carry. `marpaia/labop`'s `vocabulary.rs` already has
`Unit::{Microlitre, Celsius, Minute}` to salvage; contributing the constants
upstream is the better end state.

**Namespaces are a policy call.** Whether a package must declare one, and what a
project without a SynBioHub account uses. A `https://lab-compiler.org/local/<package>`
default works but publishes IRIs that do not resolve, which is its own kind of
lie.

**Round-trip fidelity is a two-way claim and needs a test.** Lab-specific
properties such as `reaction_volume` and `assembly_cycles` ride as extension
triples under a `lab:` namespace and survive third-party round trips. But
sbol-rs drops subjects carrying only extension predicates and no SBOL type, so
Lab's own object types must carry `sbol:Identified` through
`IdentifiedExtension`. The labop branch's `sbol3::Document::read` plus
`validate()` harness is the right place to assert it.

**Reverse traversal in sbol-rs is a linear scan**, documented as such. Fine at
the scale of the golden-gate example, worth an index for a project with
thousands of parts.

**Quantity units are exact; ontology terms would like to be hierarchical.**
[0025](decisions/0025-quantity-types.md) pins a unit exactly and refuses
conversion, and grounding roles in terms invites the opposite instinct, that a
promoter should satisfy a bound written for a regulatory region. The bundled
snapshot cannot answer that question at all, so the conservative reading holds
for now: terms are validated, not used to classify. Revisit only alongside the
upstream ontology work, and decide deliberately rather than inheriting whatever
the snapshot happens to encode.

**If the type system ever reads the ontology, the ontology becomes a pinned
dependency.** A snapshot update would otherwise change what compiles. `lab.lock`
is the place, using the `TSV_FORMAT_VERSION` and per-source `raw_sha256` that
`sbol-ontology` already carries.

**Two-tier validation needs an upstream change first.** Gating the completeness
family per module depends on rules in `crates/sbol3/rules.toml` carrying gates,
and none of the 149 do. Sequence the upstream catalog change before the Lab-side
pass, or the pass runs every rule at every boundary.

**Dependency weight at the wasm boundary.** `lab-ide-wasm` builds on
`lab-language`. Ontology facts are an embedded TSV and are fine there; `oxrdf`
and the RDF I/O stack are not obviously fine. This is why the validation pass
runs from `lab-project` rather than from `compile_parsed_module`, and it needs
measuring rather than assuming.

**Identity migration crosses versioned boundaries.** `PORTABLE_MODULE_SCHEMA_VERSION` moved to `lab.portable-module.v4` when grounding landed, to `lab.portable-module.v5` when SBOL Component and supplier identities became separate fields, to `lab.portable-module.v6` when action capability names became absolute SBOLInventory capability-kind IRIs, to `lab.portable-module.v7` when durable workflow calls began preserving exact resolved callee identities for package-wide reachability, and to `lab.portable-module.v8` when action parameters began preserving absolute SBOLInventory property-kind IRIs. Facility planning now retains exact Component-to-MaterialLot candidates alongside the inventory digest and freezes the selected binding in allocated LAIR.

The checker's tables are the bulk of the mechanical work: fifteen
`HashMap<String, _>` and `BTreeSet<String>` fields on `SemanticContext`, plus
one flat module-wide name table in `declarations.rs`. There are no scopes to
rework, because there is no scope stack; local environments are cloned
`HashMap<String, Ty>` values passed by argument.
