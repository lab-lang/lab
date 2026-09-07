"""Generated bindings for a Lab module."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "contribution_example.main"
"""The exact Lab module these bindings import."""

broth = Symbol(name="broth", uses=("contribution_example.science", "std.bio.designs", "std.lab.plasmid", "contribution_example.main"), definition=("contribution_example.main", "broth"))

main = ImportedWorkflow(
    name="main",
    definition=("contribution_example.main", "main"),
    inputs=(),
    python_inputs=(),
    results=("outcome",),
    uses=("contribution_example.science", "std.bio.designs", "std.lab.plasmid", "contribution_example.main"),
)
