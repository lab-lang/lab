"""Foundational types and operations available to every Lab module."""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from typing import Generic, TypeVar

from ._types import LabConstructor, LabRole, LabType
from ._vocabulary import Function, Symbol

_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")

LAB_MODULE = "std.prelude"
"""The Lab module these names come from."""

__all__ = [
    "CDS",
    "DNA",
    "Accepted",
    "Antibiotic",
    "Backbone",
    "Chassis",
    "Circuit",
    "CloneSet",
    "Colonies",
    "ColonyMap",
    "Duration",
    "Event",
    "Evidence",
    "Evidential",
    "Fragment",
    "Image",
    "List",
    "Material",
    "Medium",
    "Part",
    "Plasmid",
    "Promoter",
    "Protein",
    "Reason",
    "Regulation",
    "Rejected",
    "RestrictionEnzyme",
    "Screening",
    "Signal",
    "Strain",
    "Topology",
    "WorkflowContext",
    "acceptance_failed",
    "accepts",
    "circular",
    "detect_colonies",
    "dna",
    "inconclusive_sequence",
    "induced",
    "no_colonies",
    "repressed",
    "sequence_mismatch",
    "sites",
]


class Accepted(LabConstructor, Generic[_T1]):
    __lab_uses__ = ()


class Antibiotic(LabType):
    __lab_uses__ = ()


class Backbone(LabType):
    __lab_uses__ = ()


class CDS(LabType, Generic[_T1]):
    __lab_uses__ = ()


class Chassis(LabType):
    """A host organism that carries engineered DNA."""

    __lab_uses__ = ()


class Circuit(LabType, Generic[_T1, _T2]):
    __lab_uses__ = ()


class CloneSet(LabConstructor):
    __lab_uses__ = ()


class Colonies(LabConstructor):
    __lab_uses__ = ()


class ColonyMap(LabConstructor):
    __lab_uses__ = ()


class DNA(LabType):
    __lab_uses__ = ()


class Duration(LabType):
    __lab_uses__ = ()


class Evidence(LabType):
    __lab_uses__ = ()


class Evidential(LabRole):
    """Information that may be offered in support of a claim."""

    __lab_role__ = "Evidential"
    __lab_uses__ = ()


class Event(LabRole):
    """An occurrence the durable workflow journal records."""

    __lab_role__ = "Event"
    __lab_uses__ = ()


class Fragment(LabType):
    __lab_uses__ = ()


class Image(LabType):
    __lab_uses__ = ()


class List(LabType, Generic[_T1]):
    __lab_uses__ = ()


class Material(LabType, Generic[_T1]):
    __lab_uses__ = ()


class Medium(LabType):
    """What an organism is grown in or on."""

    __lab_uses__ = ()


class Part(LabType):
    __lab_uses__ = ()


class Plasmid(LabConstructor):
    """A backend-neutral plasmid design."""

    __lab_uses__ = ()


class Promoter(LabType, Generic[_T1]):
    __lab_uses__ = ()


class Protein(LabRole):
    """A gene product a coding sequence expresses."""

    __lab_role__ = "Protein"
    __lab_uses__ = ()


class Reason(LabType):
    __lab_uses__ = ()


class Regulation(LabType):
    """Which way a promoter answers the signal it responds to."""

    __lab_uses__ = ()


class Rejected(LabConstructor, Generic[_T1]):
    __lab_uses__ = ()


class RestrictionEnzyme(LabType):
    __lab_uses__ = ()


class Screening(LabConstructor):
    __lab_uses__ = ()


class Signal(LabRole):
    """A molecule or condition a circuit responds to."""

    __lab_role__ = "Signal"
    __lab_uses__ = ()


class Strain(LabConstructor):
    """A chassis carrying a defined set of plasmid designs."""

    __lab_uses__ = ()


class Topology(LabType):
    __lab_uses__ = ()


class WorkflowContext(LabConstructor):
    __lab_uses__ = ()


circular = Symbol(name="circular", uses=())
"""A value of type Topology."""

induced = Symbol(name="induced", uses=())
"""A value of type Regulation."""

repressed = Symbol(name="repressed", uses=())
"""A value of type Regulation."""

no_colonies = Symbol(name="no_colonies", uses=())
"""A value of type Reason."""

sequence_mismatch = Symbol(name="sequence_mismatch", uses=())
"""A value of type Reason."""

inconclusive_sequence = Symbol(name="inconclusive_sequence", uses=())
"""A value of type Reason."""

acceptance_failed = Symbol(name="acceptance_failed", uses=())
"""A value of type Reason."""

dna = Function(name="dna", uses=())
"""Construct a DNA value from a nucleotide sequence.

Called as (String) -> DNA.
"""

detect_colonies = Function(name="detect_colonies", uses=())
"""Called as (Image) -> ColonyMap."""

sites = Function(name="sites", uses=())
"""Called as (RestrictionEnzyme) -> Integer."""

accepts = Function(name="accepts", uses=())
"""Whether a design's acceptance criteria are met by this evidence.

Called as (Plasmid, List<Evidence>) -> Bool.
"""
