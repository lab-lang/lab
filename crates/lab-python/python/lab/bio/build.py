"""Generated bindings for a Lab module."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

_Realize_T_1 = TypeVar("_Realize_T_1")

LAB_MODULE = "std.bio.build"
"""The exact Lab module these bindings import."""

realize = Action(
    name="realize",
    definition=("std.bio.build", "realize"),
    operation="std.bio.build.realize",
    phrase=("realize", "<design>", "from", "<dependencies>"),
    python_slots=("design", "dependencies"),
    results=("product",),
    optional=(("from", "<dependencies>"),),
    uses=("std.bio.build",),
)
