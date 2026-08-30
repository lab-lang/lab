"""Typed access to Lab's shared facility-planning pipeline.

Python-authored and file-backed Lab programs use the same Rust compiler service. A plan is produced
only after Method refinement, exact SBOLInventory MaterialLot and capability allocation, allocated
Procedure LAIR verification, and adapter-invocation projection all succeed.
"""

from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any, cast

from ._native import plan_lab_modules as _plan_lab_modules
from ._native import plan_lab_project as _plan_lab_project
from ._program import Program
from .methods import (
    ConstraintRelation,
    Method,
    MethodCatalog,
    Port,
    ProcedureValue,
    PropertyValue,
    Scalar,
    ScalarType,
)
from .procedures import ProcedureProgram, parse_program


@dataclass(frozen=True, slots=True)
class InventorySelection:
    """The exact catalog bytes and Facility selected for planning."""

    document: Path
    sha256: str
    facility: str


@dataclass(frozen=True, slots=True)
class MaterialLotSource:
    """An existing physical lot selected through an exact SBOL Component identity."""

    component: str
    material_lot: str


@dataclass(frozen=True, slots=True)
class ChoiceOutputSource:
    """A physical input produced by another selected Method choice in this plan."""

    choice: str


SelectedMaterialSource = MaterialLotSource | ChoiceOutputSource


@dataclass(frozen=True, slots=True)
class MaterialBinding:
    input: str
    symbol: str
    source: SelectedMaterialSource


@dataclass(frozen=True, slots=True)
class MaterialCandidates:
    """The exact SBOL Component identity and active candidate lots for one program symbol."""

    status: str
    component: str | None
    material_lots: tuple[str, ...]

    @property
    def identified(self) -> bool:
        return self.status == "identified"


@dataclass(frozen=True, slots=True)
class MaterialInventory:
    """The immutable inventory projection used by the solver and every adapter invocation."""

    source_sha256: str
    facility: str
    materials: Mapping[str, MaterialCandidates]
    artifacts: Mapping[str, MaterialCandidates]


@dataclass(frozen=True, slots=True)
class AdapterSelection:
    """The exact implementation and profile frozen for one Asset binding."""

    driver: str
    procedure_implementation: str | None
    profile_path: Path
    profile_sha256: str
    features: tuple[str, ...]
    accepted_run_formats: tuple[str, ...]
    emitted_run_formats: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class CapabilityParameterMatch:
    """One typed requirement constraint matched against one offering parameter."""

    property_kind: str
    relation: ConstraintRelation
    required: PropertyValue
    offering_parameter: str
    observed: PropertyValue


@dataclass(frozen=True, slots=True)
class RejectedOffering:
    offering: str
    asset: str
    observed_qualification: str
    control_mode: str
    reasons: tuple[dict[str, Any], ...]


@dataclass(frozen=True, slots=True)
class RequirementBinding:
    """One selected semantic requirement binding, including planner explanations."""

    requirement: str
    capability_kind: str
    minimum_qualification: str
    accepted_control_modes: tuple[str, ...]
    offering: str
    asset: str
    observed_qualification: str
    control_mode: str
    parameters: tuple[CapabilityParameterMatch, ...]
    adapter: AdapterSelection | None
    rejected_candidates: tuple[RejectedOffering, ...]


@dataclass(frozen=True, slots=True)
class ProcedureTaskSelection:
    task: str
    materials: tuple[MaterialBinding, ...]
    requirements: tuple[RequirementBinding, ...]


@dataclass(frozen=True, slots=True)
class MethodSelection:
    choice: str
    source_operation: str
    method: str
    tasks: tuple[ProcedureTaskSelection, ...]


@dataclass(frozen=True, slots=True)
class FacilitySolution:
    """The globally selected Methods, materials, offerings, Assets, and adapters."""

    schema_version: str
    problem_sha256: str
    inventory_sha256: str
    facility: str
    policy: dict[str, Any]
    selections: tuple[MethodSelection, ...]


@dataclass(frozen=True, slots=True)
class ChoiceInputValueSource:
    input: str


@dataclass(frozen=True, slots=True)
class ChoiceOutputValueSource:
    choice: str
    output: str


@dataclass(frozen=True, slots=True)
class TaskOutputValueSource:
    task: str
    output: str


ProcedureValueSource = ChoiceInputValueSource | ChoiceOutputValueSource | TaskOutputValueSource


@dataclass(frozen=True, slots=True)
class ProcedureTaskInput:
    source: ProcedureValueSource
    port_type: Port


@dataclass(frozen=True, slots=True)
class ProcedureTaskOutput:
    name: str
    port_type: Port


