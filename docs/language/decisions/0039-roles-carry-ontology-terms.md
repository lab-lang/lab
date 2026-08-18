# 0039 — A role may name the ontology term it stands for

## Status

Accepted.

## Context

The compiler knows more about a design than it can say. It knows `J23101` is a
promoter, that `chloramphenicol` is a selection agent rather than a nucleic
acid, and that a plasmid is an ordered composition of parts. None of that leaves
the toolchain in a vocabulary anyone else reads.

The cost is visible wherever Lab meets another tool. An exporter that must state
what a named item *is* has nothing to consult, so it states the same
general-purpose term for every item and the distinction the checker was holding
is lost on the way out.

The missing piece is small: a total function from a Lab type to the terms it
stands for. Everything else that makes Lab interoperable is downstream of it.

## Decision

A role may name the ontology term it stands for, and a kind may play roles.

```lab
role NucleicAcid = "SBO:0000251"
role EngineeredRegion = "SO:0000804"

artifact Plasmid is NucleicAcid, EngineeredRegion:
  sequence?: DNA
```

Grounding is ordinary role membership. A role already classifies types,
membership already travels with the type that declares it, and a package may
already classify its own types against a role it imported. Naming a term adds
an identity to a mechanism that had none; it does not add a mechanism.

A role's whole content is its identity, so the term is written after `=` rather
than as a property in a block. This is the form
[0021](0021-typed-external-identities.md) first proposed for catalogued items,
which is right here for the reason it was wrong there: a role has nothing else
to state.

The `is` clause is the one records already use, and it classifies the type a
kind produces, because that is the type a workflow names and a bound reads. No
package introduces a grammar production, so
[0022](0022-fixed-grammar-open-vocabulary.md) holds: a chemistry package grounds
its kinds in ChEBI without the compiler learning chemistry.

An ontology-grounded role is an ordinary open role and not a law. Nothing about
it is enforced by a bespoke rule, which is what keeps
[0020](0020-laws-are-declared-roles.md)'s closed set closed.

A compact identifier expands to its `identifiers.org` IRI when it is checked, so
`SO:0000167` and `https://identifiers.org/SO:0000167` are one term written two
ways rather than two terms that happen to agree.

## Consequences

`std.bio.ontology` names the SBO, SO, and EDAM terms a synthetic-biology design
is described in, and `std.bio.designs` grounds every kind it declares. A program
that imports the standard library describes its plasmids in a shared vocabulary
without stating anything about ontologies itself.

`Grounding` answers the question the rest of the work depends on: given a type,
which terms does it stand for. It is built over the modules in scope rather than
one module, because the role usually comes from a vocabulary package and the
membership from a design package, and neither knows the whole answer alone.

A grounded role is usable as a bound, and that turned out to matter more than
expected. `Plasmid.components` was `List<Part | Plasmid>`, which admitted
neither a promoter nor a coding sequence even though an assembly joins those as
readily as it joins a bare part. Enumerating the admissible kinds would need
editing every time a package adds one. The field is now `List<any NucleicAcid>`,
bounded by a role introduced so that designs could be described in a shared
vocabulary. A term added for the sake of what a design *is* turned out to be the
right statement of what a design may be *built from*.

Roles and types share one namespace, so a role cannot take a name a kind already
has. `role Promoter` collides with `artifact Promoter`. The vocabulary therefore
names the region rather than the part: `PromoterRegion`, `CodingSequence`,
`RibosomeEntrySite`. Splitting the namespace would cost more than the naming
does.

Whether a term *exists* is not checked here. The frontend checks that a term
could name one — an absolute IRI, or a compact identifier with a recognized
prefix — and reports what it expected where the term is written:

```
error: 'engineered region' is neither an IRI nor a compact identifier
  = help: write a term as "SO:0000167" or as the IRI it stands for
  = help: a role with no term classifies types without naming any ontology
```

Membership, branch, and conflict checks need an ontology snapshot and stay
outside `lab-language`. The obvious move, depending on `sbol-ontology` from the
frontend, is wrong: that crate depends unconditionally on a CLI argument parser
and an HTTP stack, and `lab-language` is what `lab-ide-wasm` builds on. Keeping
the split also means a single file can be checked for a malformed term without
resolving a package.

A term from a vocabulary this compiler has never heard of is written as a full
IRI and accepted. Recognizing a prefix is a convenience, not a gate.

The portable module schema is `lab.portable-module.v4`. A consumer that ignores
the new fields reads a design with nothing said about what it is, which is the
silence this decision exists to end, so they raise the version rather than
riding along as optional additions.
