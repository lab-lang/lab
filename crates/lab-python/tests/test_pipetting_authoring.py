"""Python authoring preserves the shared Rust pipetting contract and contribution fixture."""

import json
import runpy
import shutil
import subprocess
import sys
from collections.abc import Callable
from dataclasses import replace
from decimal import Decimal
from pathlib import Path
from typing import cast

import lab
import pytest
from lab import methods as m
from lab import procedures as views
from lab.procedures import pipetting as p

ROOT = Path(__file__).resolve().parents[3]
EXAMPLE = ROOT / "examples/contributing/scientific-package"
AUTHOR = EXAMPLE / "methods/homogenize.py"


def method() -> m.Method:
    author = runpy.run_path(str(AUTHOR))
    return cast(Callable[[], m.Method], author["homogenize_method"])()


def refine(project: Path) -> m.RefinedProgram:
    source = (project / "src/main.lab").read_text()
    program = lab.check_sources({"contribution_consumer": source}, project=project)
    return lab.refine(program, entry_module="contribution_consumer", project=project)


def test_python_author_matches_existing_json_and_rust_refinement(tmp_path: Path) -> None:
    catalog = m.MethodCatalog((method(),))
    project = tmp_path / "package"
    shutil.copytree(EXAMPLE, project)
    baseline = refine(project)
    output = catalog.write(project / "methods/homogenize.json")
    assert json.loads(output.read_text()) == json.loads(
        (EXAMPLE / "methods/homogenize.json").read_text()
    )
    assert refine(project).planning_problem == baseline.planning_problem


