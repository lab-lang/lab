"""Golden Gate assembly and heat-shock transformation.

The scale a reaction runs at is neither the design's identity nor the
laboratory's equipment: it is what this method needs to make one. A package
that builds by another method describes its own, and a design that builds by
this one imports it.

Every property here is optional, because a method's standard values stand
behind a design that states nothing."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from __future__ import annotations

from typing import Any, Final, Generic, Protocol, TypeVar

from lab._effects import Effect
from lab._expressions import Decimal, Quantity
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import WorkflowCall

import lab.bio.designs as _module_0

LAB_MODULE: Final[str]

class Plasmid(ArtifactKind, LabType): ...
"""What Golden Gate assembly needs to build a plasmid."""

class Strain(ArtifactKind, LabType): ...
"""What heat-shock transformation and plating need to build a strain."""
