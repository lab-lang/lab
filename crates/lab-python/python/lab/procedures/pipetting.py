"""Author device-neutral pipetting templates for ``lab.methods.TemplateExecution``.

Python constructs ordinary template data. Catalog validation checks task references in Rust;
refinement resolves parameters and validates the canonical program, including liquid accounting
and capability derivation. No Python callback runs during compilation or on an instrument.
"""

from __future__ import annotations

from collections.abc import Sequence
from copy import deepcopy
from dataclasses import dataclass
from decimal import Decimal
from types import TracebackType
from typing import Self

from ..methods import _local
from . import (
    DEGREE_CELSIUS,
    MICROLITRE,
    MILLIMETRE,
    PIPETTING_PROGRAM_V1,
    AboveLiquidDispense,
    AspirationStrategy,
    DispenseStrategy,
    InputOutputVesselRole,
    IntermediateVesselRole,
    LiquidAspiration,
    LiquidDispense,
    MaterialProductVesselRole,
    MaterialSourceVesselRole,
    MaterialSurfaceDispense,
    ProcedureInputVesselRole,
    ProductVesselRole,
    TrackedLiquidSurfaceAspiration,
    VesselBottomAspiration,
    VesselBottomDispense,
    VesselRole,
    VesselTopDispense,
)
from . import FluidPathPolicy as FluidPathPolicy
from . import MixTechnique as MixTechnique
from . import TemperatureRange as TemperatureRange
from . import TransferTechnique as TransferTechnique
from . import Volume as Volume

CONTRACT = PIPETTING_PROGRAM_V1
ISOLATED_DESTINATIONS = FluidPathPolicy.ISOLATED_DESTINATIONS
SHARED_SOURCE_NO_REENTRY = FluidPathPolicy.SHARED_SOURCE_NO_REENTRY


@dataclass(frozen=True, slots=True)
class VolumeParameter:
    """A task parameter that must resolve to a positive volume in QUDT microlitres."""

    id: str

    def __post_init__(self) -> None:
        _local(self.id, "Volume parameter")


@dataclass(frozen=True, slots=True)
class IntegerParameter:
    """A task parameter that must resolve to a unitless integer."""

    id: str

    def __post_init__(self) -> None:
        _local(self.id, "Integer parameter")


VolumeValue = Volume | VolumeParameter
IntegerValue = int | IntegerParameter


def volume_parameter(name: str) -> VolumeParameter:
    """Reference an enclosing task's Procedure parameter; this does not declare its value."""
    return VolumeParameter(name)


def integer_parameter(name: str) -> IntegerParameter:
    """Reference an enclosing task's unitless integer Procedure parameter."""
    return IntegerParameter(name)


def microlitres(value: str | int | Decimal) -> Volume:
    """An exact positive literal volume. Use a string or Decimal for fractional values."""
    if isinstance(value, bool) or not isinstance(value, str | int | Decimal):
        raise TypeError("microlitres requires a string, integer, or Decimal")
    volume = Volume(Decimal(value))
    _volume(volume)
    return volume


def _slot(kind: str, name: str) -> dict[str, object]:
    return {"$lab": {"kind": kind, "id": name}}


def _input(port: int) -> dict[str, object]:
    return {"$lab": {"kind": "input", "index": _count(port, "port", minimum=0)}}


def _quantity(value: Decimal, unit: str) -> dict[str, object]:
    if not value.is_finite():
        raise ValueError("Procedure quantities must be finite")
    return {"value": {"type": "real", "value": format(value, "f")}, "unit": unit}


def _volume(value: VolumeValue) -> dict[str, object]:
    if isinstance(value, VolumeParameter):
        return _slot("scalar", value.id)
    if not isinstance(value, Volume):
        raise TypeError("volume requires microlitres(...) or volume_parameter(...)")
    if not value.value.is_finite() or value.value <= 0:
        raise ValueError("volume must be finite and positive")
    return _quantity(value.value, MICROLITRE)


def _count(value: int, label: str, *, minimum: int = 1) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{label} must be an integer")
    if not minimum <= value <= 2**32 - 1:
        raise ValueError(f"{label} must be between {minimum} and {2**32 - 1}")
    return value


def _aspiration(value: AspirationStrategy) -> dict[str, object]:
    match value:
        case LiquidAspiration():
            return {"kind": "liquid"}
        case TrackedLiquidSurfaceAspiration():
            return {"kind": "tracked_liquid_surface"}
        case VesselBottomAspiration(offset):
            return {"kind": "vessel_bottom", "offset": _quantity(offset.value, MILLIMETRE)}
    raise TypeError("unknown aspiration strategy")


def _dispense(value: DispenseStrategy) -> dict[str, object]:
    match value:
        case LiquidDispense():
            return {"kind": "liquid"}
        case AboveLiquidDispense():
            return {"kind": "above_liquid"}
        case VesselBottomDispense(offset):
            return {"kind": "vessel_bottom", "offset": _quantity(offset.value, MILLIMETRE)}
        case VesselTopDispense(offset):
            return {"kind": "vessel_top", "offset": _quantity(offset.value, MILLIMETRE)}
        case MaterialSurfaceDispense():
            return {"kind": "material_surface"}
    raise TypeError("unknown dispense strategy")


