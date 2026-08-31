"""Typed read views for Lab's canonical Procedure programs.

Canonical programs are produced and validated by the Rust compiler before facility planning. This
module gives Python callers the same versioned, device-neutral structures without reimplementing
normalization, capability derivation, allocation, or adapter lowering.
"""

from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from enum import StrEnum
from typing import Any, cast

PIPETTING_PROGRAM_V1 = "https://www.lab-compiler.org/ns/procedure-contract#PipettingProgramV1"
THERMAL_PROGRAM_V1 = "https://www.lab-compiler.org/ns/procedure-contract#ThermalProgramV1"

MICROLITRE = "http://qudt.org/vocab/unit/MicroL"
DEGREE_CELSIUS = "http://qudt.org/vocab/unit/DEG_C"
SECOND = "http://qudt.org/vocab/unit/SEC"
DEGREE_CELSIUS_PER_SECOND = "http://qudt.org/vocab/unit/DEG_C-PER-SEC"
MILLIMETRE = "http://qudt.org/vocab/unit/MilliM"


@dataclass(frozen=True, slots=True)
class Volume:
    """An exact positive volume in canonical QUDT microlitres."""

    value: Decimal


@dataclass(frozen=True, slots=True)
class Length:
    """An exact signed length in canonical QUDT millimetres."""

    value: Decimal


@dataclass(frozen=True, slots=True)
class Temperature:
    """An exact temperature in canonical QUDT degrees Celsius."""

    value: Decimal


@dataclass(frozen=True, slots=True)
class Duration:
    """An exact non-negative duration in canonical QUDT seconds."""

    value: Decimal


@dataclass(frozen=True, slots=True)
class TemperatureRampRate:
    """An exact positive ramp rate in canonical QUDT degrees Celsius per second."""

    value: Decimal


@dataclass(frozen=True, slots=True)
class TemperatureRange:
    minimum: Temperature
    maximum: Temperature


@dataclass(frozen=True, slots=True)
class MaterialInput:
    id: str


@dataclass(frozen=True, slots=True)
class MaterialOutput:
    id: str


@dataclass(frozen=True, slots=True)
class ProcedureInputVesselRole:
    input: int


@dataclass(frozen=True, slots=True)
class MaterialSourceVesselRole:
    material: str


@dataclass(frozen=True, slots=True)
class InputOutputVesselRole:
    input: int
    output: str


@dataclass(frozen=True, slots=True)
class ProductVesselRole:
    output: str


@dataclass(frozen=True, slots=True)
class MaterialProductVesselRole:
    material: str
    output: str


@dataclass(frozen=True, slots=True)
class IntermediateVesselRole:
    pass


VesselRole = (
    ProcedureInputVesselRole
    | MaterialSourceVesselRole
    | InputOutputVesselRole
    | ProductVesselRole
    | MaterialProductVesselRole
    | IntermediateVesselRole
)


@dataclass(frozen=True, slots=True)
class Vessel:
    id: str
    role: VesselRole
    positions: int
    initial_volume_each: Volume | None = None
    working_capacity_each: Volume | None = None
    dead_volume_each: Volume | None = None
    temperature: TemperatureRange | None = None


@dataclass(frozen=True, slots=True)
class Location:
    vessel: str
    position: int


class FluidPathPolicy(StrEnum):
    """The strongest fluid-path reuse a pipetting implementation may perform."""

    ISOLATED_DESTINATIONS = "isolated_destinations"
    SHARED_SOURCE_NO_REENTRY = "shared_source_no_reentry"


@dataclass(frozen=True, slots=True)
class LiquidAspiration:
    pass


@dataclass(frozen=True, slots=True)
class TrackedLiquidSurfaceAspiration:
    pass


@dataclass(frozen=True, slots=True)
class VesselBottomAspiration:
    offset: Length


AspirationStrategy = LiquidAspiration | TrackedLiquidSurfaceAspiration | VesselBottomAspiration


@dataclass(frozen=True, slots=True)
class LiquidDispense:
    pass


@dataclass(frozen=True, slots=True)
class AboveLiquidDispense:
    pass


