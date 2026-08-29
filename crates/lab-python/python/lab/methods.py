"""Portable Method authoring and shared Rust refinement for Python frontends.

A Method says how one semantic Intent operation can become a facility-independent
Procedure graph. It may name semantic capabilities and constraints, but it never names
a Facility, Asset, CapabilityOffering, MaterialLot, adapter, or schedule. Those choices
belong to planning.

The classes in this module only provide a typed Python authoring surface and exact JSON
serialization. The Rust ``lab-method`` contract validates every catalog, and the Rust
compiler performs refinement and planning-problem projection for both Lab source and
Python-emitted programs.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from decimal import Decimal
from enum import StrEnum
from typing import Any, Self, cast

from ._native import refine_lab_modules as _refine_lab_modules
from ._native import validate_method_definitions as _validate_method_definitions
from ._program import Program

_INTEGER = re.compile(r"^[+-]?[0-9]+$")
_REAL = re.compile(r"^[+-]?(([0-9]+(\.[0-9]*)?)|(\.[0-9]+))$")
_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*$")
_FORBIDDEN_IRI = frozenset('<>"{}|\\^`')


def _local(value: str, field: str) -> str:
    if not value or any(character.isspace() or ord(character) < 32 for character in value):
        raise ValueError(f"{field} must be non-empty and contain no whitespace or controls")
    return value


def _iri(value: str, field: str) -> str:
    scheme, separator, rest = value.partition(":")
    if (
        not separator
        or not rest
        or _SCHEME.fullmatch(scheme) is None
        or any(character.isspace() or ord(character) < 32 for character in value)
        or any(character in _FORBIDDEN_IRI for character in value)
    ):
        raise ValueError(f"{field} must be an absolute IRI")
    return value


class ScalarType(StrEnum):
    """The closed scalar shapes accepted from Intent parameters."""

    TEXT = "text"
    INTEGER = "integer"
    REAL = "real"
    BOOLEAN = "boolean"
    IRI = "iri"


@dataclass(frozen=True, slots=True)
class ParameterType:
    """The structural type of an Intent parameter consumed by a Method."""

    kind: str
    scalar_type: ScalarType | None = None
    element_type: ScalarType | None = None

    def __post_init__(self) -> None:
        if self.kind == "scalar":
            if self.scalar_type is None or self.element_type is not None:
                raise ValueError("a scalar parameter type requires exactly one scalar type")
        elif self.kind == "list":
            if self.element_type is None or self.scalar_type is not None:
                raise ValueError("a list parameter type requires exactly one element type")
        else:
            raise ValueError(f"unknown parameter-type kind {self.kind!r}")

    @classmethod
    def scalar(cls, scalar_type: ScalarType) -> Self:
        return cls(kind="scalar", scalar_type=scalar_type)

    @classmethod
    def list(cls, element_type: ScalarType) -> Self:
        return cls(kind="list", element_type=element_type)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "scalar":
            assert self.scalar_type is not None
            return {"kind": self.kind, "scalar_type": self.scalar_type.value}
        assert self.element_type is not None
        return {"kind": self.kind, "element_type": self.element_type.value}


class Qualification(StrEnum):
    """The ordered SBOLInventory qualification ladder."""

    DISCOVERED = "https://sbol.io/ns/facility#Discovered"
    DESCRIBED = "https://sbol.io/ns/facility#Described"
    PLANNABLE = "https://sbol.io/ns/facility#Plannable"
    SIMULATABLE = "https://sbol.io/ns/facility#Simulatable"
    EXECUTABLE = "https://sbol.io/ns/facility#Executable"
    QUALIFIED = "https://sbol.io/ns/facility#Qualified"


class ControlMode(StrEnum):
    """The closed SBOLInventory control-mode vocabulary."""

    UNSPECIFIED = "https://sbol.io/ns/facility#UnspecifiedControl"
    MANUAL = "https://sbol.io/ns/facility#ManualControl"
    REVIEWED_FILE = "https://sbol.io/ns/facility#ReviewedFileControl"
    VENDOR_SESSION = "https://sbol.io/ns/facility#VendorSessionControl"
    API = "https://sbol.io/ns/facility#ApiControl"
    SILA2 = "https://sbol.io/ns/facility#SiLA2Control"
    OPC_UA = "https://sbol.io/ns/facility#OpcUaControl"


class ConstraintRelation(StrEnum):
    """How a required value is compared with an offering property."""

    EXACT = "exact"
    AT_LEAST = "at_least"
    AT_MOST = "at_most"


@dataclass(frozen=True, slots=True)
class Port:
    """The semantic type of a Method or Procedure value."""

    kind: str
    state: str | None = None
    data_kind: str | None = None

    def __post_init__(self) -> None:
        if self.kind == "design":
            if self.state is not None or self.data_kind is not None:
                raise ValueError("a design port cannot carry a state or data kind")
        elif self.kind == "material":
            if self.state is None or self.data_kind is not None:
                raise ValueError("a material port requires exactly one state IRI")
            _iri(self.state, "material state")
        elif self.kind == "data":
            if self.data_kind is None or self.state is not None:
                raise ValueError("a data port requires exactly one data-kind IRI")
            _iri(self.data_kind, "data kind")
        else:
            raise ValueError(f"unknown port kind {self.kind!r}")

    @classmethod
    def design(cls) -> Self:
        return cls(kind="design")

    @classmethod
    def material(cls, state: str) -> Self:
        return cls(kind="material", state=state)

    @classmethod
    def data(cls, data_kind: str) -> Self:
        return cls(kind="data", data_kind=data_kind)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "material":
            return {"kind": self.kind, "state": self.state}
        if self.kind == "data":
            return {"kind": self.kind, "data_kind": self.data_kind}
        return {"kind": self.kind}


@dataclass(frozen=True, slots=True)
class MethodInput:
    name: str
    port_type: Port

    def __post_init__(self) -> None:
        _local(self.name, "Method input name")

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "port_type": self.port_type.to_dict()}


@dataclass(frozen=True, slots=True)
class MethodParameter:
    name: str
    value_type: ParameterType

    def __post_init__(self) -> None:
        _local(self.name, "Method parameter name")

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "value_type": self.value_type.to_dict()}

    @classmethod
    def scalar(cls, name: str, scalar_type: ScalarType) -> Self:
        return cls(name=name, value_type=ParameterType.scalar(scalar_type))

    @classmethod
    def list(cls, name: str, element_type: ScalarType) -> Self:
        return cls(name=name, value_type=ParameterType.list(element_type))


@dataclass(frozen=True, slots=True)
class ValueReference:
    """A Method input or the named output of an earlier Procedure task."""

    kind: str
    input: str | None = None
    task: str | None = None
    output: str | None = None

    def __post_init__(self) -> None:
        if self.kind == "input":
            if self.input is None or self.task is not None or self.output is not None:
                raise ValueError("an input reference requires exactly one input name")
            _local(self.input, "input reference")
        elif self.kind == "task_output":
            if self.task is None or self.output is None or self.input is not None:
                raise ValueError("a task-output reference requires a task and output name")
            _local(self.task, "task reference")
            _local(self.output, "task output reference")
        else:
            raise ValueError(f"unknown value-reference kind {self.kind!r}")

    @classmethod
    def method_input(cls, name: str) -> Self:
        return cls(kind="input", input=name)

    @classmethod
    def task_output(cls, task: str, output: str) -> Self:
        return cls(kind="task_output", task=task, output=output)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "input":
            return {"kind": self.kind, "input": self.input}
        return {"kind": self.kind, "task": self.task, "output": self.output}


@dataclass(frozen=True, slots=True)
class TaskOutput:
    name: str
    port_type: Port

    def __post_init__(self) -> None:
        _local(self.name, "task output name")

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "port_type": self.port_type.to_dict()}


@dataclass(frozen=True, slots=True)
class MethodOutput:
    name: str
    source: ValueReference

    def __post_init__(self) -> None:
        _local(self.name, "Method output name")

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "source": self.source.to_dict()}


@dataclass(frozen=True, slots=True)
class Scalar:
    """An exactly serialized scalar used in a literal property value."""

    type: ScalarType
    value: str | bool

    @classmethod
    def text(cls, value: str) -> Self:
        return cls(type=ScalarType.TEXT, value=value)

    @classmethod
    def integer(cls, value: int | str) -> Self:
        lexical = str(value)
        if _INTEGER.fullmatch(lexical) is None:
            raise ValueError(f"{lexical!r} is not an exact integer")
        return cls(type=ScalarType.INTEGER, value=lexical)

    @classmethod
    def real(cls, value: Decimal | int | str) -> Self:
        lexical = str(value)
        if _REAL.fullmatch(lexical) is None:
            raise ValueError(f"{lexical!r} is not an exact decimal")
        return cls(type=ScalarType.REAL, value=lexical)

    @classmethod
    def boolean(cls, value: bool) -> Self:
        return cls(type=ScalarType.BOOLEAN, value=value)

    @classmethod
    def iri(cls, value: str) -> Self:
        return cls(type=ScalarType.IRI, value=_iri(value, "scalar IRI"))

    def __post_init__(self) -> None:
        if self.type is ScalarType.BOOLEAN:
            if not isinstance(self.value, bool):
                raise ValueError("a boolean scalar requires a bool")
        elif not isinstance(self.value, str):
            raise ValueError(f"a {self.type.value} scalar requires a string lexical value")

    def to_dict(self) -> dict[str, object]:
        return {"type": self.type.value, "value": self.value}


@dataclass(frozen=True, slots=True)
class PropertyValue:
    value: Scalar
    unit: str | None = None

    def __post_init__(self) -> None:
        if self.unit is not None:
            _iri(self.unit, "unit")
            if self.value.type not in (ScalarType.INTEGER, ScalarType.REAL):
                raise ValueError("only numeric property values may carry a unit")

    def to_dict(self) -> dict[str, object]:
        result: dict[str, object] = {"value": self.value.to_dict()}
        if self.unit is not None:
            result["unit"] = self.unit
        return result


@dataclass(frozen=True, slots=True)
class ValueExpression:
    """A literal or a reference to a scalar on the refined Intent operation."""

    kind: str
    literal: PropertyValue | None = None
    parameter: str | None = None
    unit: str | None = None

    def __post_init__(self) -> None:
        if self.kind == "literal":
            if self.literal is None or self.parameter is not None or self.unit is not None:
                raise ValueError("a literal expression requires exactly one property value")
        elif self.kind == "intent_parameter":
            if self.parameter is None or self.literal is not None:
                raise ValueError("an Intent-parameter expression requires exactly one parameter")
            _local(self.parameter, "Intent parameter reference")
            if self.unit is not None:
                _iri(self.unit, "Intent parameter unit")
        else:
            raise ValueError(f"unknown value-expression kind {self.kind!r}")

    @classmethod
    def constant(cls, value: PropertyValue) -> Self:
        return cls(kind="literal", literal=value)

    @classmethod
    def intent_parameter(cls, parameter: str, unit: str | None = None) -> Self:
        return cls(kind="intent_parameter", parameter=parameter, unit=unit)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "literal":
            assert self.literal is not None
            return {"kind": self.kind, "value": self.literal.to_dict()}
        result: dict[str, object] = {"kind": self.kind, "parameter": self.parameter}
        if self.unit is not None:
            result["unit"] = self.unit
        return result


@dataclass(frozen=True, slots=True)
class ProcedureValue:
    """An exact scalar or homogeneous ordered list carried into a Procedure task."""

    kind: str
    value: PropertyValue | None = None
    element_type: ScalarType | None = None
    values: tuple[PropertyValue, ...] = ()

    def __post_init__(self) -> None:
        if self.kind == "scalar":
            if self.value is None or self.element_type is not None or self.values:
                raise ValueError("a scalar Procedure value requires exactly one property value")
        elif self.kind == "list":
            if self.value is not None or self.element_type is None:
                raise ValueError("a list Procedure value requires an element type")
            if any(item.value.type is not self.element_type for item in self.values):
                raise ValueError("Procedure list values must match the declared element type")
        else:
            raise ValueError(f"unknown Procedure-value kind {self.kind!r}")

    @classmethod
    def scalar(cls, value: PropertyValue) -> Self:
        return cls(kind="scalar", value=value)

    @classmethod
    def list(cls, element_type: ScalarType, values: tuple[PropertyValue, ...] = ()) -> Self:
        return cls(kind="list", element_type=element_type, values=values)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "scalar":
            assert self.value is not None
            return {"kind": self.kind, "value": self.value.to_dict()}
        assert self.element_type is not None
        return {
            "kind": self.kind,
            "element_type": self.element_type.value,
            "values": [value.to_dict() for value in self.values],
        }


@dataclass(frozen=True, slots=True)
class ProcedureValueExpression:
    """A literal Procedure value or an exact reference to an Intent parameter."""

    kind: str
    literal: ProcedureValue | None = None
    parameter: str | None = None
    unit: str | None = None

    def __post_init__(self) -> None:
        if self.kind == "literal":
            if self.literal is None or self.parameter is not None or self.unit is not None:
                raise ValueError("a literal expression requires exactly one Procedure value")
        elif self.kind == "intent_parameter":
            if self.parameter is None or self.literal is not None:
                raise ValueError("an Intent-parameter expression requires exactly one parameter")
            _local(self.parameter, "Intent parameter reference")
            if self.unit is not None:
                _iri(self.unit, "Intent parameter unit")
        else:
            raise ValueError(f"unknown Procedure-value expression kind {self.kind!r}")

    @classmethod
    def constant(cls, value: ProcedureValue) -> Self:
        return cls(kind="literal", literal=value)

    @classmethod
    def intent_parameter(cls, parameter: str, unit: str | None = None) -> Self:
        return cls(kind="intent_parameter", parameter=parameter, unit=unit)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "literal":
            assert self.literal is not None
            return {"kind": self.kind, "value": self.literal.to_dict()}
        result: dict[str, object] = {"kind": self.kind, "parameter": self.parameter}
        if self.unit is not None:
            result["unit"] = self.unit
        return result


@dataclass(frozen=True, slots=True)
class CapabilityConstraint:
    property_kind: str
    relation: ConstraintRelation
    required: ValueExpression

    def __post_init__(self) -> None:
        _iri(self.property_kind, "constraint property kind")

    def to_dict(self) -> dict[str, object]:
        return {
            "property_kind": self.property_kind,
            "relation": self.relation.value,
            "required": self.required.to_dict(),
        }


@dataclass(frozen=True, slots=True)
class ProcedureParameter:
    id: str
    property_kind: str
    value: ProcedureValueExpression

    def __post_init__(self) -> None:
        _local(self.id, "Procedure parameter ID")
        _iri(self.property_kind, "Procedure parameter property kind")

    def to_dict(self) -> dict[str, object]:
        return {
            "id": self.id,
            "property_kind": self.property_kind,
            "value": self.value.to_dict(),
        }


@dataclass(frozen=True, slots=True)
class MaterialSource:
    """A literal inventory symbol or a symbol-valued Intent parameter."""

    kind: str
    symbol: str | None = None
    parameter: str | None = None

    def __post_init__(self) -> None:
        if self.kind == "literal":
            if not self.symbol or self.parameter is not None:
                raise ValueError("a literal material source requires one non-empty symbol")
        elif self.kind == "intent_parameter":
            if self.parameter is None or self.symbol is not None:
                raise ValueError("an Intent material source requires exactly one parameter")
            _local(self.parameter, "material Intent parameter")
        else:
            raise ValueError(f"unknown material-source kind {self.kind!r}")

    @classmethod
    def constant(cls, symbol: str) -> Self:
        return cls(kind="literal", symbol=symbol)

    @classmethod
    def intent_parameter(cls, parameter: str) -> Self:
        return cls(kind="intent_parameter", parameter=parameter)

    def to_dict(self) -> dict[str, object]:
        if self.kind == "literal":
            return {"kind": self.kind, "symbol": self.symbol}
        return {"kind": self.kind, "parameter": self.parameter}


@dataclass(frozen=True, slots=True)
class MaterialInput:
    """One external material source required by a Procedure task."""

    id: str
    source: MaterialSource

    def __post_init__(self) -> None:
        _local(self.id, "Procedure material input ID")

    def to_dict(self) -> dict[str, object]:
        return {"id": self.id, "source": self.source.to_dict()}


@dataclass(frozen=True, slots=True)
class Requirement:
    id: str
    capability_kind: str
    accepted_control_modes: tuple[ControlMode, ...]
    minimum_qualification: Qualification = Qualification.PLANNABLE
    constraints: tuple[CapabilityConstraint, ...] = ()

    def __post_init__(self) -> None:
        _local(self.id, "capability requirement ID")
        _iri(self.capability_kind, "capability kind")

    def to_dict(self) -> dict[str, object]:
        return {
            "id": self.id,
            "capability_kind": self.capability_kind,
            "minimum_qualification": self.minimum_qualification.value,
            "accepted_control_modes": sorted(mode.value for mode in self.accepted_control_modes),
            "constraints": [constraint.to_dict() for constraint in self.constraints],
        }


@dataclass(frozen=True, slots=True)
class Task:
    id: str
    operation: str
    requirements: tuple[Requirement, ...]
    inputs: tuple[ValueReference, ...] = ()
    outputs: tuple[TaskOutput, ...] = ()
    parameters: tuple[ProcedureParameter, ...] = ()
    materials: tuple[MaterialInput, ...] = ()

    def __post_init__(self) -> None:
        _local(self.id, "Procedure task ID")
        _iri(self.operation, "Procedure operation")

    def to_dict(self) -> dict[str, object]:
        return {
            "id": self.id,
            "operation": self.operation,
            "inputs": [reference.to_dict() for reference in self.inputs],
            "outputs": [output.to_dict() for output in self.outputs],
            "parameters": [parameter.to_dict() for parameter in self.parameters],
            "materials": [material.to_dict() for material in self.materials],
            "requirements": [requirement.to_dict() for requirement in self.requirements],
        }


@dataclass(frozen=True, slots=True)
class Method:
    """One portable candidate implementation of an Intent operation."""

    id: str
    refines: str
    tasks: tuple[Task, ...]
    inputs: tuple[MethodInput, ...] = ()
    parameters: tuple[MethodParameter, ...] = ()
    outputs: tuple[MethodOutput, ...] = ()

    def __post_init__(self) -> None:
        _iri(self.id, "Method ID")
        _local(self.refines, "refined Intent operation")

    def to_dict(self) -> dict[str, object]:
        return {
            "id": self.id,
            "refines": self.refines,
            "inputs": [item.to_dict() for item in self.inputs],
            "parameters": [parameter.to_dict() for parameter in self.parameters],
            "tasks": [task.to_dict() for task in self.tasks],
            "outputs": [output.to_dict() for output in self.outputs],
        }


@dataclass(frozen=True, slots=True)
class MethodCatalog:
    """A custom Method set, optionally composed with Lab's standard catalog."""

    methods: tuple[Method, ...] = ()
    include_standard: bool = True

    def to_json(self) -> str:
        return json.dumps([method.to_dict() for method in self.methods], separators=(",", ":"))

    def validate(self) -> list[dict[str, Any]]:
        """Validate the complete composed catalog with the Rust contract."""

        return cast(
            list[dict[str, Any]],
            json.loads(_validate_method_definitions(self.to_json(), self.include_standard)),
        )


