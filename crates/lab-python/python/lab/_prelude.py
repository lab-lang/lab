"""Foundational types and operations available to every Lab module."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

_Accepted_Value_1 = TypeVar("_Accepted_Value_1")
_CDS_Product_1 = TypeVar("_CDS_Product_1")
_Circuit_Trigger_1 = TypeVar("_Circuit_Trigger_1")
_Circuit_Product_2 = TypeVar("_Circuit_Product_2")
_List_Item_1 = TypeVar("_List_Item_1")
_Material_Subject_1 = TypeVar("_Material_Subject_1", covariant=True)
_Promoter_Trigger_1 = TypeVar("_Promoter_Trigger_1")
_Rejected_Value_1 = TypeVar("_Rejected_Value_1")

LAB_MODULE = "std.prelude"
"""The exact Lab module these bindings import."""

__all__ = [
    "Accepted",
    "Antibiotic",
    "Backbone",
    "Buffer",
    "CDS",
    "Chassis",
    "Circuit",
    "CloneSet",
    "Colonies",
    "ColonyMap",
    "DNA",
    "Duration",
    "Event",
    "Evidence",
    "Evidential",
    "Fragment",
    "Image",
    "List",
    "Material",
    "Medium",
    "None_",
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
    "Solution",
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

class Accepted(LabConstructor, Generic[_Accepted_Value_1]):
    __lab_fields__ = (("evidence", "evidence", False), ("material", "material", False))
    __lab_name__ = "Accepted"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Accepted")
    __lab_uses__ = ()

class Antibiotic(LabType):
    __lab_name__ = "Antibiotic"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Antibiotic")
    __lab_uses__ = ()

class Backbone(LabType):
    __lab_name__ = "Backbone"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Backbone")
    __lab_uses__ = ()

class Buffer(LabType):
    __lab_name__ = "Buffer"
    __lab_roles__ = ("Solution",)
    __lab_definition__ = ("std.prelude", "Buffer")
    __lab_uses__ = ()
"""A salt solution cells are washed and resuspended in."""

class CDS(LabType, Generic[_CDS_Product_1]):
    __lab_name__ = "CDS"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "CDS")
    __lab_uses__ = ()

class Chassis(LabType):
    __lab_name__ = "Chassis"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Chassis")
    __lab_uses__ = ()
"""A host organism that carries engineered DNA."""

class Circuit(LabType, Generic[_Circuit_Trigger_1, _Circuit_Product_2]):
    __lab_name__ = "Circuit"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Circuit")
    __lab_uses__ = ()

class CloneSet(LabConstructor):
    __lab_fields__ = (("highest_confidence", "highest_confidence", False),)
    __lab_name__ = "CloneSet"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "CloneSet")
    __lab_uses__ = ()

class Colonies(LabConstructor):
    __lab_fields__ = (("count", "count", False),)
    __lab_name__ = "Colonies"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Colonies")
    __lab_uses__ = ()

class ColonyMap(LabConstructor):
    __lab_fields__ = (("isolated", "isolated", False),)
    __lab_name__ = "ColonyMap"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "ColonyMap")
    __lab_uses__ = ()

class DNA(LabType):
    __lab_name__ = "DNA"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "DNA")
    __lab_uses__ = ()

class Duration(LabType):
    __lab_name__ = "Duration"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Duration")
    __lab_uses__ = ()

class Evidence(LabType):
    __lab_name__ = "Evidence"
    __lab_roles__ = ("Evidential",)
    __lab_definition__ = ("std.prelude", "Evidence")
    __lab_uses__ = ()

class Evidential(LabRole):
    __lab_role__ = "Evidential"
    __lab_name__ = "Evidential"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Evidential")
    __lab_uses__ = ()
"""Information that may be offered in support of a claim."""

class Event(LabRole):
    __lab_role__ = "Event"
    __lab_name__ = "Event"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Event")
    __lab_uses__ = ()
"""An occurrence the durable workflow journal records."""

class Fragment(LabType):
    __lab_name__ = "Fragment"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Fragment")
    __lab_uses__ = ()

class Image(LabType):
    __lab_name__ = "Image"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Image")
    __lab_uses__ = ()

class List(LabType, Generic[_List_Item_1]):
    __lab_name__ = "List"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "List")
    __lab_uses__ = ()

class Material(LabType, Generic[_Material_Subject_1]):
    __lab_name__ = "Material"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Material")
    __lab_uses__ = ()

class Medium(LabType):
    __lab_name__ = "Medium"
    __lab_roles__ = ("Solution",)
    __lab_definition__ = ("std.prelude", "Medium")
    __lab_uses__ = ()
"""What an organism is grown in or on."""

class Part(LabType):
    __lab_name__ = "Part"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Part")
    __lab_uses__ = ()

class Plasmid(LabConstructor):
    __lab_fields__ = (("concentration", "concentration", False), ("design", "design", False), ("length", "length", False), ("sequence", "sequence", False), ("topology", "topology", False), ("volume", "volume", False))
    __lab_name__ = "Plasmid"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Plasmid")
    __lab_uses__ = ()
"""A backend-neutral plasmid design."""

class Promoter(LabType, Generic[_Promoter_Trigger_1]):
    __lab_name__ = "Promoter"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Promoter")
    __lab_uses__ = ()

class Protein(LabRole):
    __lab_role__ = "Protein"
    __lab_name__ = "Protein"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Protein")
    __lab_uses__ = ()
"""A gene product a coding sequence expresses."""

class Reason(LabType):
    __lab_name__ = "Reason"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Reason")
    __lab_uses__ = ()

class Regulation(LabType):
    __lab_name__ = "Regulation"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Regulation")
    __lab_uses__ = ()
"""Which way a promoter answers the signal it responds to."""

class Rejected(LabConstructor, Generic[_Rejected_Value_1]):
    __lab_fields__ = (("evidence", "evidence", False), ("material", "material", False), ("reason", "reason", False))
    __lab_name__ = "Rejected"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Rejected")
    __lab_uses__ = ()

class RestrictionEnzyme(LabType):
    __lab_name__ = "RestrictionEnzyme"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "RestrictionEnzyme")
    __lab_uses__ = ()

class Screening(LabConstructor):
    __lab_fields__ = (("clones", "clones", False),)
    __lab_name__ = "Screening"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Screening")
    __lab_uses__ = ()

class Signal(LabRole):
    __lab_role__ = "Signal"
    __lab_name__ = "Signal"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Signal")
    __lab_uses__ = ()
"""A molecule or condition a circuit responds to."""

class Solution(LabRole):
    __lab_role__ = "Solution"
    __lab_name__ = "Solution"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Solution")
    __lab_uses__ = ()
"""A poured solution: a buffer or a medium a verb pours the same way."""

class Strain(LabConstructor):
    __lab_fields__ = (("chassis", "chassis", False), ("plasmids", "plasmids", False), ("selection", "selection", False))
    __lab_name__ = "Strain"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Strain")
    __lab_uses__ = ()
"""A chassis carrying a defined set of plasmid designs."""

class Topology(LabType):
    __lab_name__ = "Topology"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "Topology")
    __lab_uses__ = ()

class WorkflowContext(LabConstructor):
    __lab_fields__ = (("elapsed", "elapsed", False),)
    __lab_name__ = "WorkflowContext"
    __lab_roles__ = ()
    __lab_definition__ = ("std.prelude", "WorkflowContext")
    __lab_uses__ = ()

circular = Symbol(name="circular", uses=(), definition=("std.prelude", "circular"))

induced = Symbol(name="induced", uses=(), definition=("std.prelude", "induced"))

repressed = Symbol(name="repressed", uses=(), definition=("std.prelude", "repressed"))

None_ = Symbol(name="None", uses=(), definition=("std.prelude", "None"))

no_colonies = Symbol(name="no_colonies", uses=(), definition=("std.prelude", "no_colonies"))

sequence_mismatch = Symbol(name="sequence_mismatch", uses=(), definition=("std.prelude", "sequence_mismatch"))

inconclusive_sequence = Symbol(name="inconclusive_sequence", uses=(), definition=("std.prelude", "inconclusive_sequence"))

acceptance_failed = Symbol(name="acceptance_failed", uses=(), definition=("std.prelude", "acceptance_failed"))

dna = Function(
    name="dna",
    definition=("std.prelude", "dna"),
    inputs=("argument_1",),
    python_inputs=("argument_1",),
    uses=(),
)
"""Construct a DNA value from a nucleotide sequence."""

detect_colonies = Function(
    name="detect_colonies",
    definition=("std.prelude", "detect_colonies"),
    inputs=("argument_1",),
    python_inputs=("argument_1",),
    uses=(),
)

sites = Function(
    name="sites",
    definition=("std.prelude", "sites"),
    inputs=("argument_1",),
    python_inputs=("argument_1",),
    uses=(),
)

accepts = Function(
    name="accepts",
    definition=("std.prelude", "accepts"),
    inputs=("argument_1", "argument_2"),
    python_inputs=("argument_1", "argument_2"),
    uses=(),
)
"""Whether a design's acceptance criteria are met by this evidence."""