@dataclass(frozen=True, slots=True)
class VesselBottomDispense:
    offset: Length


@dataclass(frozen=True, slots=True)
class VesselTopDispense:
    offset: Length


@dataclass(frozen=True, slots=True)
class MaterialSurfaceDispense:
    pass


DispenseStrategy = (
    LiquidDispense
    | AboveLiquidDispense
    | VesselBottomDispense
    | VesselTopDispense
    | MaterialSurfaceDispense
)


@dataclass(frozen=True, slots=True)
class TransferTechnique:
    aspiration: AspirationStrategy = LiquidAspiration()
    dispense: DispenseStrategy = LiquidDispense()
    air_gap: Volume | None = None
    blow_out: bool = False
    touch_tip: bool = False


@dataclass(frozen=True, slots=True)
class MixTechnique:
    aspiration: AspirationStrategy = LiquidAspiration()
    dispense: DispenseStrategy = LiquidDispense()
    blow_out: bool = False
    touch_tip: bool = False


@dataclass(frozen=True, slots=True)
class Transfer:
    id: str
    source: Location
    destination: Location
    volume: Volume
    fluid_path: FluidPathPolicy
    fluid_path_group: str | None
    technique: TransferTechnique


@dataclass(frozen=True, slots=True)
class Distribute:
    id: str
    source: Location
    destinations: tuple[Location, ...]
    volume_each: Volume
    fluid_path: FluidPathPolicy
    fluid_path_group: str | None
    technique: TransferTechnique


@dataclass(frozen=True, slots=True)
class Mix:
    id: str
    targets: tuple[Location, ...]
    cycles: int
    volume: Volume
    fluid_path: FluidPathPolicy
    fluid_path_group: str | None
    technique: MixTechnique


@dataclass(frozen=True, slots=True)
class Barrier:
    id: str
    reason: str


PipettingStep = Transfer | Distribute | Mix | Barrier


@dataclass(frozen=True, slots=True)
class PipettingConstraints:
    """Cross-cutting conditions every realization must preserve. Staging temperature is a property
    of each vessel rather than of the program, so this carries nothing today."""


@dataclass(frozen=True, slots=True)
class PipettingProgramV1:
    """A validated, device-neutral sequence of logical liquid operations."""

    materials: tuple[MaterialInput, ...]
    outputs: tuple[MaterialOutput, ...]
    vessels: tuple[Vessel, ...]
    steps: tuple[PipettingStep, ...]
    constraints: PipettingConstraints


@dataclass(frozen=True, slots=True)
class ThermalLoad:
    input: int
    outputs: tuple[str, ...]
    sample_count: int
    volume_each: Volume


@dataclass(frozen=True, slots=True)
class ThermalStep:
    id: str
    temperature: Temperature
    hold: Duration
    ramp_rate: TemperatureRampRate | None = None


@dataclass(frozen=True, slots=True)
class ThermalStage:
    id: str
    repeats: int
    steps: tuple[ThermalStep, ...]


@dataclass(frozen=True, slots=True)
class ThermalProgramV1:
    """A validated, device-neutral thermal profile and its material transition."""

    load: ThermalLoad
    lid_temperature: Temperature | None
    stages: tuple[ThermalStage, ...]
    final_hold: Temperature | None


CanonicalProcedureBody = PipettingProgramV1 | ThermalProgramV1


@dataclass(frozen=True, slots=True)
class ProcedureProgram:
    """The exact contract identity and typed body frozen onto an allocated task."""

    contract: str
    body: CanonicalProcedureBody


def parse_program(raw: dict[str, Any]) -> ProcedureProgram:
    """Parse a compiler-validated canonical program and fail closed on an unknown contract."""

    contract = cast(str, raw["contract"])
    body = cast(dict[str, Any], raw["body"])
    if contract == PIPETTING_PROGRAM_V1:
        parsed: CanonicalProcedureBody = _pipetting_program(body)
    elif contract == THERMAL_PROGRAM_V1:
        parsed = _thermal_program(body)
    else:
        raise ValueError(f"unknown canonical Procedure contract {contract!r}")
    return ProcedureProgram(contract=contract, body=parsed)


