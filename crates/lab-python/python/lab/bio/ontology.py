"""The ontology terms a synthetic-biology design is described in.

A role here names a term rather than classifying a Lab type on its own. A
package grounds its kinds by playing these roles, so what a plasmid *is*
travels in a vocabulary every SBOL tool already reads, and the compiler never
has to guess whether a named item is DNA, a protein, or a reagent.

Terms come from three ontologies, and each answers a different question.
SBO says what kind of physical entity something is. SO says what part it
plays in a sequence. EDAM says how a sequence is written down.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._vocabulary import Symbol

LAB_MODULE = "std.bio.ontology"
"""The Lab module these names come from."""

CircularTopology = Symbol(name="CircularTopology", uses=("std.bio.ontology",))
"""A sequence with no free ends."""

CodingSequence = Symbol(name="CodingSequence", uses=("std.bio.ontology",))
"""A region translated into a protein."""

EngineeredRegion = Symbol(name="EngineeredRegion", uses=("std.bio.ontology",))
"""A region deliberately assembled rather than found."""

FunctionalEntity = Symbol(name="FunctionalEntity", uses=("std.bio.ontology",))
"""An entity described by what it does rather than what it is made of.

This is the term SBOL falls back to when nothing more specific is known, so
a kind that plays it is saying only that it participates in a design.
"""

IupacNucleicAcid = Symbol(name="IupacNucleicAcid", uses=("std.bio.ontology",))
"""Nucleotides written in the IUPAC alphabet."""

IupacProtein = Symbol(name="IupacProtein", uses=("std.bio.ontology",))
"""Amino acids written in the IUPAC alphabet."""

LinearTopology = Symbol(name="LinearTopology", uses=("std.bio.ontology",))
"""A sequence with two free ends."""

Macromolecule = Symbol(name="Macromolecule", uses=("std.bio.ontology",))
"""A protein, which is what a coding sequence expresses."""

NucleicAcid = Symbol(name="NucleicAcid", uses=("std.bio.ontology",))
"""A nucleic acid: DNA or RNA."""

Operator = Symbol(name="Operator", uses=("std.bio.ontology",))
"""A region a repressor or activator binds."""

PromoterRegion = Symbol(name="PromoterRegion", uses=("std.bio.ontology",))
"""A region transcription begins at.

Named for the region rather than the part because roles and types share one
namespace, and `Promoter` is already the kind a supplier lists.
"""

RibosomeEntrySite = Symbol(name="RibosomeEntrySite", uses=("std.bio.ontology",))
"""Where a ribosome binds ahead of a coding sequence."""

SimpleChemical = Symbol(name="SimpleChemical", uses=("std.bio.ontology",))
"""A small molecule: an inducer, an antibiotic, a buffer component."""

Terminator = Symbol(name="Terminator", uses=("std.bio.ontology",))
"""Where transcription stops."""
