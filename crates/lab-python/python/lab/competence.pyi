"""Making a chassis competent.

Competent cells are grown up, chilled, spun into a pellet, and washed into a
cold buffer until they will take up DNA. None of these steps is assembly or
transformation, so none had a verb until a package could declare one. Each
verb here is an `action`: it names the material it takes, the material it
yields and the state that yielding leaves it in, and exactly which input
lineage the result continues. Methods separately state how a facility can
perform each action.

A buffer and a medium are both solutions a laboratory pours, and both are
washed or grown into cells, so they share the `Solution` role rather than
repeating what a solution can do on each kind."""

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

_Growth_T1_1 = TypeVar("_Growth_T1_1")

LAB_MODULE: Final[str]

Growth: Final[Symbol]


class dormant(LabState, Generic[_Growth_T1_1]): ...


class growing(LabState, Generic[_Growth_T1_1]): ...


class pelleted(LabState, Generic[_Growth_T1_1]): ...
"""How far a batch of cells is along the way to being competent.

Competence itself is a separate fact, declared where a chassis is: cells are
competent or they are not, and transformation is the one operation that
cares. These are the physical states a preparation passes through before it
gets there, so a verb that spins cells down can say it takes ones that are
growing and leaves ones that are pelleted."""

class Buffer(ArtifactKind, _module_0.Solution): ...
"""A salt solution cells are washed and resuspended in.

A buffer and a medium are both solutions a laboratory pours, so both play the
`Solution` role and a verb that resuspends cells asks for either. The
concentration is the batch's own: a competent-cell protocol is written for a
molarity of calcium chloride, and what to weigh out is that times the volume."""

class _CentrifugeAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, cells: _module_0.Material[growing[_module_1.Chassis]], force: Quantity, duration: Quantity) -> Effect[_module_0.Material[pelleted[_module_1.Chassis]]]: ...

centrifuge: Final[_CentrifugeAction]
"""Spin a chilled culture into a pellet at a stated relative force."""

class _ChillAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, cells: _module_0.Material[growing[_module_1.Chassis]], duration: Quantity) -> Effect[_module_0.Material[growing[_module_1.Chassis]]]: ...

chill: Final[_ChillAction]
"""Chill a growing culture on ice before it is spun down."""

class _GrowAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, cells: _module_0.Material[_module_1.Chassis], temperature: Quantity, target: Quantity | Quantity) -> Effect[_module_0.Material[growing[_module_1.Chassis]]]: ...

grow: Final[_GrowAction]
"""Grow cells up to a target optical density.

The target is read at 600 or 700 nanometres, and the two do not convert: an
OD600 of 0.4 is not an OD700 of 0.4, so the unit says which meter the number
came off. Either reaches the same growing culture, so the operand admits
either and the protocol writes whichever its plate reader reports."""

class _ResuspendAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, cells: _module_0.Material[pelleted[_module_1.Chassis]], buffer: _module_0.Material[Any]) -> Effect[_module_0.Material[_module_1.competent[_module_1.Chassis]]]: ...

resuspend: Final[_ResuspendAction]
"""Resuspend a pellet in cold buffer, which is the wash that makes it competent.

The buffer is a solution, so the same verb pours a calcium-chloride wash or
any other a protocol calls for. What comes out is competent: ready for the
one operation that takes cells that are."""
