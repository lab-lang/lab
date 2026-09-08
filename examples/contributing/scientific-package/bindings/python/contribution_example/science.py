"""A scientific operation implemented entirely with portable pipetting steps."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "contribution_example.science"
"""The exact Lab module these bindings import."""

homogenize = Action(
    name="homogenize",
    definition=("contribution_example.science", "homogenize"),
    operation="contribution_example.science.homogenize",
    phrase=("homogenize", "<sample>"),
    python_slots=("sample",),
    results=("prepared",),
    optional=(),
    uses=("std.bio.designs", "std.lab.plasmid", "contribution_example.science"),
)
"""Move the complete 30 uL sample to a fresh vessel and mix it three times."""