def _technique(value: TransferTechnique | MixTechnique) -> dict[str, object]:
    if not isinstance(value.blow_out, bool) or not isinstance(value.touch_tip, bool):
        raise TypeError("blow_out and touch_tip must be booleans")
    result: dict[str, object] = {
        "aspiration": _aspiration(value.aspiration),
        "dispense": _dispense(value.dispense),
        "blow_out": value.blow_out,
        "touch_tip": value.touch_tip,
    }
    if isinstance(value, TransferTechnique) and value.air_gap is not None:
        result["air_gap"] = _volume(value.air_gap)
    return result


@dataclass(frozen=True, slots=True)
class Location:
    """A logical position belonging to one template, obtained from a vessel handle."""

    _owner: object
    vessel: str
    position: int


@dataclass(frozen=True, slots=True)
class VesselHandle:
    """A declared logical vessel. Positions are zero-based, independent of physical wells."""

    _owner: object
    id: str
    _positions: int

    def position(self, index: int) -> Location:
        _count(index, "position", minimum=0)
        if index >= self._positions:
            raise ValueError(
                f"vessel {self.id!r} has {self._positions} positions, requested {index}"
            )
        return Location(self._owner, self.id, index)

    def positions(self) -> tuple[Location, ...]:
        return tuple(self.position(index) for index in range(self._positions))


class Template:
    """Build a portable PipettingProgramV1 template with typed handles and parameters.

    ``to_template`` returns a detached JSON-compatible snapshot, not a validated program.
    Use it as a Method task's template body, then validate the catalog and refine the Method.
    """

    def __init__(self) -> None:
        self._owner = object()
        self._vessels: dict[str, dict[str, object]] = {}
        self._materials: dict[str, dict[str, object]] = {}
        self._outputs: dict[str, dict[str, object]] = {}
        self._steps: dict[str, dict[str, object]] = {}
        self._paths: set[str] = set()
        self._active_path: FluidPath | None = None

    def vessel(
        self,
        name: str,
        *,
        role: VesselRole,
        positions: int = 1,
        initial_volume: VolumeValue | None = None,
        working_capacity: VolumeValue | None = None,
        dead_volume: VolumeValue | None = None,
        temperature: TemperatureRange | None = None,
    ) -> VesselHandle:
        """Declare any canonical vessel role, with optional per-position constraints."""
        self._idle()
        _local(name, "Vessel ID")
        _count(positions, "positions")
        if name in self._vessels:
            raise ValueError(f"duplicate vessel {name!r}")
        encoded_role: dict[str, object]
        material: str | None = None
        output: str | None = None
        match role:
            case ProcedureInputVesselRole(port):
                encoded_role = {"kind": "procedure_input", "input": _input(port)}
            case MaterialSourceVesselRole(material):
                encoded_role = {"kind": "material_source"}
            case ProductVesselRole(output):
                encoded_role = {"kind": "product"}
            case InputOutputVesselRole(port, output):
                encoded_role = {"kind": "input_output", "input": _input(port)}
            case MaterialProductVesselRole(material, output):
                encoded_role = {"kind": "material_product"}
            case IntermediateVesselRole():
                encoded_role = {"kind": "intermediate"}
            case _:
                raise TypeError("unknown vessel role")
        if material is not None:
            encoded_role["material"] = _slot("material", _local(material, "Material ID"))
        if output is not None:
            encoded_role["output"] = _slot("output", _local(output, "Output ID"))
        vessel: dict[str, object] = {"id": name, "role": encoded_role, "positions": positions}
        for key, value in (
            ("initial_volume_each", initial_volume),
            ("working_capacity_each", working_capacity),
            ("dead_volume_each", dead_volume),
        ):
            if value is not None:
                vessel[key] = _volume(value)
        if temperature is not None:
            vessel["temperature"] = {
                "minimum": _quantity(temperature.minimum.value, DEGREE_CELSIUS),
                "maximum": _quantity(temperature.maximum.value, DEGREE_CELSIUS),
            }
        # Commit declarations only after all arguments have serialized successfully.
        if material is not None:
            self._materials[material] = {"id": encoded_role["material"]}
        if output is not None:
            self._outputs[output] = {"id": encoded_role["output"]}
        self._vessels[name] = vessel
        return VesselHandle(self._owner, name, positions)

    def input(
        self, name: str, *, port: int, positions: int = 1, initial_volume: VolumeValue
    ) -> VesselHandle:
        """Declare liquid arriving through a zero-based task input port."""
        return self.vessel(
            name,
            role=ProcedureInputVesselRole(port),
            positions=positions,
            initial_volume=initial_volume,
        )

    def product(self, name: str, *, output: str, positions: int = 1) -> VesselHandle:
        """Declare an initially empty vessel associated with a named task output."""
        return self.vessel(name, role=ProductVesselRole(output), positions=positions)

    def source(
        self,
        name: str,
        *,
        material: str,
        positions: int = 1,
        initial_volume: VolumeValue | None = None,
    ) -> VesselHandle:
        """Declare a task material source; allocation supplies its exact lot or upstream value."""
        return self.vessel(
            name,
            role=MaterialSourceVesselRole(material),
            positions=positions,
            initial_volume=initial_volume,
        )

    def _location(self, location: Location) -> dict[str, object]:
        if not isinstance(location, Location) or location._owner is not self._owner:
            raise ValueError("location belongs to another template")
        vessel = self._vessels.get(location.vessel)
        if vessel is None or not isinstance(vessel["positions"], int):
            raise ValueError("location refers to an undeclared vessel")
        _count(location.position, "position", minimum=0)
        if location.position >= vessel["positions"]:
            raise ValueError("location position is outside its declared vessel")
        return {"vessel": location.vessel, "position": location.position}

    def _idle(self) -> None:
        if self._active_path is not None:
            raise ValueError(
                "finish the active fluid path before changing or exporting the template"
            )

    def _append(self, name: str, step: dict[str, object]) -> None:
        self._idle()
        _local(name, "Step ID")
        if name in self._steps:
            raise ValueError(f"duplicate step {name!r}")
        self._steps[name] = {"id": name, **step}

    def distribute(
        self,
        name: str,
        source: Location,
        destinations: Sequence[Location],
        *,
        volume_each: VolumeValue,
        policy: FluidPathPolicy,
        technique: TransferTechnique | None = None,
    ) -> None:
        """Distribute to logical destinations with an explicit fluid-path reuse policy."""
        if not destinations:
            raise ValueError("distribute needs at least one destination")
        step: dict[str, object] = {
            "kind": "distribute",
            "source": self._location(source),
            "destinations": [self._location(destination) for destination in destinations],
            "volume_each": _volume(volume_each),
            "fluid_path": FluidPathPolicy(policy).value,
        }
        if technique is not None:
            step["technique"] = _technique(technique)
        self._append(name, step)

    def barrier(self, name: str, *, reason: str) -> None:
        """Record a semantic boundary between liquid operations."""
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError("a barrier needs a reason")
        self._append(name, {"kind": "barrier", "reason": reason})

    def path(self, name: str, *, policy: FluidPathPolicy) -> FluidPath:
        """Open a ``with`` block whose ordered steps require one continuous fluid path."""
        return FluidPath(self, name, FluidPathPolicy(policy))

    def to_template(self) -> dict[str, object]:
        """Export ordinary template data; Rust validation occurs through the enclosing Method."""
        self._idle()
        result: dict[str, object] = {
            "vessels": list(self._vessels.values()),
            "steps": list(self._steps.values()),
        }
        if self._materials:
            result["materials"] = list(self._materials.values())
        if self._outputs:
            result["outputs"] = list(self._outputs.values())
        return deepcopy(result)


