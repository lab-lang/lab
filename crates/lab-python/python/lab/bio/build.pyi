"""Generated bindings for a Lab module."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from __future__ import annotations

from typing import Any, Final, Generic, Protocol, TypeVar

from lab._effects import Effect
from lab._expressions import Decimal, Quantity
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import WorkflowCall

import lab._prelude as _module_0

_Realize_T_1 = TypeVar("_Realize_T_1")

LAB_MODULE: Final[str]

class _RealizeAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, design: _Realize_T_1, dependencies: list[_module_0.Material[_module_0.Plasmid]] = ...) -> Effect[_module_0.Material[_Realize_T_1]]: ...

realize: Final[_RealizeAction]