def test_author_script_runs_and_is_strictly_typed(tmp_path: Path) -> None:
    script = tmp_path / AUTHOR.name
    shutil.copyfile(AUTHOR, script)
    result = subprocess.run(
        [sys.executable, str(script)], capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert json.loads(script.with_suffix(".json").read_text()) == json.loads(
        (EXAMPLE / "methods/homogenize.json").read_text()
    )
    checked = subprocess.run(
        [sys.executable, "-m", "mypy", "--strict", str(script)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert checked.returncode == 0, checked.stdout + checked.stderr


@pytest.mark.parametrize(
    ("parameter", "value", "unit", "message"),
    [
        ("mix_volume", m.Scalar.real("10"), views.SECOND, "unit"),
        ("mix_volume", m.Scalar.real("31"), views.MICROLITRE, "contains only 30 uL"),
        ("mix_cycles", m.Scalar.real("3"), None, "integer"),
        ("mix_cycles", m.Scalar.integer(0), None, "cycles"),
    ],
)
def test_rust_rejects_invalid_resolved_parameters(
    tmp_path: Path,
    parameter: str,
    value: m.Scalar,
    unit: str | None,
    message: str,
) -> None:
    authored = method()
    task = authored.tasks[0]
    parameters = tuple(
        replace(
            item,
            value=m.ProcedureValueExpression.constant(
                m.ProcedureValue.scalar(m.PropertyValue(value, unit)),
            ),
        )
        if item.id == parameter
        else item
        for item in task.parameters
    )
    authored = replace(authored, tasks=(replace(task, parameters=parameters),))
    project = tmp_path / "package"
    shutil.copytree(EXAMPLE, project)
    m.MethodCatalog((authored,)).write(project / "methods/homogenize.json")
    with pytest.raises(ValueError, match=message):
        refine(project)


def test_rust_rejects_an_undeclared_parameter_at_catalog_validation() -> None:
    authored = method()
    task = authored.tasks[0]
    authored = replace(authored, tasks=(replace(task, parameters=task.parameters[:-1]),))
    with pytest.raises(ValueError, match=r"mix_cycles.*not declared"):
        m.MethodCatalog((authored,)).validate()


def test_positions_and_owner_prevent_accidentally_using_another_template() -> None:
    program = p.Template()
    sample = program.input("sample", port=0, initial_volume=p.microlitres("30"))
    foreign = p.Template().product("sample", output="prepared")
    with pytest.raises(ValueError, match="requested 1"):
        sample.position(1)
    with pytest.raises(TypeError, match="integer"):
        sample.position(True)
    with (
        pytest.raises(ValueError, match="another template"),
        program.path("sample-path", policy=p.ISOLATED_DESTINATIONS) as path,
    ):
        path.transfer("move", sample.position(0), foreign.position(0), volume=p.microlitres(10))
    assert program.to_template()["steps"] == []


def test_path_lifetime_contiguity_and_failure_are_explicit() -> None:
    program = p.Template()
    sample = program.input("sample", port=0, initial_volume=p.microlitres(30))
    with program.path("sample-path", policy=p.ISOLATED_DESTINATIONS) as path:
        path.mix("mix", sample.position(0), volume=p.microlitres(10), cycles=3)
        with pytest.raises(ValueError, match="active fluid path"):
            program.to_template()
        with pytest.raises(ValueError, match="active fluid path"):
            program.barrier("pause", reason="Separate operations")
        with (
            pytest.raises(ValueError, match="active fluid path"),
            program.path("nested", policy=p.ISOLATED_DESTINATIONS),
        ):
            pass
    with pytest.raises(ValueError, match="active with block"):
        path.mix("escaped", sample.position(0), volume=p.microlitres(10), cycles=3)
    with (
        pytest.raises(ValueError, match="already been used"),
        program.path("sample-path", policy=p.ISOLATED_DESTINATIONS),
    ):
        pass
    with (
        pytest.raises(ValueError, match="duplicate step"),
        program.path("failed", policy=p.ISOLATED_DESTINATIONS) as failed,
    ):
        failed.mix("uncommitted", sample.position(0), volume=p.microlitres(10), cycles=3)
        failed.mix("mix", sample.position(0), volume=p.microlitres(10), cycles=3)
    snapshot = program.to_template()
    steps = cast(list[dict[str, object]], snapshot["steps"])
    assert [step["id"] for step in steps] == ["mix"]
    steps.clear()
    assert program.to_template() != snapshot


@pytest.mark.parametrize("value", ["0", "-1", "NaN", "Infinity"])
def test_invalid_literal_volumes_fail_at_authoring(value: str) -> None:
    with pytest.raises(ValueError, match="finite and positive"):
        p.microlitres(value)


def test_exact_literals_and_declaration_errors() -> None:
    assert p.microlitres("0.100000000000000001").value == Decimal("0.100000000000000001")
    program = p.Template()
    with pytest.raises(ValueError, match="positions"):
        program.product("product", output="prepared", positions=0)
    program.product("product", output="prepared")
    with pytest.raises(ValueError, match="duplicate vessel"):
        program.product("product", output="prepared")
    with pytest.raises(ValueError, match="whitespace"):
        p.volume_parameter("mix volume")


def test_distribution_constraints_and_techniques_refine_through_rust(tmp_path: Path) -> None:
    program = p.Template()
    sample = program.vessel(
        "sample",
        role=views.ProcedureInputVesselRole(0),
        initial_volume=p.microlitres(30),
        working_capacity=p.microlitres(50),
        dead_volume=p.microlitres(5),
        temperature=views.TemperatureRange(
            views.Temperature(Decimal(4)),
            views.Temperature(Decimal(8)),
        ),
    )
    product = program.product("prepared", output="prepared", positions=2)
    program.distribute(
        "distribute",
        sample.position(0),
        product.positions(),
        volume_each=p.microlitres(10),
        policy=p.SHARED_SOURCE_NO_REENTRY,
        technique=views.TransferTechnique(
            aspiration=views.TrackedLiquidSurfaceAspiration(),
            dispense=views.VesselTopDispense(views.Length(Decimal(-1))),
            air_gap=p.microlitres(2),
            blow_out=True,
            touch_tip=True,
        ),
    )
    program.barrier("prepare-mix", reason="Start an isolated path for each product")
    for index, location in enumerate(product.positions()):
        with program.path(f"product-{index}", policy=p.ISOLATED_DESTINATIONS) as path:
            path.mix(
                f"mix-{index}",
                location,
                cycles=3,
                volume=p.microlitres(5),
                technique=views.MixTechnique(
                    aspiration=views.VesselBottomAspiration(views.Length(Decimal(1))),
                    dispense=views.AboveLiquidDispense(),
                ),
            )
    authored = method()
    task = authored.tasks[0]
    assert isinstance(task.execution, m.TemplateExecution)
    authored = replace(
        authored,
        tasks=(
            replace(
                task,
                execution=replace(task.execution, body=program.to_template()),
            ),
        ),
    )
    project = tmp_path / "package"
    shutil.copytree(EXAMPLE, project)
    m.MethodCatalog((authored,)).write(project / "methods/homogenize.json")
    result = refine(project)
    assert "tracked_liquid_surface" in json.dumps(result.planning_problem)
    assert "vessel_top" in json.dumps(result.planning_problem)


def test_static_types_reject_parameter_category_errors(tmp_path: Path) -> None:
    source = tmp_path / "invalid.py"
    source.write_text(
        "from lab.procedures import pipetting as p\n"
        "program = p.Template()\n"
        "program.input('sample', port=0, initial_volume=p.integer_parameter('cycles'))\n"
    )
    result = subprocess.run(
        [sys.executable, "-m", "mypy", "--strict", str(source)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1, result.stdout + result.stderr
    assert 'incompatible type "IntegerParameter"' in result.stdout


@pytest.mark.parametrize("material_product", [False, True])
def test_material_slots_and_remaining_vessel_roles_refine(
    tmp_path: Path,
    material_product: bool,
) -> None:
    program = p.Template()
    sample = program.vessel(
        "sample",
        role=views.InputOutputVesselRole(0, "prepared"),
        initial_volume=p.microlitres(30),
    )
    water = program.source("water", material="water", initial_volume=p.microlitres(20))
    intermediate = program.vessel("intermediate", role=views.IntermediateVesselRole())
    if material_product:
        # One material-backed product uses the same task output contract as an input/output vessel.
        program = p.Template()
        sample = program.vessel(
            "sample",
            role=views.MaterialProductVesselRole("sample", "prepared"),
            initial_volume=p.microlitres(30),
        )
        water = program.source("water", material="water", initial_volume=p.microlitres(20))
        intermediate = program.vessel("intermediate", role=views.IntermediateVesselRole())
    with program.path("water-path", policy=p.ISOLATED_DESTINATIONS) as path:
        path.transfer(
            "add-water", water.position(0), intermediate.position(0), volume=p.microlitres(5)
        )
    with program.path("sample-path", policy=p.ISOLATED_DESTINATIONS) as path:
        path.transfer(
            "dilute", intermediate.position(0), sample.position(0), volume=p.microlitres(5)
        )
        path.mix("mix", sample.position(0), volume=p.microlitres(10), cycles=3)

    authored = method()
    task = authored.tasks[0]
    assert isinstance(task.execution, m.TemplateExecution)
    materials: tuple[m.MaterialInput, ...] = (
        m.MaterialInput("water", m.MaterialSource.constant("water")),
    )
    if material_product:
        materials += (m.MaterialInput("sample", m.MaterialSource.constant("sample")),)
    authored = replace(
        authored,
        tasks=(
            replace(
                task,
                materials=materials,
                execution=replace(task.execution, body=program.to_template()),
            ),
        ),
    )
    project = tmp_path / "package"
    shutil.copytree(EXAMPLE, project)
    m.MethodCatalog((authored,)).write(project / "methods/homogenize.json")
    result = refine(project)
    rendered = json.dumps(result.planning_problem)
    assert "::material::water" in rendered
    assert '"$lab"' not in rendered


@pytest.mark.parametrize("reference", ["input", "output", "material"])
def test_rust_checks_task_references_when_loading_the_catalog(reference: str) -> None:
    program = p.Template()
    if reference == "input":
        program.input("sample", port=1, initial_volume=p.microlitres(30))
    elif reference == "output":
        program.product("product", output="missing")
    else:
        program.source("water", material="missing")
    authored = method()
    task = authored.tasks[0]
    assert isinstance(task.execution, m.TemplateExecution)
    authored = replace(
        authored,
        tasks=(
            replace(
                task,
                execution=replace(task.execution, body=program.to_template()),
            ),
        ),
    )
    with pytest.raises(ValueError, match=r"out of bounds|not declared"):
        m.MethodCatalog((authored,)).validate()