def _pipetting_program(raw: dict[str, Any]) -> PipettingProgramV1:
    return PipettingProgramV1(
        materials=tuple(
            MaterialInput(id=cast(str, item["id"]))
            for item in cast(list[dict[str, Any]], raw.get("materials", []))
        ),
        outputs=tuple(
            MaterialOutput(id=cast(str, item["id"]))
            for item in cast(list[dict[str, Any]], raw.get("outputs", []))
        ),
        vessels=tuple(_vessel(item) for item in cast(list[dict[str, Any]], raw.get("vessels", []))),
        steps=tuple(
            _pipetting_step(item) for item in cast(list[dict[str, Any]], raw.get("steps", []))
        ),
        constraints=PipettingConstraints(),
    )


def _vessel(raw: dict[str, Any]) -> Vessel:
    role = cast(dict[str, Any], raw["role"])
    kind = cast(str, role["kind"])
    if kind == "procedure_input":
        parsed_role: VesselRole = ProcedureInputVesselRole(input=cast(int, role["input"]))
    elif kind == "material_source":
        parsed_role = MaterialSourceVesselRole(material=cast(str, role["material"]))
    elif kind == "input_output":
        parsed_role = InputOutputVesselRole(
            input=cast(int, role["input"]), output=cast(str, role["output"])
        )
    elif kind == "product":
        parsed_role = ProductVesselRole(output=cast(str, role["output"]))
    elif kind == "material_product":
        parsed_role = MaterialProductVesselRole(
            material=cast(str, role["material"]), output=cast(str, role["output"])
        )
    elif kind == "intermediate":
        parsed_role = IntermediateVesselRole()
    else:
        raise ValueError(f"unknown canonical vessel role {kind!r}")
    return Vessel(
        id=cast(str, raw["id"]),
        role=parsed_role,
        positions=cast(int, raw["positions"]),
        initial_volume_each=_optional_volume(raw, "initial_volume_each"),
        working_capacity_each=_optional_volume(raw, "working_capacity_each"),
        dead_volume_each=_optional_volume(raw, "dead_volume_each"),
        temperature=(
            _temperature_range(cast(dict[str, Any], raw["temperature"]))
            if raw.get("temperature") is not None
            else None
        ),
    )


def _optional_volume(raw: dict[str, Any], key: str) -> Volume | None:
    value = raw.get(key)
    return _volume(cast(dict[str, Any], value)) if value is not None else None


def _location(raw: dict[str, Any]) -> Location:
    return Location(vessel=cast(str, raw["vessel"]), position=cast(int, raw["position"]))


def _pipetting_step(raw: dict[str, Any]) -> PipettingStep:
    kind = cast(str, raw["kind"])
    if kind == "transfer":
        return Transfer(
            id=cast(str, raw["id"]),
            source=_location(cast(dict[str, Any], raw["source"])),
            destination=_location(cast(dict[str, Any], raw["destination"])),
            volume=_volume(cast(dict[str, Any], raw["volume"])),
            fluid_path=FluidPathPolicy(cast(str, raw["fluid_path"])),
            fluid_path_group=cast(str | None, raw.get("fluid_path_group")),
            technique=_transfer_technique(cast(dict[str, Any], raw.get("technique", {}))),
        )
    if kind == "distribute":
        return Distribute(
            id=cast(str, raw["id"]),
            source=_location(cast(dict[str, Any], raw["source"])),
            destinations=tuple(
                _location(item) for item in cast(list[dict[str, Any]], raw.get("destinations", []))
            ),
            volume_each=_volume(cast(dict[str, Any], raw["volume_each"])),
            fluid_path=FluidPathPolicy(cast(str, raw["fluid_path"])),
            fluid_path_group=cast(str | None, raw.get("fluid_path_group")),
            technique=_transfer_technique(cast(dict[str, Any], raw.get("technique", {}))),
        )
    if kind == "mix":
        return Mix(
            id=cast(str, raw["id"]),
            targets=tuple(
                _location(item) for item in cast(list[dict[str, Any]], raw.get("targets", []))
            ),
            cycles=cast(int, raw["cycles"]),
            volume=_volume(cast(dict[str, Any], raw["volume"])),
            fluid_path=FluidPathPolicy(cast(str, raw["fluid_path"])),
            fluid_path_group=cast(str | None, raw.get("fluid_path_group")),
            technique=_mix_technique(cast(dict[str, Any], raw.get("technique", {}))),
        )
    if kind == "barrier":
        return Barrier(id=cast(str, raw["id"]), reason=cast(str, raw["reason"]))
    raise ValueError(f"unknown canonical pipetting step {kind!r}")


