"""A scientific operation implemented entirely with portable pipetting steps."""

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

class _HomogenizeAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, sample: _module_0.Material[_module_1.Medium]) -> Effect[_module_0.Material[_module_1.Medium]]: ...

homogenize: Final[_HomogenizeAction]
"""Move the complete 30 uL sample to a fresh vessel and mix it three times."""
