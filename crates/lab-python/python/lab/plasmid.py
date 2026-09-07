"""Generated bindings for a Lab module."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

_Provision_T_1 = TypeVar("_Provision_T_1")
_Dispose_T_1 = TypeVar("_Dispose_T_1")

LAB_MODULE = "std.lab.plasmid"
"""The exact Lab module these bindings import."""

class SequenceCheck(LabConstructor):
    __lab_fields__ = (("evidence", "evidence", False), ("material", "material", False))
    __lab_name__ = "SequenceCheck"
    __lab_roles__ = ()
    __lab_definition__ = ("std.lab.plasmid", "SequenceCheck")
    __lab_uses__ = ("std.lab.plasmid",)
"""A sequenced plasmid material together with the evidence used to judge it."""

class Exact(LabConstructor):
    __lab_fields__ = (("evidence", "evidence", False), ("material", "material", False))
    __lab_name__ = "Exact"
    __lab_roles__ = ()
    __lab_definition__ = ("std.lab.plasmid", "Exact")
    __lab_uses__ = ("std.lab.plasmid",)
"""A sequence that exactly matches the intended plasmid."""

class Mismatch(LabConstructor):
    __lab_fields__ = (("evidence", "evidence", False), ("material", "material", False))
    __lab_name__ = "Mismatch"
    __lab_roles__ = ()
    __lab_definition__ = ("std.lab.plasmid", "Mismatch")
    __lab_uses__ = ("std.lab.plasmid",)
"""A sequence that does not match the intended plasmid."""

class Inconclusive(LabConstructor):
    __lab_fields__ = (("evidence", "evidence", False), ("material", "material", False))
    __lab_name__ = "Inconclusive"
    __lab_roles__ = ()
    __lab_definition__ = ("std.lab.plasmid", "Inconclusive")
    __lab_uses__ = ("std.lab.plasmid",)
"""Evidence that is insufficient to judge the plasmid sequence."""

capture = Action(
    name="capture",
    definition=("std.lab.plasmid", "capture"),
    operation="std.lab.plasmid.capture",
    phrase=("capture", "image", "of", "<plate>"),
    python_slots=("plate",),
    results=("image",),
    optional=(),
    uses=("std.lab.plasmid",),
)

synthesize = Action(
    name="synthesize",
    definition=("std.lab.plasmid", "synthesize"),
    operation="std.lab.plasmid.synthesize",
    phrase=("synthesize", "<design>"),
    python_slots=("design",),
    results=("fragments",),
    optional=(),
    uses=("std.lab.plasmid",),
)

assemble = Action(
    name="assemble",
    definition=("std.lab.plasmid", "assemble"),
    operation="std.lab.plasmid.assemble",
    phrase=("assemble", "<fragments>"),
    python_slots=("fragments",),
    results=("construct",),
    optional=(),
    uses=("std.lab.plasmid",),
)

provision = Action(
    name="provision",
    definition=("std.lab.plasmid", "provision"),
    operation="std.lab.plasmid.provision",
    phrase=("provision", "<item>"),
    python_slots=("item",),
    results=("material",),
    optional=(),
    uses=("std.lab.plasmid",),
)

transform = Action(
    name="transform",
    definition=("std.lab.plasmid", "transform"),
    operation="std.lab.plasmid.transform",
    phrase=("transform", "<design>", "from", "<plasmids>", "into", "<cells>"),
    python_slots=("design", "plasmids", "cells"),
    results=("strain", "culture"),
    optional=(),
    uses=("std.lab.plasmid",),
)

recover = Action(
    name="recover",
    definition=("std.lab.plasmid", "recover"),
    operation="std.lab.plasmid.recover",
    phrase=("recover", "<culture>", "for", "<duration>"),
    python_slots=("culture", "duration"),
    results=("culture",),
    optional=(),
    uses=("std.lab.plasmid",),
)

dilute = Action(
    name="dilute",
    definition=("std.lab.plasmid", "dilute"),
    operation="std.lab.plasmid.dilute",
    phrase=("dilute", "<culture>"),
    python_slots=("culture",),
    results=("culture",),
    optional=(),
    uses=("std.lab.plasmid",),
)

plate = Action(
    name="plate",
    definition=("std.lab.plasmid", "plate"),
    operation="std.lab.plasmid.plate",
    phrase=("plate", "<culture>", "on", "<medium>"),
    python_slots=("culture", "medium"),
    results=("plate",),
    optional=(),
    uses=("std.lab.plasmid",),
)

pick = Action(
    name="pick",
    definition=("std.lab.plasmid", "pick"),
    operation="std.lab.plasmid.pick",
    phrase=("pick", "<count>", "isolated", "colonies", "from", "<plate>"),
    python_slots=("count", "plate"),
    results=("candidates",),
    optional=(),
    uses=("std.lab.plasmid",),
)

screen = Action(
    name="screen",
    definition=("std.lab.plasmid", "screen"),
    operation="std.lab.plasmid.screen",
    phrase=("screen", "<candidates>", "against", "<design>"),
    python_slots=("candidates", "design"),
    results=("screening",),
    optional=(),
    uses=("std.lab.plasmid",),
)

culture = Action(
    name="culture",
    definition=("std.lab.plasmid", "culture"),
    operation="std.lab.plasmid.culture",
    phrase=("culture", "<clone>", "at", "<temperature>", "for", "<duration>"),
    python_slots=("clone", "temperature", "duration"),
    results=("culture",),
    optional=(),
    uses=("std.lab.plasmid",),
)

purify = Action(
    name="purify",
    definition=("std.lab.plasmid", "purify"),
    operation="std.lab.plasmid.purify",
    phrase=("purify", "<culture>"),
    python_slots=("culture",),
    results=("plasmid",),
    optional=(),
    uses=("std.lab.plasmid",),
)

split = Action(
    name="split",
    definition=("std.lab.plasmid", "split"),
    operation="std.lab.plasmid.split",
    phrase=("split", "<material>"),
    python_slots=("material",),
    results=("retained", "aliquot"),
    optional=(),
    uses=("std.lab.plasmid",),
)

sequence = Action(
    name="sequence",
    definition=("std.lab.plasmid", "sequence"),
    operation="std.lab.plasmid.sequence",
    phrase=("sequence", "<aliquot>"),
    python_slots=("aliquot",),
    results=("result",),
    optional=(),
    uses=("std.lab.plasmid",),
)

quantify = Action(
    name="quantify",
    definition=("std.lab.plasmid", "quantify"),
    operation="std.lab.plasmid.quantify",
    phrase=("quantify", "<material>"),
    python_slots=("material",),
    results=("evidence",),
    optional=(),
    uses=("std.lab.plasmid",),
)

store = Action(
    name="store",
    definition=("std.lab.plasmid", "store"),
    operation="std.lab.plasmid.store",
    phrase=("store", "<material>", "at", "<temperature>"),
    python_slots=("material", "temperature"),
    results=("material",),
    optional=(),
    uses=("std.lab.plasmid",),
)

dispose = Action(
    name="dispose",
    definition=("std.lab.plasmid", "dispose"),
    operation="std.lab.plasmid.dispose",
    phrase=("dispose", "<material>"),
    python_slots=("material",),
    results=(),
    optional=(),
    uses=("std.lab.plasmid",),
)