def _aspiration_strategy(raw: dict[str, Any]) -> AspirationStrategy:
    kind = cast(str, raw.get("kind", "liquid"))
    if kind == "liquid":
        return LiquidAspiration()
    if kind == "tracked_liquid_surface":
        return TrackedLiquidSurfaceAspiration()
    if kind == "vessel_bottom":
        return VesselBottomAspiration(offset=_length(cast(dict[str, Any], raw["offset"])))
    raise ValueError(f"unknown canonical aspiration strategy {kind!r}")


def _dispense_strategy(raw: dict[str, Any]) -> DispenseStrategy:
    kind = cast(str, raw.get("kind", "liquid"))
    if kind == "liquid":
        return LiquidDispense()
    if kind == "above_liquid":
        return AboveLiquidDispense()
    if kind == "vessel_bottom":
        return VesselBottomDispense(offset=_length(cast(dict[str, Any], raw["offset"])))
    if kind == "vessel_top":
        return VesselTopDispense(offset=_length(cast(dict[str, Any], raw["offset"])))
    if kind == "material_surface":
        return MaterialSurfaceDispense()
    raise ValueError(f"unknown canonical dispense strategy {kind!r}")


def _transfer_technique(raw: dict[str, Any]) -> TransferTechnique:
    air_gap = raw.get("air_gap")
    return TransferTechnique(
        aspiration=_aspiration_strategy(cast(dict[str, Any], raw.get("aspiration", {}))),
        dispense=_dispense_strategy(cast(dict[str, Any], raw.get("dispense", {}))),
        air_gap=_volume(cast(dict[str, Any], air_gap)) if air_gap is not None else None,
        blow_out=cast(bool, raw.get("blow_out", False)),
        touch_tip=cast(bool, raw.get("touch_tip", False)),
    )


def _mix_technique(raw: dict[str, Any]) -> MixTechnique:
    return MixTechnique(
        aspiration=_aspiration_strategy(cast(dict[str, Any], raw.get("aspiration", {}))),
        dispense=_dispense_strategy(cast(dict[str, Any], raw.get("dispense", {}))),
        blow_out=cast(bool, raw.get("blow_out", False)),
        touch_tip=cast(bool, raw.get("touch_tip", False)),
    )


def _thermal_program(raw: dict[str, Any]) -> ThermalProgramV1:
    load = cast(dict[str, Any], raw["load"])
    lid = raw.get("lid_temperature")
    final_hold = raw.get("final_hold")
    return ThermalProgramV1(
        load=ThermalLoad(
            input=cast(int, load["input"]),
            outputs=tuple(cast(list[str], load["outputs"])),
            sample_count=cast(int, load["sample_count"]),
            volume_each=_volume(cast(dict[str, Any], load["volume_each"])),
        ),
        lid_temperature=(_temperature(cast(dict[str, Any], lid)) if lid is not None else None),
        stages=tuple(
            ThermalStage(
                id=cast(str, stage["id"]),
                repeats=cast(int, stage["repeats"]),
                steps=tuple(
                    ThermalStep(
                        id=cast(str, step["id"]),
                        temperature=_temperature(cast(dict[str, Any], step["temperature"])),
                        hold=_duration(cast(dict[str, Any], step["hold"])),
                        ramp_rate=(
                            _ramp_rate(cast(dict[str, Any], step["ramp_rate"]))
                            if step.get("ramp_rate") is not None
                            else None
                        ),
                    )
                    for step in cast(list[dict[str, Any]], stage.get("steps", []))
                ),
            )
            for stage in cast(list[dict[str, Any]], raw.get("stages", []))
        ),
        final_hold=(
            _temperature(cast(dict[str, Any], final_hold)) if final_hold is not None else None
        ),
    )


