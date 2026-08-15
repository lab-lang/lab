from pathlib import Path

import pytest

from lab_isaac import ContractError, load_prototype

FIXTURES = Path(__file__).parent / "fixtures"
TASK = FIXTURES / "robot-tasks" / "task.json"
BINDING = Path(__file__).parent.parent / "examples" / "golden-gate-plate-transfer.binding.toml"


def test_golden_gate_transfer_contract_resolves() -> None:
    prototype = load_prototype(TASK, BINDING)

    assert prototype.task.object_name == "reaction_plate"
    assert prototype.task.source_station == "star-1"
    assert prototype.task.destination_station == "odtc-1"
    assert prototype.task.plan_path.name == "plan.workcell.json"
    assert prototype.binding.robot_model == "franka-panda"
    assert prototype.binding.object.size_m == (0.076, 0.050, 0.0144)
    assert prototype.summary()["calibration"] == "prototype-proxy"


def test_binding_and_task_station_must_agree(tmp_path: Path) -> None:
    text = BINDING.read_text().replace('station = "odtc-1"', 'station = "reader-1"', 1)
    binding = tmp_path / "mismatch.toml"
    binding.write_text(text)

    with pytest.raises(ContractError, match="destination station 'reader-1'"):
        load_prototype(TASK, binding)


def test_task_nodes_must_exist_in_the_semantic_scene(tmp_path: Path) -> None:
    tasks = tmp_path / "robot-tasks"
    tasks.mkdir()
    task = tasks / "task.json"
    plan = tmp_path / "plan.workcell.json"
    scene = tmp_path / "scene.json"
    task.write_text(TASK.read_text())
    plan.write_text((FIXTURES / "plan.workcell.json").read_text())
    scene.write_text((FIXTURES / "scene.json").read_text().replace('"id": "odtc-1"', '"id": "x"'))

    with pytest.raises(ContractError, match="no node 'odtc-1'"):
        load_prototype(task, BINDING)


def test_projected_task_must_still_match_its_source_plan(tmp_path: Path) -> None:
    tasks = tmp_path / "robot-tasks"
    tasks.mkdir()
    task = tasks / "task.json"
    task.write_text(TASK.read_text().replace("Seal and transfer", "Discard"))
    (tmp_path / "plan.workcell.json").write_text((FIXTURES / "plan.workcell.json").read_text())
    (tmp_path / "scene.json").write_text((FIXTURES / "scene.json").read_text())

    with pytest.raises(ContractError, match="does not match its source workcell node"):
        load_prototype(task, BINDING)