@dataclass(frozen=True, slots=True)
class AllocatedProcedureParameter:
    id: str
    property_kind: str
    value: ProcedureValue


@dataclass(frozen=True, slots=True)
class AllocatedRequirement:
    """One verified requirement with its exact catalog and implementation binding."""

    id: str
    capability_kind: str
    minimum_qualification: str
    accepted_control_modes: tuple[str, ...]
    offering: str
    asset: str
    observed_qualification: str
    control_mode: str
    parameters: tuple[CapabilityParameterMatch, ...]
    procedure_implementation: str | None
    adapter: AdapterSelection | None


@dataclass(frozen=True, slots=True)
class AllocatedProcedureTask:
    """One exact Procedure node after Method and facility allocation."""

    id: str
    operation: str
    program: ProcedureProgram | None
    inputs: tuple[ProcedureTaskInput, ...]
    outputs: tuple[ProcedureTaskOutput, ...]
    parameters: tuple[AllocatedProcedureParameter, ...]
    materials: tuple[MaterialBinding, ...]
    requirements: tuple[AllocatedRequirement, ...]


@dataclass(frozen=True, slots=True)
class AllocatedMethod:
    choice: str
    source_operation: str
    method: str
    tasks: tuple[AllocatedProcedureTask, ...]


@dataclass(frozen=True, slots=True)
class AdapterInvocation:
    """The exact subset of allocated Procedure work delivered to one adapter."""

    id: str
    asset: str
    adapter: AdapterSelection
    tasks: tuple[str, ...]
    requirements: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class AdapterInvocationPlan:
    """The immutable backend-facing projection of one allocated Procedure program."""

    schema_version: str
    problem_sha256: str
    allocated_lair_sha256: str
    inventory_sha256: str
    facility: str
    material_inventory: MaterialInventory
    methods: tuple[AllocatedMethod, ...]
    invocations: tuple[AdapterInvocation, ...]

    def task(self, task_id: str) -> AllocatedProcedureTask:
        """Resolve an exact task ID from the verified allocated graph."""

        for method in self.methods:
            for task in method.tasks:
                if task.id == task_id:
                    return task
        raise KeyError(task_id)

    def requirement(self, requirement_id: str) -> AllocatedRequirement:
        """Resolve an exact requirement ID from the verified allocated graph."""

        for method in self.methods:
            for task in method.tasks:
                for requirement in task.requirements:
                    if requirement.id == requirement_id:
                        return requirement
        raise KeyError(requirement_id)

    def invocation(self, invocation_id: str) -> AdapterInvocation:
        for invocation in self.invocations:
            if invocation.id == invocation_id:
                return invocation
        raise KeyError(invocation_id)

    def tasks_for(self, invocation: AdapterInvocation | str) -> tuple[AllocatedProcedureTask, ...]:
        """Resolve the exact typed Procedure tasks assigned to one adapter invocation."""

        selected = self.invocation(invocation) if isinstance(invocation, str) else invocation
        return tuple(self.task(task_id) for task_id in selected.tasks)


@dataclass(frozen=True, slots=True)
class FacilityPlan:
    """One complete result from portable Intent through exact adapter invocations."""

    schema_version: str
    package: str
    version: str
    inventory: InventorySelection
    refined_lair: str
    planning_problem: dict[str, Any]
    solution: FacilitySolution
    allocated_lair: str
    adapter_bindings: dict[str, Any] | None
    adapter_invocations: AdapterInvocationPlan
    raw_invocation_plan: dict[str, Any]

    @property
    def invocation_plan(self) -> dict[str, Any]:
        """Raw versioned record for forward-compatible interoperation."""

        return self.raw_invocation_plan

    @property
    def invocations(self) -> tuple[AdapterInvocation, ...]:
        return self.adapter_invocations.invocations

    @property
    def methods(self) -> tuple[AllocatedMethod, ...]:
        return self.adapter_invocations.methods

    @property
    def material_inventory(self) -> MaterialInventory:
        return self.adapter_invocations.material_inventory

    def task(self, task_id: str) -> AllocatedProcedureTask:
        return self.adapter_invocations.task(task_id)

    def invocation_tasks(
        self, invocation: AdapterInvocation | str
    ) -> tuple[AllocatedProcedureTask, ...]:
        return self.adapter_invocations.tasks_for(invocation)


