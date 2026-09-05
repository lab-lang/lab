"""Named biological parts and the roles they play.

A catalogued name says only that a supplier lists the item. It is not a claim
that a suitable lot is on the shelf; that remains an inventory resolution and
a runtime evidence question."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "std.bio.parts"
"""The exact Lab module these bindings import."""

class Arabinose(LabType):
    __lab_name__ = "Arabinose"
    __lab_roles__ = ("Signal",)
    __lab_definition__ = ("std.bio.parts", "Arabinose")
    __lab_uses__ = ("std.bio.designs", "std.bio.parts")
"""The inducer an arabinose-responsive promoter answers to."""

B0015 = Symbol(name="B0015", uses=("std.bio.designs", "std.bio.parts"), definition=("std.bio.parts", "B0015"))

B0034 = Symbol(name="B0034", uses=("std.bio.designs", "std.bio.parts"), definition=("std.bio.parts", "B0034"))

BsaI = Symbol(name="BsaI", uses=("std.bio.designs", "std.bio.parts"), definition=("std.bio.parts", "BsaI"))

class GreenFluorescentProtein(LabType):
    __lab_name__ = "GreenFluorescentProtein"
    __lab_roles__ = ("Protein",)
    __lab_definition__ = ("std.bio.parts", "GreenFluorescentProtein")
    __lab_uses__ = ("std.bio.designs", "std.bio.parts")
"""A reporter protein read as green fluorescence."""

class Tetracycline(LabType):
    __lab_name__ = "Tetracycline"
    __lab_roles__ = ("Signal",)
    __lab_definition__ = ("std.bio.parts", "Tetracycline")
    __lab_uses__ = ("std.bio.designs", "std.bio.parts")
"""The inducer a tetracycline-responsive promoter answers to."""

pBAD = Symbol(name="pBAD", uses=("std.bio.designs", "std.bio.parts"), definition=("std.bio.parts", "pBAD"))

pTet = Symbol(name="pTet", uses=("std.bio.designs", "std.bio.parts"), definition=("std.bio.parts", "pTet"))

sfGFP = Symbol(name="sfGFP", uses=("std.bio.designs", "std.bio.parts"), definition=("std.bio.parts", "sfGFP"))
