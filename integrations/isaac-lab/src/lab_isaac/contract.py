"""Checked boundary between Lab workflow intent and an Isaac physics task."""

from __future__ import annotations

import json
import math
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import cast

ROBOT_TASK_FORMAT = "lab.robot-task.v0"
SCENE_FORMAT = "lab.scene.v0"
WORKCELL_FORMAT = "lab.workcell-run.v0"
ISAAC_BINDING_FORMAT = "lab.isaac-binding.v0"

JsonObject = dict[str, object]
Vector3 = tuple[float, float, float]
Quaternion = tuple[float, float, float, float]


class ContractError(ValueError):
    """The semantic task and simulator binding do not agree."""


@dataclass(frozen=True)
class RobotTask:
    path: Path
    task_id: str
    object_name: str
    object_node: str
    source_station: str
    source_node: str
    destination_station: str
    destination_node: str
    plan_path: Path
    scene_path: Path


@dataclass(frozen=True)
class Pose:
    station: str
    position_m: Vector3
    quaternion_wxyz: Quaternion
    position_jitter_m: Vector3


@dataclass(frozen=True)
class ObjectPhysics:
    shape: str
    size_m: Vector3
    mass_kg: float
    static_friction: float
    dynamic_friction: float


@dataclass(frozen=True)
class Goal:
    position_tolerance_m: float
    orientation_tolerance_rad: float
    max_linear_velocity_mps: float
    max_angular_velocity_radps: float
    minimum_gripper_open_m: float


@dataclass(frozen=True)
class Simulation:
    dt_seconds: float
    decimation: int
    episode_length_seconds: float
    num_envs: int
    env_spacing_m: float


@dataclass(frozen=True)
class IsaacBinding:
    path: Path
    task_id: str
    calibration: str
    provenance: str
    robot_model: str
    controller: str
    object: ObjectPhysics
    source: Pose
    destination: Pose
    goal: Goal
    simulation: Simulation


@dataclass(frozen=True)
class Prototype:
    task: RobotTask
    binding: IsaacBinding

    def summary(self) -> JsonObject:
        """Return stable JSON-ready resolved configuration."""
        return cast(
            JsonObject,
            {
                "task": self.task.task_id,
                "object": self.task.object_name,
                "source": self.task.source_station,
                "destination": self.task.destination_station,
                "plan": str(self.task.plan_path),
                "scene": str(self.task.scene_path),
                "calibration": self.binding.calibration,
                "provenance": self.binding.provenance,
                "robot": {
                    "model": self.binding.robot_model,
                    "controller": self.binding.controller,
                },
                "object_physics": asdict(self.binding.object),
                "source_pose": asdict(self.binding.source),
                "destination_pose": asdict(self.binding.destination),
                "goal": asdict(self.binding.goal),
                "simulation": asdict(self.binding.simulation),
            },
        )


def _mapping(value: object, context: str) -> JsonObject:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise ContractError(f"{context} must be a table/object")
    return cast(JsonObject, value)


def _string(table: JsonObject, key: str, context: str) -> str:
    value = table.get(key)
    if not isinstance(value, str) or not value:
        raise ContractError(f"{context}.{key} must be a non-empty string")
    return value


def _number(table: JsonObject, key: str, context: str, *, positive: bool = True) -> float:
    value = table.get(key)
    if isinstance(value, bool) or not isinstance(value, int | float):
        raise ContractError(f"{context}.{key} must be a number")
    result = float(value)
    if not math.isfinite(result) or (positive and result <= 0.0):
        qualifier = "positive and finite" if positive else "finite"
        raise ContractError(f"{context}.{key} must be {qualifier}")
    return result


def _integer(table: JsonObject, key: str, context: str) -> int:
    value = table.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ContractError(f"{context}.{key} must be a positive integer")
    return value


def _vector(table: JsonObject, key: str, context: str, length: int) -> tuple[float, ...]:
    value = table.get(key)
    if not isinstance(value, list) or len(value) != length:
        raise ContractError(f"{context}.{key} must contain exactly {length} numbers")
    result: list[float] = []
    for component in value:
        if isinstance(component, bool) or not isinstance(component, int | float):
            raise ContractError(f"{context}.{key} must contain only numbers")
        number = float(component)
        if not math.isfinite(number):
            raise ContractError(f"{context}.{key} must contain only finite numbers")
        result.append(number)
    return tuple(result)


def _vector3(table: JsonObject, key: str, context: str) -> Vector3:
    return cast(Vector3, _vector(table, key, context, 3))


def _quaternion(table: JsonObject, key: str, context: str) -> Quaternion:
    result = cast(Quaternion, _vector(table, key, context, 4))
    norm = math.sqrt(sum(component * component for component in result))
    if not math.isclose(norm, 1.0, rel_tol=0.0, abs_tol=1.0e-3):
        raise ContractError(f"{context}.{key} must be a normalized WXYZ quaternion")
    return result


def _string_list(table: JsonObject, key: str, context: str) -> list[str]:
    value = table.get(key)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ContractError(f"{context}.{key} must be a list of strings")
    return cast(list[str], value)