def _adapter(raw: dict[str, Any]) -> AdapterSelection:
    return AdapterSelection(
        driver=cast(str, raw["driver"]),
        procedure_implementation=cast(str | None, raw.get("procedure_implementation")),
        profile_path=Path(cast(str, raw["profile_path"])),
        profile_sha256=cast(str, raw["profile_sha256"]),
        features=tuple(cast(list[str], raw.get("features", []))),
        accepted_run_formats=tuple(cast(list[str], raw.get("accepted_run_formats", []))),
        emitted_run_formats=tuple(cast(list[str], raw.get("emitted_run_formats", []))),
    )


def _property_value(raw: dict[str, Any]) -> PropertyValue:
    scalar = cast(dict[str, Any], raw["value"])
    return PropertyValue(
        value=Scalar(
            type=ScalarType(cast(str, scalar["type"])),
            value=cast(str | bool, scalar["value"]),
        ),
        unit=cast(str | None, raw.get("unit")),
    )


def _procedure_value(raw: dict[str, Any]) -> ProcedureValue:
    kind = cast(str, raw["kind"])
    if kind == "scalar":
        return ProcedureValue.scalar(_property_value(cast(dict[str, Any], raw["value"])))
    if kind == "list":
        return ProcedureValue.list(
            ScalarType(cast(str, raw["element_type"])),
            tuple(
                _property_value(value)
                for value in cast(list[dict[str, Any]], raw.get("values", []))
            ),
        )
    raise ValueError(f"unknown Procedure value kind {kind!r}")


def _parameter_match(raw: dict[str, Any]) -> CapabilityParameterMatch:
    return CapabilityParameterMatch(
        property_kind=cast(str, raw["property_kind"]),
        relation=ConstraintRelation(cast(str, raw["relation"])),
        required=_property_value(cast(dict[str, Any], raw["required"])),
        offering_parameter=cast(str, raw["offering_parameter"]),
        observed=_property_value(cast(dict[str, Any], raw["observed"])),
    )


def _material(raw: dict[str, Any]) -> MaterialBinding:
    source = cast(dict[str, Any], raw["source"])
    if source["kind"] == "material_lot":
        selected: SelectedMaterialSource = MaterialLotSource(
            component=cast(str, source["component"]),
            material_lot=cast(str, source["material_lot"]),
        )
    elif source["kind"] == "choice_output":
        selected = ChoiceOutputSource(choice=cast(str, source["choice"]))
    else:
        raise ValueError(f"unknown selected material source {source['kind']!r}")
    return MaterialBinding(
        input=cast(str, raw["input"]),
        symbol=cast(str, raw["symbol"]),
        source=selected,
    )


def _rejected_offering(raw: dict[str, Any]) -> RejectedOffering:
    return RejectedOffering(
        offering=cast(str, raw["offering"]),
        asset=cast(str, raw["asset"]),
        observed_qualification=cast(str, raw["observed_qualification"]),
        control_mode=cast(str, raw["control_mode"]),
        reasons=tuple(cast(list[dict[str, Any]], raw.get("reasons", []))),
    )


def _requirement(raw: dict[str, Any]) -> RequirementBinding:
    adapter = raw.get("adapter")
    return RequirementBinding(
        requirement=cast(str, raw["requirement"]),
        capability_kind=cast(str, raw["capability_kind"]),
        minimum_qualification=cast(str, raw["minimum_qualification"]),
        accepted_control_modes=tuple(cast(list[str], raw["accepted_control_modes"])),
        offering=cast(str, raw["offering"]),
        asset=cast(str, raw["asset"]),
        observed_qualification=cast(str, raw["observed_qualification"]),
        control_mode=cast(str, raw["control_mode"]),
        parameters=tuple(
            _parameter_match(parameter)
            for parameter in cast(list[dict[str, Any]], raw.get("parameters", []))
        ),
        adapter=_adapter(cast(dict[str, Any], adapter)) if adapter is not None else None,
        rejected_candidates=tuple(
            _rejected_offering(candidate)
            for candidate in cast(list[dict[str, Any]], raw.get("rejected_candidates", []))
        ),
    )


def _selected_task(raw: dict[str, Any]) -> ProcedureTaskSelection:
    return ProcedureTaskSelection(
        task=cast(str, raw["task"]),
        materials=tuple(
            _material(material) for material in cast(list[dict[str, Any]], raw.get("materials", []))
        ),
        requirements=tuple(
            _requirement(requirement)
            for requirement in cast(list[dict[str, Any]], raw["requirements"])
        ),
    )


