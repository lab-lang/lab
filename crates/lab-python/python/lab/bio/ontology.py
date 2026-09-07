"""The ontology terms a synthetic-biology design is described in.

A role here names a term rather than classifying a Lab type on its own. A
package grounds its kinds by playing these roles, so what a plasmid *is*
travels in a vocabulary every SBOL tool already reads, and the compiler never
has to guess whether a named item is DNA, a protein, or a reagent.

Terms come from three ontologies, and each answers a different question.
SBO says what kind of physical entity something is. SO says what part it
plays in a sequence. EDAM says how a sequence is written down."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "std.bio.ontology"
"""The exact Lab module these bindings import."""

class CircularTopology(LabRole):
    __lab_role__ = "CircularTopology"
    __lab_name__ = "CircularTopology"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "CircularTopology")
    __lab_uses__ = ("std.bio.ontology",)
"""A sequence with no free ends."""

class CodingSequence(LabRole):
    __lab_role__ = "CodingSequence"
    __lab_name__ = "CodingSequence"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "CodingSequence")
    __lab_uses__ = ("std.bio.ontology",)
"""A region translated into a protein."""

class EngineeredRegion(LabRole):
    __lab_role__ = "EngineeredRegion"
    __lab_name__ = "EngineeredRegion"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "EngineeredRegion")
    __lab_uses__ = ("std.bio.ontology",)
"""A region deliberately assembled rather than found."""

class FunctionalEntity(LabRole):
    __lab_role__ = "FunctionalEntity"
    __lab_name__ = "FunctionalEntity"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "FunctionalEntity")
    __lab_uses__ = ("std.bio.ontology",)
"""An entity described by what it does rather than what it is made of.

This is the term SBOL falls back to when nothing more specific is known, so
a kind that plays it is saying only that it participates in a design."""

class IupacNucleicAcid(LabRole):
    __lab_role__ = "IupacNucleicAcid"
    __lab_name__ = "IupacNucleicAcid"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "IupacNucleicAcid")
    __lab_uses__ = ("std.bio.ontology",)
"""Nucleotides written in the IUPAC alphabet."""

class IupacProtein(LabRole):
    __lab_role__ = "IupacProtein"
    __lab_name__ = "IupacProtein"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "IupacProtein")
    __lab_uses__ = ("std.bio.ontology",)
"""Amino acids written in the IUPAC alphabet."""

class LinearTopology(LabRole):
    __lab_role__ = "LinearTopology"
    __lab_name__ = "LinearTopology"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "LinearTopology")
    __lab_uses__ = ("std.bio.ontology",)
"""A sequence with two free ends."""

class Macromolecule(LabRole):
    __lab_role__ = "Macromolecule"
    __lab_name__ = "Macromolecule"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "Macromolecule")
    __lab_uses__ = ("std.bio.ontology",)
"""A protein, which is what a coding sequence expresses."""

class NucleicAcid(LabRole):
    __lab_role__ = "NucleicAcid"
    __lab_name__ = "NucleicAcid"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "NucleicAcid")
    __lab_uses__ = ("std.bio.ontology",)
"""A nucleic acid: DNA or RNA."""

class Operator(LabRole):
    __lab_role__ = "Operator"
    __lab_name__ = "Operator"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "Operator")
    __lab_uses__ = ("std.bio.ontology",)
"""A region a repressor or activator binds."""

class PromoterRegion(LabRole):
    __lab_role__ = "PromoterRegion"
    __lab_name__ = "PromoterRegion"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "PromoterRegion")
    __lab_uses__ = ("std.bio.ontology",)
"""A region transcription begins at.

Named for the region rather than the part because roles and types share one
namespace, and `Promoter` is already the kind a supplier lists."""

class RibosomeEntrySite(LabRole):
    __lab_role__ = "RibosomeEntrySite"
    __lab_name__ = "RibosomeEntrySite"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "RibosomeEntrySite")
    __lab_uses__ = ("std.bio.ontology",)
"""Where a ribosome binds ahead of a coding sequence."""

class SimpleChemical(LabRole):
    __lab_role__ = "SimpleChemical"
    __lab_name__ = "SimpleChemical"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "SimpleChemical")
    __lab_uses__ = ("std.bio.ontology",)
"""A small molecule: an inducer, an antibiotic, a buffer component."""

class Terminator(LabRole):
    __lab_role__ = "Terminator"
    __lab_name__ = "Terminator"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.ontology", "Terminator")
    __lab_uses__ = ("std.bio.ontology",)
"""Where transcription stops."""
