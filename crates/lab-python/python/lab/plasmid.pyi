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

_Provision_T_1 = TypeVar("_Provision_T_1")
_Dispose_T_1 = TypeVar("_Dispose_T_1")

LAB_MODULE: Final[str]

class SequenceCheck(LabConstructor):
    def __new__(cls, *, evidence: list[_module_0.Evidence], material: _module_0.Material[_module_0.Plasmid]) -> SequenceCheck: ...
"""A sequenced plasmid material together with the evidence used to judge it."""

class _ExactConstructor(Protocol):
    def __call__(self, *, evidence: list[_module_0.Evidence], material: _module_0.Material[_module_0.Plasmid]) -> SequenceCheck: ...

Exact: Final[_ExactConstructor]
"""A sequence that exactly matches the intended plasmid."""

class _MismatchConstructor(Protocol):
    def __call__(self, *, evidence: list[_module_0.Evidence], material: _module_0.Material[_module_0.Plasmid]) -> SequenceCheck: ...

Mismatch: Final[_MismatchConstructor]
"""A sequence that does not match the intended plasmid."""

class _InconclusiveConstructor(Protocol):
    def __call__(self, *, evidence: list[_module_0.Evidence], material: _module_0.Material[_module_0.Plasmid]) -> SequenceCheck: ...

Inconclusive: Final[_InconclusiveConstructor]
"""Evidence that is insufficient to judge the plasmid sequence."""

class _CaptureAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, plate: _module_0.Material[_module_1.inoculated[_module_0.Medium]]) -> Effect[_module_0.Image]: ...

capture: Final[_CaptureAction]

class _SynthesizeAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, design: _module_0.Plasmid | DesignReference[_module_0.Plasmid]) -> Effect[list[_module_0.Fragment]]: ...

synthesize: Final[_SynthesizeAction]

class _AssembleAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, fragments: list[_module_0.Fragment]) -> Effect[_module_0.Material[_module_0.Plasmid]]: ...

assemble: Final[_AssembleAction]

class _ProvisionAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    @overload
    def __call__(self, item: DesignReference[_Provision_T_1]) -> Effect[_module_0.Material[_Provision_T_1]]: ...
    @overload
    def __call__(self, item: _Provision_T_1) -> Effect[_module_0.Material[_Provision_T_1]]: ...

provision: Final[_ProvisionAction]

class _TransformAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, design: _module_0.Strain | DesignReference[_module_0.Strain], plasmids: list[_module_0.Material[_module_0.Plasmid]], cells: _module_0.Material[_module_1.competent[_module_0.Chassis]]) -> Effect[tuple[_module_0.Material[_module_0.Strain], _module_0.Material[_module_1.transformed[_module_0.Strain]]]]: ...

transform: Final[_TransformAction]

class _RecoverAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, culture: _module_0.Material[_module_1.transformed[_module_0.Strain]], duration: Quantity) -> Effect[_module_0.Material[_module_1.recovered[_module_0.Strain]]]: ...

recover: Final[_RecoverAction]

class _DiluteAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, culture: _module_0.Material[_module_1.recovered[_module_0.Strain]]) -> Effect[_module_0.Material[_module_1.diluted[_module_0.Strain]]]: ...

dilute: Final[_DiluteAction]

class _PlateAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, culture: _module_0.Material[_module_1.recovered[_module_0.Strain]] | _module_0.Material[_module_1.diluted[_module_0.Strain]], medium: _module_0.Material[_module_1.poured[_module_0.Medium]]) -> Effect[_module_0.Material[_module_1.inoculated[_module_0.Medium]]]: ...

plate: Final[_PlateAction]

class _PickAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, count: int, plate: _module_0.Material[_module_1.inoculated[_module_0.Medium]]) -> Effect[list[_module_0.Material[_module_1.isolated[_module_0.Strain]]]]: ...

pick: Final[_PickAction]

class _ScreenAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, candidates: list[_module_0.Material[_module_1.isolated[_module_0.Strain]]], design: _module_0.Plasmid | DesignReference[_module_0.Plasmid]) -> Effect[_module_0.Screening]: ...

screen: Final[_ScreenAction]

class _CultureAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, clone: _module_0.Material[_module_1.isolated[_module_0.Strain]], temperature: Quantity, duration: Quantity) -> Effect[_module_0.Material[_module_1.grown[_module_0.Strain]]]: ...

culture: Final[_CultureAction]

class _PurifyAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, culture: _module_0.Material[_module_1.grown[_module_0.Strain]]) -> Effect[_module_0.Material[_module_0.Plasmid]]: ...

purify: Final[_PurifyAction]

class _SplitAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, material: _module_0.Material[_module_0.Plasmid]) -> Effect[tuple[_module_0.Material[_module_0.Plasmid], _module_0.Material[_module_0.Plasmid]]]: ...

split: Final[_SplitAction]

class _SequenceAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, aliquot: _module_0.Material[_module_0.Plasmid]) -> Effect[SequenceCheck]: ...

sequence: Final[_SequenceAction]

class _QuantifyAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, material: _module_0.Material[_module_0.Plasmid]) -> Effect[_module_0.Evidence]: ...

quantify: Final[_QuantifyAction]

class _StoreAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, material: _module_0.Material[_module_0.Plasmid], temperature: Quantity) -> Effect[_module_0.Material[_module_0.Plasmid]]: ...

store: Final[_StoreAction]

class _DisposeAction(Protocol):
    @property
    def definition(self) -> tuple[str, str]: ...
    def __call__(self, material: _module_0.Material[_Dispose_T_1]) -> Effect[None]: ...

dispose: Final[_DisposeAction]
