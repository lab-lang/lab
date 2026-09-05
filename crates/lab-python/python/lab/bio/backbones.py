"""Assembly backbones a laboratory can order."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "std.bio.backbones"
"""The exact Lab module these bindings import."""

p15A_kan = Symbol(name="p15A_kan", uses=("std.bio.designs", "std.bio.backbones"), definition=("std.bio.backbones", "p15A_kan"))
