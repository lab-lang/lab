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

from __future__ import annotations

from typing import Any, Final, Generic, Protocol, TypeVar

from lab._effects import Effect
from lab._expressions import Decimal, Quantity
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import WorkflowCall

LAB_MODULE: Final[str]

class CircularTopology(LabRole): ...
"""A sequence with no free ends."""

class CodingSequence(LabRole): ...
"""A region translated into a protein."""

class EngineeredRegion(LabRole): ...
"""A region deliberately assembled rather than found."""

class FunctionalEntity(LabRole): ...
"""An entity described by what it does rather than what it is made of.

This is the term SBOL falls back to when nothing more specific is known, so
a kind that plays it is saying only that it participates in a design."""

class IupacNucleicAcid(LabRole): ...
"""Nucleotides written in the IUPAC alphabet."""

class IupacProtein(LabRole): ...
"""Amino acids written in the IUPAC alphabet."""

class LinearTopology(LabRole): ...
"""A sequence with two free ends."""

class Macromolecule(LabRole): ...
"""A protein, which is what a coding sequence expresses."""

class NucleicAcid(LabRole): ...
"""A nucleic acid: DNA or RNA."""

class Operator(LabRole): ...
"""A region a repressor or activator binds."""

class PromoterRegion(LabRole): ...
"""A region transcription begins at.

Named for the region rather than the part because roles and types share one
namespace, and `Promoter` is already the kind a supplier lists."""

class RibosomeEntrySite(LabRole): ...
"""Where a ribosome binds ahead of a coding sequence."""

class SimpleChemical(LabRole): ...
"""A small molecule: an inducer, an antibiotic, a buffer component."""

class Terminator(LabRole): ...
"""Where transcription stops."""
