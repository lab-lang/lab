"""Named biological parts and the roles they play.

A catalogued name says only that a supplier lists the item. It is not a claim
that a suitable lot is on the shelf; that remains an inventory resolution and
a runtime evidence question."""

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
import lab.bio.designs as _module_1

LAB_MODULE: Final[str]

class Arabinose(_module_0.Signal): ...
"""The inducer an arabinose-responsive promoter answers to."""

B0015: Final[Symbol]

B0034: Final[Symbol]

BsaI: Final[Symbol]

class GreenFluorescentProtein(_module_0.Protein): ...
"""A reporter protein read as green fluorescence."""

class Tetracycline(_module_0.Signal): ...
"""The inducer a tetracycline-responsive promoter answers to."""

pBAD: Final[Symbol]

pTet: Final[Symbol]

sfGFP: Final[Symbol]
