"""Reporters and the readouts they produce.

A reporter is what a circuit expresses so that its behaviour can be measured.
The readout is what an instrument records, and it is what makes two circuits
comparable: a panel may vary which signal triggers it, but pinning the
readout is what lets the numbers sit next to each other.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._types import LabRole, LabType

LAB_MODULE = "std.bio.reporters"
"""The Lab module these names come from."""


class Absorbance(LabType):
    """Light absorbed rather than emitted, read as optical density."""

    __lab_uses__ = ("std.bio.reporters",)


class Fluorescence(LabType):
    """Light emitted after excitation, read by a plate reader or a microscope."""

    __lab_uses__ = ("std.bio.reporters",)


class Luminescence(LabType):
    """Light emitted by an enzymatic reaction, requiring no excitation source."""

    __lab_uses__ = ("std.bio.reporters",)


class Reporter(LabRole):
    __lab_role__ = "Reporter"
    __lab_uses__ = ("std.bio.reporters",)
