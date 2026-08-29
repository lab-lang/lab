"""Typed access to Lab's shared facility-planning pipeline.

Python-authored and file-backed Lab programs use the same Rust compiler service. A plan is produced
only after Method refinement, exact SBOLInventory MaterialLot and capability allocation, allocated
Procedure LAIR verification, and adapter-invocation projection all succeed.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from ._native import plan_lab_modules as _plan_lab_modules
from ._native import plan_lab_project as _plan_lab_project
from ._program import Program
from .methods import Method, MethodCatalog


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
class AdapterSelection:
    """The exact implementation and profile frozen for one Asset binding."""

    driver: str
    profile_path: Path
    profile_sha256: str
    features: tuple[str, ...]
    accepted_run_formats: tuple[str, ...]
    emitted_run_formats: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class RequirementBinding:
    """One semantic requirement bound through an offering to an exact Asset."""

    requirement: str
    capability_kind: str
    minimum_qualification: str
    accepted_control_modes: tuple[str, ...]
    offering: str
    asset: str
    observed_qualification: str
    control_mode: str
    parameters: tuple[dict[str, Any], ...]
    adapter: AdapterSelection | None
    rejected_candidates: tuple[dict[str, Any], ...]


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
class AdapterInvocation:
    """The exact subset of allocated Procedure work delivered to one adapter."""

    id: str
    asset: str
    adapter: AdapterSelection
    tasks: tuple[str, ...]
    requirements: tuple[str, ...]


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
    invocation_plan: dict[str, Any]
    invocations: tuple[AdapterInvocation, ...]


def _adapter(raw: dict[str, Any]) -> AdapterSelection:
    return AdapterSelection(
        driver=cast(str, raw["driver"]),
        profile_path=Path(cast(str, raw["profile_path"])),
        profile_sha256=cast(str, raw["profile_sha256"]),
        features=tuple(cast(list[str], raw.get("features", []))),
        accepted_run_formats=tuple(cast(list[str], raw.get("accepted_run_formats", []))),
        emitted_run_formats=tuple(cast(list[str], raw.get("emitted_run_formats", []))),
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
        parameters=tuple(cast(list[dict[str, Any]], raw.get("parameters", []))),
        adapter=_adapter(cast(dict[str, Any], adapter)) if adapter is not None else None,
        rejected_candidates=tuple(cast(list[dict[str, Any]], raw.get("rejected_candidates", []))),
    )


def _task(raw: dict[str, Any]) -> ProcedureTaskSelection:
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
                tasks=tuple(_task(task) for task in cast(list[dict[str, Any]], method["tasks"])),
            )
            for method in cast(list[dict[str, Any]], raw["selections"])
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
        invocation_plan=invocation_plan,
        invocations=tuple(
            AdapterInvocation(
                id=cast(str, invocation["id"]),
                asset=cast(str, invocation["asset"]),
                adapter=_adapter(cast(dict[str, Any], invocation["adapter"])),
                tasks=tuple(cast(list[str], invocation["tasks"])),
                requirements=tuple(cast(list[str], invocation["requirements"])),
            )
            for invocation in cast(list[dict[str, Any]], invocation_plan["invocations"])
        ),
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
    "AdapterSelection",
    "ChoiceOutputSource",
    "FacilityPlan",
    "FacilitySolution",
    "InventorySelection",
    "MaterialBinding",
    "MaterialLotSource",
    "MethodSelection",
    "ProcedureTaskSelection",
    "RequirementBinding",
    "SelectedMaterialSource",
    "plan",
    "plan_project",
]
