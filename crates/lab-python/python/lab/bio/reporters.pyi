"""Reporters and the readouts they produce.

A reporter is what a circuit expresses so that its behaviour can be measured.
The readout is what an instrument records, and it is what makes two circuits
comparable: a panel may vary which signal triggers it, but pinning the
readout is what lets the numbers sit next to each other."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from __future__ import annotations

from typing import Any, Final, Generic, Protocol, TypeVar, overload

from lab._effects import Effect
from lab._expressions import Decimal, Quantity
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import WorkflowCall

LAB_MODULE: Final[str]

class Absorbance(Reporter): ...
"""Light absorbed rather than emitted, read as optical density."""

class Fluorescence(Reporter): ...
"""Light emitted after excitation, read by a plate reader or a microscope."""

class Luminescence(Reporter): ...
"""Light emitted by an enzymatic reaction, requiring no excitation source."""

class Reporter(LabRole): ...