def _read_json(path: Path) -> JsonObject:
    try:
        return _mapping(json.loads(path.read_text()), str(path))
    except OSError as error:
        raise ContractError(f"cannot read {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ContractError(f"{path} is not valid JSON: {error}") from error


def _read_toml(path: Path) -> JsonObject:
    try:
        return _mapping(tomllib.loads(path.read_text()), str(path))
    except OSError as error:
        raise ContractError(f"cannot read {path}: {error}") from error
    except tomllib.TOMLDecodeError as error:
        raise ContractError(f"{path} is not valid TOML: {error}") from error


def _scene_nodes(node: JsonObject, found: dict[str, list[str]]) -> None:
    node_id = _string(node, "id", "scene node")
    semantic = _mapping(node.get("semantic"), f"scene node '{node_id}'.semantic")
    kind = _string(semantic, "kind", f"scene node '{node_id}'.semantic")
    found.setdefault(node_id, []).append(kind)
    children = node.get("children", [])
    if not isinstance(children, list):
        raise ContractError(f"scene node '{node_id}'.children must be a list")
    for child in children:
        _scene_nodes(_mapping(child, f"child of scene node '{node_id}'"), found)


def _require_scene_node(found: dict[str, list[str]], node_id: str, kind: str) -> None:
    kinds = found.get(node_id, [])
    matches = sum(candidate == kind for candidate in kinds)
    if not kinds:
        raise ContractError(f"semantic scene has no node '{node_id}'")
    if matches == 0:
        raise ContractError(f"semantic scene node '{node_id}' is not kind '{kind}'")
    if matches > 1:
        raise ContractError(f"semantic scene has {matches} '{kind}' nodes named '{node_id}'")


def _load_task(path: Path) -> RobotTask:
    document = _read_json(path)
    if _string(document, "format", "robot task") != ROBOT_TASK_FORMAT:
        raise ContractError(f"{path} is not a {ROBOT_TASK_FORMAT} document")
    if _string(document, "action", "robot task") != "transfer":
        raise ContractError("the Isaac prototype currently supports transfer tasks only")

    task_id = _string(document, "id", "robot task")
    object_ref = _mapping(document.get("object"), "robot task.object")
    source = _mapping(document.get("source"), "robot task.source")
    destination = _mapping(document.get("destination"), "robot task.destination")
    completion = _mapping(document.get("completion"), "robot task.completion")
    object_name = _string(object_ref, "labware", "robot task.object")
    source_station = _string(source, "station", "robot task.source")
    destination_station = _string(destination, "station", "robot task.destination")

    plan_ref = Path(_string(document, "plan", "robot task"))
    plan_path = plan_ref if plan_ref.is_absolute() else path.parent / plan_ref
    plan = _read_json(plan_path)
    if _string(plan, "format", "workcell plan") != WORKCELL_FORMAT:
        raise ContractError(f"{plan_path} is not a {WORKCELL_FORMAT} document")
    nodes = plan.get("nodes")
    if not isinstance(nodes, list):
        raise ContractError("workcell plan.nodes must be a list")
    matching_nodes = [
        _mapping(node, "workcell plan node")
        for node in nodes
        if isinstance(node, dict) and node.get("id") == task_id
    ]
    if len(matching_nodes) != 1:
        raise ContractError(
            f"workcell plan must contain exactly one source node '{task_id}'; "
            f"found {len(matching_nodes)}"
        )
    plan_node = matching_nodes[0]
    if _string(plan_node, "action", f"workcell node '{task_id}'") != "handoff":
        raise ContractError(f"workcell node '{task_id}' is not a handoff")
    plan_relation = (
        _string(plan_node, "from", f"workcell node '{task_id}'"),
        _string(plan_node, "to", f"workcell node '{task_id}'"),
        _string(plan_node, "labware", f"workcell node '{task_id}'"),
        _string(plan_node, "instructions", f"workcell node '{task_id}'"),
        _string_list(plan_node, "after", f"workcell node '{task_id}'"),
    )
    task_relation = (
        source_station,
        destination_station,
        object_name,
        _string(document, "instructions", "robot task"),
        _string_list(document, "after", "robot task"),
    )
    if plan_relation != task_relation:
        raise ContractError(f"robot task '{task_id}' does not match its source workcell node")
    if (
        _string(completion, "relation", "robot task.completion") != "object-at-station"
        or _string(completion, "object", "robot task.completion") != object_name
        or _string(completion, "target", "robot task.completion") != destination_station
    ):
        raise ContractError(f"robot task '{task_id}' has an inconsistent completion relation")

    scene_ref = Path(_string(document, "scene", "robot task"))
    scene_path = scene_ref if scene_ref.is_absolute() else path.parent / scene_ref
    scene = _read_json(scene_path)
    if _string(scene, "format", "semantic scene") != SCENE_FORMAT:
        raise ContractError(f"{scene_path} is not a {SCENE_FORMAT} document")
    found: dict[str, list[str]] = {}
    _scene_nodes(_mapping(scene.get("root"), "semantic scene.root"), found)

    object_node = _string(object_ref, "scene_node", "robot task.object")
    source_node = _string(source, "scene_node", "robot task.source")
    destination_node = _string(destination, "scene_node", "robot task.destination")
    _require_scene_node(found, object_node, "labware")
    _require_scene_node(found, source_node, "station")
    _require_scene_node(found, destination_node, "station")
    return RobotTask(
        path=path,
        task_id=task_id,
        object_name=object_name,
        object_node=object_node,
        source_station=source_station,
        source_node=source_node,
        destination_station=destination_station,
        destination_node=destination_node,
        plan_path=plan_path,
        scene_path=scene_path,
    )


def _pose(table: JsonObject, context: str) -> Pose:
    return Pose(
        station=_string(table, "station", context),
        position_m=_vector3(table, "position_m", context),
        quaternion_wxyz=_quaternion(table, "quaternion_wxyz", context),
        position_jitter_m=_vector3(table, "position_jitter_m", context),
    )


def _load_binding(path: Path) -> IsaacBinding:
    document = _read_toml(path)
    if _string(document, "format", "Isaac binding") != ISAAC_BINDING_FORMAT:
        raise ContractError(f"{path} is not a {ISAAC_BINDING_FORMAT} document")
    robot = _mapping(document.get("robot"), "Isaac binding.robot")
    object_table = _mapping(document.get("object"), "Isaac binding.object")
    goal_table = _mapping(document.get("goal"), "Isaac binding.goal")
    simulation = _mapping(document.get("simulation"), "Isaac binding.simulation")

    shape = _string(object_table, "shape", "Isaac binding.object")
    if shape != "cuboid":
        raise ContractError("the prototype currently supports object.shape = 'cuboid' only")
    size = _vector3(object_table, "size_m", "Isaac binding.object")
    if any(component <= 0.0 for component in size):
        raise ContractError("Isaac binding.object.size_m components must be positive")
    robot_model = _string(robot, "model", "Isaac binding.robot")
    controller = _string(robot, "controller", "Isaac binding.robot")
    if (robot_model, controller) != ("franka-panda", "relative-ik"):
        raise ContractError(
            "the prototype currently supports robot model 'franka-panda' "
            "with controller 'relative-ik' only"
        )
    return IsaacBinding(
        path=path,
        task_id=_string(document, "task_id", "Isaac binding"),
        calibration=_string(document, "calibration", "Isaac binding"),
        provenance=_string(document, "provenance", "Isaac binding"),
        robot_model=robot_model,
        controller=controller,
        object=ObjectPhysics(
            shape=shape,
            size_m=size,
            mass_kg=_number(object_table, "mass_kg", "Isaac binding.object"),
            static_friction=_number(object_table, "static_friction", "Isaac binding.object"),
            dynamic_friction=_number(object_table, "dynamic_friction", "Isaac binding.object"),
        ),
        source=_pose(_mapping(document.get("source"), "Isaac binding.source"), "source"),
        destination=_pose(
            _mapping(document.get("destination"), "Isaac binding.destination"),
            "destination",
        ),
        goal=Goal(
            position_tolerance_m=_number(goal_table, "position_tolerance_m", "Isaac binding.goal"),
            orientation_tolerance_rad=_number(
                goal_table, "orientation_tolerance_rad", "Isaac binding.goal"
            ),
            max_linear_velocity_mps=_number(
                goal_table, "max_linear_velocity_mps", "Isaac binding.goal"
            ),
            max_angular_velocity_radps=_number(
                goal_table, "max_angular_velocity_radps", "Isaac binding.goal"
            ),
            minimum_gripper_open_m=_number(
                goal_table, "minimum_gripper_open_m", "Isaac binding.goal"
            ),
        ),
        simulation=Simulation(
            dt_seconds=_number(simulation, "dt_seconds", "Isaac binding.simulation"),
            decimation=_integer(simulation, "decimation", "Isaac binding.simulation"),
            episode_length_seconds=_number(
                simulation, "episode_length_seconds", "Isaac binding.simulation"
            ),
            num_envs=_integer(simulation, "num_envs", "Isaac binding.simulation"),
            env_spacing_m=_number(simulation, "env_spacing_m", "Isaac binding.simulation"),
        ),
    )


def load_prototype(task_path: Path, binding_path: Path) -> Prototype:
    """Load and cross-check the semantic task, scene, and Isaac binding."""
    task = _load_task(task_path)
    binding = _load_binding(binding_path)
    if binding.task_id != task.task_id:
        raise ContractError(
            f"binding task_id '{binding.task_id}' does not match task '{task.task_id}'"
        )
    if binding.source.station != task.source_station:
        raise ContractError(
            f"binding source station '{binding.source.station}' does not match "
            f"task source '{task.source_station}'"
        )
    if binding.destination.station != task.destination_station:
        raise ContractError(
            f"binding destination station '{binding.destination.station}' does not match "
            f"task destination '{task.destination_station}'"
        )
    return Prototype(task=task, binding=binding)