@dataclass(frozen=True, slots=True)
class RefinedProgram:
    """The shared compiler's refined LAIR and facility planning problem."""

    lair: str
    planning_problem: dict[str, Any]


def refine(
    program: Program,
    *,
    methods: tuple[Method, ...] = (),
    include_standard: bool = True,
) -> RefinedProgram:
    """Refine a checked Python or Lab-source program through the shared compiler pipeline."""

    catalog = MethodCatalog(methods=methods, include_standard=include_standard)
    raw = cast(
        dict[str, Any],
        json.loads(
            _refine_lab_modules(
                list(program.sources.items()), catalog.to_json(), catalog.include_standard
            )
        ),
    )
    return RefinedProgram(
        lair=cast(str, raw["refined_lair"]),
        planning_problem=cast(dict[str, Any], raw["planning_problem"]),
    )


__all__ = [
    "CapabilityConstraint",
    "ConstraintRelation",
    "ControlMode",
    "MaterialInput",
    "MaterialSource",
    "Method",
    "MethodCatalog",
    "MethodInput",
    "MethodOutput",
    "MethodParameter",
    "ParameterType",
    "Port",
    "ProcedureParameter",
    "ProcedureValue",
    "ProcedureValueExpression",
    "PropertyValue",
    "Qualification",
    "RefinedProgram",
    "Requirement",
    "Scalar",
    "ScalarType",
    "Task",
    "TaskOutput",
    "ValueExpression",
    "ValueReference",
    "refine",
]