def _temperature_range(raw: dict[str, Any]) -> TemperatureRange:
    return TemperatureRange(
        minimum=_temperature(cast(dict[str, Any], raw["minimum"])),
        maximum=_temperature(cast(dict[str, Any], raw["maximum"])),
    )


def _exact_decimal(raw: dict[str, Any], expected_unit: str, quantity: str) -> Decimal:
    unit = raw.get("unit")
    if unit != expected_unit:
        raise ValueError(f"canonical {quantity} must use unit {expected_unit!r}, found {unit!r}")
    scalar = cast(dict[str, Any], raw["value"])
    if scalar.get("type") != "real" or not isinstance(scalar.get("value"), str):
        raise ValueError(f"canonical {quantity} must carry an exact real lexical value")
    lexical = cast(str, scalar["value"])
    try:
        value = Decimal(lexical)
    except InvalidOperation as error:
        raise ValueError(f"canonical {quantity} has invalid exact value {lexical!r}") from error
    if not value.is_finite():
        raise ValueError(f"canonical {quantity} must be finite")
    return value


def _volume(raw: dict[str, Any]) -> Volume:
    value = _exact_decimal(raw, MICROLITRE, "volume")
    if value <= 0:
        raise ValueError("canonical volume must be greater than zero")
    return Volume(value)


def _length(raw: dict[str, Any]) -> Length:
    return Length(_exact_decimal(raw, MILLIMETRE, "length"))


def _temperature(raw: dict[str, Any]) -> Temperature:
    return Temperature(_exact_decimal(raw, DEGREE_CELSIUS, "temperature"))


def _duration(raw: dict[str, Any]) -> Duration:
    value = _exact_decimal(raw, SECOND, "duration")
    if value < 0:
        raise ValueError("canonical duration must not be negative")
    return Duration(value)


def _ramp_rate(raw: dict[str, Any]) -> TemperatureRampRate:
    value = _exact_decimal(raw, DEGREE_CELSIUS_PER_SECOND, "temperature ramp rate")
    if value <= 0:
        raise ValueError("canonical temperature ramp rate must be greater than zero")
    return TemperatureRampRate(value)


__all__ = [
    "DEGREE_CELSIUS",
    "DEGREE_CELSIUS_PER_SECOND",
    "MICROLITRE",
    "MILLIMETRE",
    "PIPETTING_PROGRAM_V1",
    "SECOND",
    "THERMAL_PROGRAM_V1",
    "AboveLiquidDispense",
    "AspirationStrategy",
    "Barrier",
    "CanonicalProcedureBody",
    "Distribute",
    "Duration",
    "FluidPathPolicy",
    "InputOutputVesselRole",
    "IntermediateVesselRole",
    "Length",
    "LiquidAspiration",
    "LiquidDispense",
    "Location",
    "MaterialInput",
    "MaterialOutput",
    "MaterialProductVesselRole",
    "MaterialSourceVesselRole",
    "MaterialSurfaceDispense",
    "Mix",
    "MixTechnique",
    "PipettingConstraints",
    "PipettingProgramV1",
    "PipettingStep",
    "ProcedureInputVesselRole",
    "ProcedureProgram",
    "ProductVesselRole",
    "Temperature",
    "TemperatureRampRate",
    "TemperatureRange",
    "ThermalLoad",
    "ThermalProgramV1",
    "ThermalStage",
    "ThermalStep",
    "TrackedLiquidSurfaceAspiration",
    "Transfer",
    "TransferTechnique",
    "Vessel",
    "VesselBottomAspiration",
    "VesselBottomDispense",
    "VesselRole",
    "VesselTopDispense",
    "Volume",
    "parse_program",
]
