"""Reporters and the readouts they produce.

A reporter is what a circuit expresses so that its behaviour can be measured.
The readout is what an instrument records, and it is what makes two circuits
comparable: a panel may vary which signal triggers it, but pinning the
readout is what lets the numbers sit next to each other.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._vocabulary import Symbol

LAB_MODULE = "std.bio.reporters"
"""The Lab module these names come from."""

Absorbance = Symbol(name="Absorbance", uses=("std.bio.reporters",))
"""Light absorbed rather than emitted, read as optical density."""

Fluorescence = Symbol(name="Fluorescence", uses=("std.bio.reporters",))
"""Light emitted after excitation, read by a plate reader or a microscope."""

Luminescence = Symbol(name="Luminescence", uses=("std.bio.reporters",))
"""Light emitted by an enzymatic reaction, requiring no excitation source."""

Reporter = Symbol(name="Reporter", uses=("std.bio.reporters",))
