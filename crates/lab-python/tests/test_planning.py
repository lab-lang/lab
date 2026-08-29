"""Python consumes the same exact facility-planning service as the Lab CLI."""

import shutil
from pathlib import Path

import lab
import pytest
from lab import methods as method_types
from lab import planning

REPOSITORY = Path(__file__).resolve().parents[3]
GOLDEN_GATE = REPOSITORY / "examples" / "golden-gate"
OT2 = "https://example.org/golden-gate/opentrons_ot2"
CAPABILITY = "https://sbol.io/ns/capability#"
PROCEDURE = "https://www.lab-compiler.org/ns/procedure#"
MATERIAL_STATE = "https://www.lab-compiler.org/ns/material-state#"
CUSTOM_RECOVERY = "https://example.org/method/custom-recovery"


def custom_recovery() -> method_types.Method:
    return method_types.Method(
        id=CUSTOM_RECOVERY,
        refines="std.lab.plasmid.recover",
        inputs=(
            method_types.MethodInput(
                "culture", method_types.Port.material(f"{MATERIAL_STATE}TransformedCulture")
            ),
        ),
        parameters=(method_types.MethodParameter.scalar("duration", method_types.ScalarType.REAL),),
        tasks=(
            method_types.Task(
                id="recover",
                operation=f"{PROCEDURE}RecoverCulture",
                inputs=(method_types.ValueReference.method_input("culture"),),
                outputs=(
                    method_types.TaskOutput(
                        "recovered", method_types.Port.material(f"{MATERIAL_STATE}RecoveredCulture")
                    ),
                ),
                parameters=(
                    method_types.ProcedureParameter(
                        "duration",
                        f"{CAPABILITY}Duration",
                        method_types.ProcedureValueExpression.intent_parameter("duration"),
                    ),
                ),
                materials=(
                    method_types.MaterialInput(
                        "medium", method_types.MaterialSource.constant("recovery_medium")
                    ),
                ),
                requirements=(
                    method_types.Requirement(
                        id="incubation",
                        capability_kind=f"{CAPABILITY}Incubation",
                        accepted_control_modes=(method_types.ControlMode.MANUAL,),
                        constraints=(
                            method_types.CapabilityConstraint(
                                property_kind=f"{CAPABILITY}Duration",
                                relation=method_types.ConstraintRelation.EXACT,
                                required=method_types.ValueExpression.intent_parameter("duration"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        outputs=(
            method_types.MethodOutput(
                "recovered", method_types.ValueReference.task_output("recover", "recovered")
            ),
        ),
    )


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


def test_file_backed_planning_composes_package_authored_method_documents(tmp_path: Path) -> None:
    project = tmp_path / "golden-gate"
    shutil.copytree(GOLDEN_GATE, project)
    with (project / "lab.toml").open("a", encoding="utf-8") as manifest:
        manifest.write(
            f'\n[methods]\ndocuments = ["methods/custom.json"]\n\n'
            f'[[planning.methods]]\nsource-operation = "std.lab.plasmid.recover"\n'
            f'method = "{CUSTOM_RECOVERY}"\n'
        )
    (project / "methods").mkdir()
    method_types.MethodCatalog((custom_recovery(),)).write(project / "methods/custom.json")

    planned = lab.plan_project(project)

    assert any(selection.method == CUSTOM_RECOVERY for selection in planned.solution.selections)