class FluidPath:
    """A contiguous group of transfers and mixes, committed when its ``with`` block succeeds."""

    def __init__(self, template: Template, name: str, policy: FluidPathPolicy) -> None:
        self._template = template
        self._name = _local(name, "Fluid path ID")
        self._policy = policy
        self._steps: dict[str, dict[str, object]] = {}
        self._entered = False

    def __enter__(self) -> Self:
        self._template._idle()
        if self._entered or self._name in self._template._paths:
            raise ValueError(f"fluid path {self._name!r} has already been used")
        self._entered = True
        self._template._active_path = self
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self._check_active()
        self._template._active_path = None
        if exc_type is None:
            if not self._steps:
                raise ValueError("a fluid path needs at least one step")
            self._template._steps.update(self._steps)
            self._template._paths.add(self._name)

    def _check_active(self) -> None:
        if self._template._active_path is not self:
            raise ValueError("fluid path operations require its active with block")

    def _append(self, name: str, step: dict[str, object]) -> None:
        self._check_active()
        _local(name, "Step ID")
        if name in self._steps or name in self._template._steps:
            raise ValueError(f"duplicate step {name!r}")
        self._steps[name] = {
            "id": name,
            **step,
            "fluid_path": self._policy.value,
            "fluid_path_group": self._name,
        }

    def transfer(
        self,
        name: str,
        source: Location,
        destination: Location,
        *,
        volume: VolumeValue,
        technique: TransferTechnique | None = None,
    ) -> None:
        """Move liquid between two logical positions on this fluid path."""
        step: dict[str, object] = {
            "kind": "transfer",
            "source": self._template._location(source),
            "destination": self._template._location(destination),
            "volume": _volume(volume),
        }
        if technique is not None:
            step["technique"] = _technique(technique)
        self._append(name, step)

    def mix(
        self,
        name: str,
        target: Location,
        *,
        volume: VolumeValue,
        cycles: IntegerValue,
        technique: MixTechnique | None = None,
    ) -> None:
        """Mix one logical position, using a literal or task-parameter cycle count."""
        step: dict[str, object] = {
            "kind": "mix",
            "targets": [self._template._location(target)],
            "volume": _volume(volume),
            "cycles": _slot("integer", cycles.id)
            if isinstance(cycles, IntegerParameter)
            else _count(cycles, "cycles"),
        }
        if technique is not None:
            step["technique"] = _technique(technique)
        self._append(name, step)
