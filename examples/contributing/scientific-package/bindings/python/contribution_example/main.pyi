"""Generated bindings for a Lab module."""

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

import lab._prelude as _module_0
import lab.bio.designs as _module_1

LAB_MODULE: Final[str]

broth: Final[Symbol]

class _MainWorkflow(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self) -> WorkflowCall[_module_0.Material[_module_1.Medium]]: ...

main: Final[_MainWorkflow]
