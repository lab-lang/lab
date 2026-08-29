"""Python consumes the same exact facility-planning service as the Lab CLI."""

from pathlib import Path

import lab
import pytest
from lab import methods as method_types
from lab import planning

REPOSITORY = Path(__file__).resolve().parents[3]
GOLDEN_GATE = REPOSITORY / "examples" / "golden-gate"
OT2 = "https://example.org/golden-gate/opentrons_ot2"


def test_a_file_backed_project_returns_typed_facility_decisions() -> None:
    planned = lab.plan_project(GOLDEN_GATE)

    assert isinstance(planned, planning.FacilityPlan)
    assert planned.schema_version == "lab.python-facility-plan.v1"
    assert planned.inventory.facility == "https://example.org/golden-gate/facility"
    assert planned.solution.problem_sha256 == planned.invocation_plan["problem_sha256"]
    assert planned.adapter_invocations.schema_version == "lab.adapter-invocations.v5"
    assert any(invocation.asset == OT2 for invocation in planned.invocations)
    assert any(
        isinstance(material.source, planning.MaterialLotSource)
        for method in planned.solution.selections
        for task in method.tasks
        for material in task.materials
    )
    assert "allocated-procedure" in planned.allocated_lair
    assert planned.material_inventory.facility == planned.inventory.facility
    assert planned.material_inventory.materials["BsaI"].identified
    assert planned.material_inventory.materials["BsaI"].material_lots == (
        "https://example.org/golden-gate/lots/BsaI_lot",
    )

    invocation = next(item for item in planned.invocations if item.asset == OT2)
    tasks = planned.invocation_tasks(invocation)
    assert tuple(task.id for task in tasks) == invocation.tasks
    assert all(task.operation.startswith("https://") for task in tasks)
    assert all(
        requirement.id in invocation.requirements
        for task in tasks
        for requirement in task.requirements
        if requirement.adapter == invocation.adapter
    )
    setup = next(task for task in tasks if task.operation.endswith("#SetupGoldenGateReaction"))
    assert isinstance(setup.inputs[0].port_type, method_types.Port)
    assert setup.inputs[0].port_type.kind == "design"
    reaction_volume = next(
        parameter
        for parameter in setup.parameters
        if parameter.property_kind.endswith("#ReactionVolumeUl")
    )
    assert reaction_volume.value.value is not None
    assert reaction_volume.value.value.value.type is method_types.ScalarType.INTEGER
    assert reaction_volume.value.value.unit == "http://qudt.org/vocab/unit/MicroL"

    with pytest.raises(KeyError):
        planned.task("not-a-task")


def test_a_python_program_uses_the_packages_inventory_and_adapter_context() -> None:
    sources = {
        module: (GOLDEN_GATE / relative).read_text()
        for module, relative in (
            ("golden_gate.designs.inventory", "src/designs/inventory.lab"),
            ("golden_gate.designs.plasmids", "src/designs/plasmids.lab"),
            ("golden_gate.designs.strains", "src/designs/strains.lab"),
            ("golden_gate.workflows.assemble", "src/workflows/assemble.lab"),
            ("golden_gate.workflows.build_strains", "src/workflows/build_strains.lab"),
            ("golden_gate.programs.reporter_panel", "src/programs/reporter_panel.lab"),
        )
    }
    program = lab.check_sources(sources)

    planned = lab.plan(program, project=GOLDEN_GATE)

    assert planned.package == "golden-gate"
    assert planned.solution.facility == planned.inventory.facility
    assert any(invocation.adapter.driver == "opentrons.ot2" for invocation in planned.invocations)
    assert all(
        requirement.asset == OT2
        for method in planned.solution.selections
        for task in method.tasks
        for requirement in task.requirements
        if requirement.adapter is not None
    )