def _solution(raw: dict[str, Any]) -> FacilitySolution:
    return FacilitySolution(
        schema_version=cast(str, raw["schema_version"]),
        problem_sha256=cast(str, raw["problem_sha256"]),
        inventory_sha256=cast(str, raw["inventory_sha256"]),
        facility=cast(str, raw["facility"]),
        policy=cast(dict[str, Any], raw["policy"]),
        selections=tuple(
            MethodSelection(
                choice=cast(str, method["choice"]),
                source_operation=cast(str, method["source_operation"]),
                method=cast(str, method["method"]),
                tasks=tuple(
                    _selected_task(task) for task in cast(list[dict[str, Any]], method["tasks"])
                ),
            )
            for method in cast(list[dict[str, Any]], raw["selections"])
        ),
    )


def _port(raw: dict[str, Any]) -> Port:
    return Port(
        kind=cast(str, raw["kind"]),
        state=cast(str | None, raw.get("state")),
        data_kind=cast(str | None, raw.get("data_kind")),
    )


def _value_source(raw: dict[str, Any]) -> ProcedureValueSource:
    kind = raw["kind"]
    if kind == "choice_input":
        return ChoiceInputValueSource(input=cast(str, raw["input"]))
    if kind == "choice_output":
        return ChoiceOutputValueSource(
            choice=cast(str, raw["choice"]), output=cast(str, raw["output"])
        )
    if kind == "task_output":
        return TaskOutputValueSource(task=cast(str, raw["task"]), output=cast(str, raw["output"]))
    raise ValueError(f"unknown Procedure value source {kind!r}")


def _allocated_requirement(raw: dict[str, Any]) -> AllocatedRequirement:
    adapter = raw.get("adapter")
    return AllocatedRequirement(
        id=cast(str, raw["id"]),
        capability_kind=cast(str, raw["capability_kind"]),
        minimum_qualification=cast(str, raw["minimum_qualification"]),
        accepted_control_modes=tuple(cast(list[str], raw["accepted_control_modes"])),
        offering=cast(str, raw["offering"]),
        asset=cast(str, raw["asset"]),
        observed_qualification=cast(str, raw["observed_qualification"]),
        control_mode=cast(str, raw["control_mode"]),
        parameters=tuple(
            _parameter_match(parameter)
            for parameter in cast(list[dict[str, Any]], raw.get("parameters", []))
        ),
        procedure_implementation=cast(str | None, raw.get("procedure_implementation")),
        adapter=_adapter(cast(dict[str, Any], adapter)) if adapter is not None else None,
    )


def _allocated_task(raw: dict[str, Any]) -> AllocatedProcedureTask:
    program = raw.get("program")
    return AllocatedProcedureTask(
        id=cast(str, raw["id"]),
        operation=cast(str, raw["operation"]),
        program=(parse_program(cast(dict[str, Any], program)) if program is not None else None),
        inputs=tuple(
            ProcedureTaskInput(
                source=_value_source(cast(dict[str, Any], item["source"])),
                port_type=_port(cast(dict[str, Any], item["port_type"])),
            )
            for item in cast(list[dict[str, Any]], raw.get("inputs", []))
        ),
        outputs=tuple(
            ProcedureTaskOutput(
                name=cast(str, item["name"]),
                port_type=_port(cast(dict[str, Any], item["port_type"])),
            )
            for item in cast(list[dict[str, Any]], raw.get("outputs", []))
        ),
        parameters=tuple(
            AllocatedProcedureParameter(
                id=cast(str, parameter["id"]),
                property_kind=cast(str, parameter["property_kind"]),
                value=_procedure_value(cast(dict[str, Any], parameter["value"])),
            )
            for parameter in cast(list[dict[str, Any]], raw.get("parameters", []))
        ),
        materials=tuple(
            _material(material) for material in cast(list[dict[str, Any]], raw.get("materials", []))
        ),
        requirements=tuple(
            _allocated_requirement(requirement)
            for requirement in cast(list[dict[str, Any]], raw["requirements"])
        ),
    )


def _material_candidates(raw: dict[str, Any]) -> MaterialCandidates:
    status = cast(str, raw["status"])
    if status == "unidentified":
        return MaterialCandidates(status=status, component=None, material_lots=())
    if status == "identified":
        return MaterialCandidates(
            status=status,
            component=cast(str, raw["component"]),
            material_lots=tuple(cast(list[str], raw["material_lots"])),
        )
    raise ValueError(f"unknown material candidate status {status!r}")


def _material_inventory(raw: dict[str, Any]) -> MaterialInventory:
    return MaterialInventory(
        source_sha256=cast(str, raw["source_sha256"]),
        facility=cast(str, raw["facility"]),
        materials=MappingProxyType(
            {
                symbol: _material_candidates(cast(dict[str, Any], candidates))
                for symbol, candidates in cast(dict[str, Any], raw["materials"]).items()
            }
        ),
        artifacts=MappingProxyType(
            {
                symbol: _material_candidates(cast(dict[str, Any], candidates))
                for symbol, candidates in cast(dict[str, Any], raw["artifacts"]).items()
            }
        ),
    )


