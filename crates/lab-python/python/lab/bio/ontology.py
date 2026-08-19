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

from .._types import LabRole

LAB_MODULE = "std.bio.ontology"
"""The Lab module these names come from."""


class CircularTopology(LabRole):
    """A sequence with no free ends."""

    __lab_role__ = "CircularTopology"
    __lab_uses__ = ("std.bio.ontology",)


class CodingSequence(LabRole):
    """A region translated into a protein."""

    __lab_role__ = "CodingSequence"
    __lab_uses__ = ("std.bio.ontology",)


class EngineeredRegion(LabRole):
    """A region deliberately assembled rather than found."""

    __lab_role__ = "EngineeredRegion"
    __lab_uses__ = ("std.bio.ontology",)


class FunctionalEntity(LabRole):
    """An entity described by what it does rather than what it is made of.

    This is the term SBOL falls back to when nothing more specific is known, so
    a kind that plays it is saying only that it participates in a design.
    """

    __lab_role__ = "FunctionalEntity"
    __lab_uses__ = ("std.bio.ontology",)


class IupacNucleicAcid(LabRole):
    """Nucleotides written in the IUPAC alphabet."""

    __lab_role__ = "IupacNucleicAcid"
    __lab_uses__ = ("std.bio.ontology",)


class IupacProtein(LabRole):
    """Amino acids written in the IUPAC alphabet."""

    __lab_role__ = "IupacProtein"
    __lab_uses__ = ("std.bio.ontology",)


class LinearTopology(LabRole):
    """A sequence with two free ends."""

    __lab_role__ = "LinearTopology"
    __lab_uses__ = ("std.bio.ontology",)


class Macromolecule(LabRole):
    """A protein, which is what a coding sequence expresses."""

    __lab_role__ = "Macromolecule"
    __lab_uses__ = ("std.bio.ontology",)


class NucleicAcid(LabRole):
    """A nucleic acid: DNA or RNA."""

    __lab_role__ = "NucleicAcid"
    __lab_uses__ = ("std.bio.ontology",)


class Operator(LabRole):
    """A region a repressor or activator binds."""

    __lab_role__ = "Operator"
    __lab_uses__ = ("std.bio.ontology",)


class PromoterRegion(LabRole):
    """A region transcription begins at.

    Named for the region rather than the part because roles and types share one
    namespace, and `Promoter` is already the kind a supplier lists.
    """

    __lab_role__ = "PromoterRegion"
    __lab_uses__ = ("std.bio.ontology",)


class RibosomeEntrySite(LabRole):
    """Where a ribosome binds ahead of a coding sequence."""

    __lab_role__ = "RibosomeEntrySite"
    __lab_uses__ = ("std.bio.ontology",)


class SimpleChemical(LabRole):
    """A small molecule: an inducer, an antibiotic, a buffer component."""

    __lab_role__ = "SimpleChemical"
    __lab_uses__ = ("std.bio.ontology",)


class Terminator(LabRole):
    """Where transcription stops."""

    __lab_role__ = "Terminator"
    __lab_uses__ = ("std.bio.ontology",)
