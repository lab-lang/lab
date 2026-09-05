"""Foundational types and operations available to every Lab module."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from __future__ import annotations

from typing import Any, Final, Generic, Protocol, TypeVar

from lab._effects import Effect
from lab._expressions import Decimal, Quantity
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import WorkflowCall

import lab.bio.designs as _module_0

_Accepted_Value_1 = TypeVar("_Accepted_Value_1")
_CDS_Product_1 = TypeVar("_CDS_Product_1")
_Circuit_Trigger_1 = TypeVar("_Circuit_Trigger_1")
_Circuit_Product_2 = TypeVar("_Circuit_Product_2")
_List_Item_1 = TypeVar("_List_Item_1")
_Material_Subject_1 = TypeVar("_Material_Subject_1")
_Promoter_Trigger_1 = TypeVar("_Promoter_Trigger_1")
_Rejected_Value_1 = TypeVar("_Rejected_Value_1")

LAB_MODULE: Final[str]

class Accepted(LabConstructor, Generic[_Accepted_Value_1]):
    def __new__(cls, *, evidence: list[Evidence], material: Material[Plasmid]) -> Accepted[Plasmid]: ...

class Antibiotic(LabType): ...

class Backbone(LabType): ...

class Buffer(Solution): ...
"""A salt solution cells are washed and resuspended in."""

class CDS(LabType, Generic[_CDS_Product_1]): ...

class Chassis(LabType): ...
"""A host organism that carries engineered DNA."""

class Circuit(LabType, Generic[_Circuit_Trigger_1, _Circuit_Product_2]): ...

class CloneSet(LabConstructor):
    def __new__(cls, *, highest_confidence: Material[_module_0.isolated[Strain]]) -> CloneSet: ...

class Colonies(LabConstructor):
    def __new__(cls, *, count: int) -> Colonies: ...

class ColonyMap(LabConstructor):
    def __new__(cls, *, isolated: Colonies) -> ColonyMap: ...

class DNA(LabType): ...

class Duration(LabType): ...

class Evidence(Evidential): ...

class Evidential(LabRole): ...
"""Information that may be offered in support of a claim."""

class Event(LabRole): ...
"""An occurrence the durable workflow journal records."""

class Fragment(LabType): ...

class Image(LabType): ...

class List(LabType, Generic[_List_Item_1]): ...

class Material(LabType, Generic[_Material_Subject_1]): ...

class Medium(Solution): ...
"""What an organism is grown in or on."""

class Part(LabType): ...

class Plasmid(LabConstructor):
    def __new__(cls, *, concentration: Quantity, design: Plasmid, length: Quantity, sequence: DNA, topology: Topology, volume: Quantity) -> Plasmid: ...
"""A backend-neutral plasmid design."""

class Promoter(LabType, Generic[_Promoter_Trigger_1]): ...

class Protein(LabRole): ...
"""A gene product a coding sequence expresses."""

class Reason(LabType): ...

class Regulation(LabType): ...
"""Which way a promoter answers the signal it responds to."""

class Rejected(LabConstructor, Generic[_Rejected_Value_1]):
    def __new__(cls, *, evidence: list[Evidence], material: Material[Plasmid] | None, reason: Reason) -> Rejected[Plasmid]: ...

class RestrictionEnzyme(LabType): ...

class Screening(LabConstructor):
    def __new__(cls, *, clones: CloneSet) -> Screening: ...

class Signal(LabRole): ...
"""A molecule or condition a circuit responds to."""

class Solution(LabRole): ...
"""A poured solution: a buffer or a medium a verb pours the same way."""

class Strain(LabConstructor):
    def __new__(cls, *, chassis: Chassis, plasmids: list[Plasmid], selection: Antibiotic) -> Strain: ...
"""A chassis carrying a defined set of plasmid designs."""

class Topology(LabType): ...

class WorkflowContext(LabConstructor):
    def __new__(cls, *, elapsed: Duration) -> WorkflowContext: ...

circular: Final[Symbol]

induced: Final[Symbol]

repressed: Final[Symbol]

None_: Final[Symbol]

no_colonies: Final[Symbol]

sequence_mismatch: Final[Symbol]

inconclusive_sequence: Final[Symbol]

acceptance_failed: Final[Symbol]

class _DnaFunction(Protocol):
    def __call__(self, argument_1: str) -> DNA: ...

dna: Final[_DnaFunction]
"""Construct a DNA value from a nucleotide sequence."""

class _DetectColoniesFunction(Protocol):
    def __call__(self, argument_1: Image) -> ColonyMap: ...

detect_colonies: Final[_DetectColoniesFunction]

class _SitesFunction(Protocol):
    def __call__(self, argument_1: RestrictionEnzyme) -> int: ...

sites: Final[_SitesFunction]

class _AcceptsFunction(Protocol):
    def __call__(self, argument_1: Plasmid, argument_2: list[Evidence]) -> bool: ...

accepts: Final[_AcceptsFunction]
"""Whether a design's acceptance criteria are met by this evidence."""