def _invocation(raw: dict[str, Any]) -> AdapterInvocation:
    return AdapterInvocation(
        id=cast(str, raw["id"]),
        asset=cast(str, raw["asset"]),
        adapter=_adapter(cast(dict[str, Any], raw["adapter"])),
        tasks=tuple(cast(list[str], raw["tasks"])),
        requirements=tuple(cast(list[str], raw["requirements"])),
    )


def _adapter_invocation_plan(raw: dict[str, Any]) -> AdapterInvocationPlan:
    return AdapterInvocationPlan(
        schema_version=cast(str, raw["schema_version"]),
        problem_sha256=cast(str, raw["problem_sha256"]),
        allocated_lair_sha256=cast(str, raw["allocated_lair_sha256"]),
        inventory_sha256=cast(str, raw["inventory_sha256"]),
        facility=cast(str, raw["facility"]),
        material_inventory=_material_inventory(cast(dict[str, Any], raw["material_inventory"])),
        methods=tuple(
            AllocatedMethod(
                choice=cast(str, method["choice"]),
                source_operation=cast(str, method["source_operation"]),
                method=cast(str, method["method"]),
                tasks=tuple(
                    _allocated_task(task) for task in cast(list[dict[str, Any]], method["tasks"])
                ),
            )
            for method in cast(list[dict[str, Any]], raw["methods"])
        ),
        invocations=tuple(
            _invocation(invocation)
            for invocation in cast(list[dict[str, Any]], raw.get("invocations", []))
        ),
    )


def _facility_plan(serialized: str) -> FacilityPlan:
    raw = cast(dict[str, Any], json.loads(serialized))
    inventory = cast(dict[str, Any], raw["inventory"])
    invocation_plan = cast(dict[str, Any], raw["adapter_invocations"])
    bindings = raw.get("adapter_bindings")
    return FacilityPlan(
        schema_version=cast(str, raw["schema_version"]),
        package=cast(str, raw["package"]),
        version=cast(str, raw["version"]),
        inventory=InventorySelection(
            document=Path(cast(str, inventory["document"])),
            sha256=cast(str, inventory["sha256"]),
            facility=cast(str, inventory["facility"]),
        ),
        refined_lair=cast(str, raw["refined_lair"]),
        planning_problem=cast(dict[str, Any], raw["planning_problem"]),
        solution=_solution(cast(dict[str, Any], raw["facility_solution"])),
        allocated_lair=cast(str, raw["allocated_lair"]),
        adapter_bindings=cast(dict[str, Any], bindings) if bindings is not None else None,
        adapter_invocations=_adapter_invocation_plan(invocation_plan),
        raw_invocation_plan=invocation_plan,
    )


def plan_project(
    path: str | Path,
    *,
    methods: tuple[Method, ...] = (),
    include_standard: bool = True,
) -> FacilityPlan:
    """Compile and plan the default runnable package at ``path``."""

    catalog = MethodCatalog(methods=methods, include_standard=include_standard)
    return _facility_plan(_plan_lab_project(str(path), catalog.to_json(), include_standard))


def plan(
    program: Program,
    *,
    project: str | Path,
    methods: tuple[Method, ...] = (),
    include_standard: bool = True,
) -> FacilityPlan:
    """Plan a checked Python program using a Lab package's inventory and adapter context."""

    catalog = MethodCatalog(methods=methods, include_standard=include_standard)
    return _facility_plan(
        _plan_lab_modules(
            list(program.sources.items()), str(project), catalog.to_json(), include_standard
        )
    )


__all__ = [
    "AdapterInvocation",
    "AdapterInvocationPlan",
    "AdapterSelection",
    "AllocatedMethod",
    "AllocatedProcedureParameter",
    "AllocatedProcedureTask",
    "AllocatedRequirement",
    "CapabilityParameterMatch",
    "ChoiceInputValueSource",
    "ChoiceOutputSource",
    "ChoiceOutputValueSource",
    "FacilityPlan",
    "FacilitySolution",
    "InventorySelection",
    "MaterialBinding",
    "MaterialCandidates",
    "MaterialInventory",
    "MaterialLotSource",
    "MethodSelection",
    "ProcedureTaskInput",
    "ProcedureTaskOutput",
    "ProcedureTaskSelection",
    "ProcedureValueSource",
    "RejectedOffering",
    "RequirementBinding",
    "SelectedMaterialSource",
    "TaskOutputValueSource",
    "plan",
    "plan_project",
]
