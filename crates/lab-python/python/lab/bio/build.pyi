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

_Realize_T_1 = TypeVar("_Realize_T_1")

LAB_MODULE: Final[str]

class _RealizeAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    @overload
    def __call__(self, design: DesignReference[_Realize_T_1], dependencies: list[_module_0.Material[_module_0.Plasmid]] = ...) -> Effect[_module_0.Material[_Realize_T_1]]: ...
    @overload
    def __call__(self, design: _Realize_T_1, dependencies: list[_module_0.Material[_module_0.Plasmid]] = ...) -> Effect[_module_0.Material[_Realize_T_1]]: ...

realize: Final[_RealizeAction]
