"""Reporters and the readouts they produce.

A reporter is what a circuit expresses so that its behaviour can be measured.
The readout is what an instrument records, and it is what makes two circuits
comparable: a panel may vary which signal triggers it, but pinning the
readout is what lets the numbers sit next to each other."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "std.bio.reporters"
"""The exact Lab module these bindings import."""

class Absorbance(LabType):
    __lab_name__ = "Absorbance"
    __lab_roles__ = ("Reporter",)
    __lab_definition__ = ("std.bio.reporters", "Absorbance")
    __lab_uses__ = ("std.bio.reporters",)
"""Light absorbed rather than emitted, read as optical density."""

class Fluorescence(LabType):
    __lab_name__ = "Fluorescence"
    __lab_roles__ = ("Reporter",)
    __lab_definition__ = ("std.bio.reporters", "Fluorescence")
    __lab_uses__ = ("std.bio.reporters",)
"""Light emitted after excitation, read by a plate reader or a microscope."""

class Luminescence(LabType):
    __lab_name__ = "Luminescence"
    __lab_roles__ = ("Reporter",)
    __lab_definition__ = ("std.bio.reporters", "Luminescence")
    __lab_uses__ = ("std.bio.reporters",)
"""Light emitted by an enzymatic reaction, requiring no excitation source."""

class Reporter(LabRole):
    __lab_role__ = "Reporter"
    __lab_name__ = "Reporter"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.reporters", "Reporter")
    __lab_uses__ = ("std.bio.reporters",)
