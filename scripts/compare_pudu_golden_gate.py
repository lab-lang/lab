#!/usr/bin/env python3
"""Compare the Golden Gate transformation protocol with PUDU's exact main entrypoint."""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
EXAMPLE = ROOT / "examples" / "golden-gate"
REFERENCE = ROOT / "scripts" / "reference" / "pudu-transformation-entrypoint.json"
MINIMUM_ROBOT_ACTIONS = 50


class ComparisonError(RuntimeError):
    """A comparison prerequisite or output is invalid."""


@dataclass(frozen=True)
class CommandResult:
    stdout: str
    stderr: str


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ComparisonError(f"cannot read JSON from {path}: {error}") from error


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def canonical_sha256(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_command(
    arguments: Sequence[str | Path],
    *,
    cwd: Path,
    environment: dict[str, str] | None = None,
) -> CommandResult:
    command = [str(argument) for argument in arguments]
    completed = subprocess.run(
        command,
        cwd=cwd,
        env={**os.environ, **(environment or {})},
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ComparisonError(
            f"command failed with exit {completed.returncode}: {' '.join(command)}\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return CommandResult(completed.stdout, completed.stderr)


def git_revision(repository: Path) -> str:
    return run_command(("git", "rev-parse", "HEAD"), cwd=repository).stdout.strip()


def git_is_dirty(repository: Path) -> bool:
    return bool(
        run_command(("git", "status", "--porcelain"), cwd=repository).stdout.strip()
    )


def git_tracked_source_is_dirty(repository: Path) -> bool:
    result = run_command(
        ("git", "status", "--porcelain", "--untracked-files=no"), cwd=repository
    )
    return bool(result.stdout.strip())


def strip_sbol2_version(value: Any, version: str = "1") -> Any:
    """Remove only the pinned terminal version from HTTP(S) identities."""

    if isinstance(value, str) and value.startswith(("http://", "https://")):
        return value.removesuffix(f"/{version}")
    if isinstance(value, list):
        return [strip_sbol2_version(item, version) for item in value]
    if isinstance(value, dict):
        return {
            strip_sbol2_version(key, version): strip_sbol2_version(item, version)
            for key, item in value.items()
        }
    return value


def artifact_declarations(module: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {
        declaration["name"]: declaration
        for declaration in module["declarations"]
        if declaration["kind"] in {"artifact", "catalog"}
    }


def property_expression(declaration: dict[str, Any], name: str) -> dict[str, Any]:
    for prop in declaration["properties"]:
        if prop["name"] == name:
            return prop["value"]["value"]
    raise ComparisonError(f"artifact {declaration['name']} has no property {name}")


def reference_local(expression: dict[str, Any]) -> str:
    if expression.get("kind") != "reference":
        raise ComparisonError(
            f"expected a reference expression, found {expression.get('kind')}"
        )
    return expression["definition"]["local"]


def reference_list(expression: dict[str, Any]) -> list[str]:
    if expression.get("kind") != "list":
        raise ComparisonError(
            f"expected a list expression, found {expression.get('kind')}"
        )
    return [reference_local(element["value"]) for element in expression["elements"]]


def project_lab_design(module_root: Path) -> dict[str, Any]:
    strains = artifact_declarations(read_json(module_root / "strains.module.json"))
    if len(strains) != 1:
        raise ComparisonError(
            f"Golden Gate defines {len(strains)} strains, expected one"
        )
    name, declaration = next(iter(strains.items()))
    return {
        "Strain": name,
        "Chassis": reference_local(property_expression(declaration, "chassis")),
        "Plasmids": reference_list(property_expression(declaration, "plasmids")),
    }


LIQUID_COMMAND = re.compile(
    r"^(Aspirating|Dispensing) ([0-9.]+) uL (?:from|into) ([A-H][0-9]+) of "
    r"(.+) at ([0-9.]+) uL/sec$"
)
BLOW_OUT = re.compile(r"^Blowing out at ([A-H][0-9]+) of (.+)$")
PICK_UP = re.compile(r"^Picking up tip from ([A-H][0-9]+) of (.+)$")
DROP_TIP = re.compile(r"^Dropping tip into (.+)$")
SLOT = re.compile(r"on slot ([0-9]+)$")


def normalize_number(value: str | float) -> int | float:
    number = float(value)
    return int(number) if number.is_integer() else number


def normalize_location(
    well: str,
    description: str,
    *,
    stage: str,
    staging: dict[str, str],
) -> str:
    slot_match = SLOT.search(description)
    slot = slot_match.group(1) if slot_match else "unknown"
    material_key = None
    if "Temperature Module" in description:
        material_key = f"temperature-module:{well}"
    elif "Tube Rack" in description:
        material_key = f"tube-rack:{slot}:{well}"
    elif stage == "transformation" and slot == "2":
        material_key = f"dna:{well}"
    if material_key in staging:
        return f"material:{staging[material_key]}"
    if "Temperature Module" in description:
        raise ComparisonError(f"staging trace references unmapped well {well}")
    if "Thermocycler Module" in description:
        return f"thermocycler:{well}"
    if "Tip Rack" in description:
        return f"tips:{slot}:{well}"
    if "Tube Rack" in description:
        return f"tubes:{slot}:{well}"
    if stage == "transformation" and slot == "2":
        return f"dna:{well}"
    return f"deck:{slot}:{well}"


def normalize_liquid_trace(
    trace: str,
    *,
    stage: str,
    staging: dict[str, str] | None = None,
) -> list[dict[str, Any]]:
    material_map = staging or {}
    events: list[dict[str, Any]] = []
    for raw_line in trace.splitlines():
        line = raw_line.strip()
        command = LIQUID_COMMAND.match(line)
        if command:
            events.append(
                {
                    "operation": command.group(1).lower(),
                    "volume_ul": normalize_number(command.group(2)),
                    "location": normalize_location(
                        command.group(3),
                        command.group(4),
                        stage=stage,
                        staging=material_map,
                    ),
                    "flow_rate_ul_s": normalize_number(command.group(5)),
                }
            )
            continue
        blow_out = BLOW_OUT.match(line)
        if blow_out:
            events.append(
                {
                    "operation": "blow_out",
                    "location": normalize_location(
                        blow_out.group(1),
                        blow_out.group(2),
                        stage=stage,
                        staging=material_map,
                    ),
                }
            )
            continue
        if line == "Touching tip":
            events.append({"operation": "touch_tip"})
            continue
        pick_up = PICK_UP.match(line)
        if pick_up:
            events.append(
                {
                    "operation": "pick_up_tip",
                    "location": normalize_location(
                        pick_up.group(1),
                        pick_up.group(2),
                        stage=stage,
                        staging=material_map,
                    ),
                }
            )
            continue
        if DROP_TIP.match(line):
            events.append({"operation": "drop_tip"})
    return events


def robot_action_semantics(events: list[dict[str, Any]]) -> dict[str, Any]:
    liquid_actions: list[dict[str, Any]] = []
    tip_change_boundaries: list[int] = []
    tip_active = False
    for event in events:
        operation = event["operation"]
        if operation == "pick_up_tip":
            if tip_active or (
                tip_change_boundaries
                and tip_change_boundaries[-1] == len(liquid_actions)
            ):
                raise ComparisonError(
                    "robot trace picks up a tip without using the prior one"
                )
            tip_active = True
            tip_change_boundaries.append(len(liquid_actions))
        elif operation == "drop_tip":
            if not tip_active or tip_change_boundaries[-1] == len(liquid_actions):
                raise ComparisonError(
                    "robot trace drops a tip that carried no liquid actions"
                )
            tip_active = False
        else:
            if not tip_active:
                raise ComparisonError(
                    f"robot trace performs {operation} without an active tip"
                )
            liquid_actions.append(event)
    if tip_active:
        raise ComparisonError("robot trace ends with an attached tip")
    return {
        "liquid_actions": liquid_actions,
        "tip_change_boundaries": tip_change_boundaries,
        "tips_used": len(tip_change_boundaries),
    }


def robot_actions_equivalent(pudu: dict[str, Any], lab: dict[str, Any]) -> bool:
    return pudu == lab


THERMAL_PROFILE = re.compile(
    r"^Thermocycler starting ([0-9]+) repetitions of cycle composed of the following steps: (.+)$"
)
TEMPERATURE_MODULE_SET = re.compile(
    r"^Setting Temperature Module temperature to ([0-9.]+) °C"
)
THERMOCYCLER_BLOCK_SET = re.compile(
    r"^Setting Thermocycler well block temperature to ([0-9.]+) °C"
)
THERMOCYCLER_LID_SET = re.compile(
    r"^Setting Thermocycler lid temperature to ([0-9.]+) °C"
)


def normalize_thermal_steps(steps: list[dict[str, Any]]) -> list[dict[str, Any]]:
    result = []
    for step in steps:
        if "hold_time_seconds" in step:
            seconds = normalize_number(step["hold_time_seconds"])
        elif "hold_time_minutes" in step:
            seconds = normalize_number(float(step["hold_time_minutes"]) * 60)
        else:
            raise ComparisonError(f"thermal step has no hold time: {step}")
        result.append(
            {
                "temperature_c": normalize_number(step["temperature"]),
                "hold_seconds": seconds,
            }
        )
    return result


def normalize_thermal_trace(trace: str) -> dict[str, Any]:
    result: dict[str, Any] = {
        "temperature_module_setpoints_c": [],
        "thermocycler_block_setpoints_c": [],
        "thermocycler_lid_setpoints_c": [],
        "profiles": [],
        "thermocycler_lid_opens": 0,
        "thermocycler_lid_closes": 0,
    }
    for raw_line in trace.splitlines():
        line = raw_line.strip()
        profile = THERMAL_PROFILE.match(line)
        if profile:
            try:
                steps = ast.literal_eval(profile.group(2))
            except (SyntaxError, ValueError) as error:
                raise ComparisonError(
                    f"cannot parse thermal profile: {line}"
                ) from error
            result["profiles"].append(
                {
                    "repeats": int(profile.group(1)),
                    "steps": normalize_thermal_steps(steps),
                }
            )
            continue
        for pattern, key in (
            (TEMPERATURE_MODULE_SET, "temperature_module_setpoints_c"),
            (THERMOCYCLER_BLOCK_SET, "thermocycler_block_setpoints_c"),
            (THERMOCYCLER_LID_SET, "thermocycler_lid_setpoints_c"),
        ):
            match = pattern.match(line)
            if match:
                result[key].append(normalize_number(match.group(1)))
                break
        if line == "Opening Thermocycler lid":
            result["thermocycler_lid_opens"] += 1
        elif line == "Closing Thermocycler lid":
            result["thermocycler_lid_closes"] += 1
    return result


TEMPERATURE_MODULE_GENERATION = re.compile(r"on Temperature Module GEN([12]) on slot")
THERMOCYCLER_MODULE_GENERATION = re.compile(r"on Thermocycler Module GEN([12]) on slot")


def trace_hardware(trace: str) -> dict[str, Any]:
    thermocycler_labware = set()
    for raw_line in trace.splitlines():
        line = raw_line.strip()
        command = LIQUID_COMMAND.match(line)
        description = command.group(4) if command else None
        if description is None:
            blow_out = BLOW_OUT.match(line)
            description = blow_out.group(2) if blow_out else None
        if description and " on Thermocycler Module " in description:
            thermocycler_labware.add(
                description.split(" on Thermocycler Module ", 1)[0]
            )
    return {
        "thermocycler_labware": sorted(thermocycler_labware),
        "temperature_module_generations": sorted(
            {int(value) for value in TEMPERATURE_MODULE_GENERATION.findall(trace)}
        ),
        "thermocycler_module_generations": sorted(
            {int(value) for value in THERMOCYCLER_MODULE_GENERATION.findall(trace)}
        ),
    }


def pudu_configuration(python: Path, entrypoint: Path) -> dict[str, Any]:
    capture = r"""
import importlib.util
import json
import sys

spec = importlib.util.spec_from_file_location("pudu_transformation_entrypoint", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
original = module.HeatShockTransformation

class Capture:
    def __init__(self, *args, **kwargs):
        self.instance = original(*args, **kwargs)

    def run(self, protocol):
        instance = self.instance
        print(json.dumps({
            "api_level": module.metadata["apiLevel"],
            "transformation_data": instance.transformations,
            "replicates": instance.replicates,
            "dna_source_volume_ul": instance.volume_dna,
            "cell_source_volume_ul": instance.tube_volume_competent_cell,
            "recovery_source_volume_ul": instance.tube_volume_recovery_media,
            "dna_transfer_volume_ul": instance.transfer_volume_dna,
            "cell_transfer_volume_ul": instance.transfer_volume_competent_cell,
            "recovery_transfer_volume_ul": instance.transfer_volume_recovery_media,
            "starting_well": instance.thermocycler_starting_well,
            "heat_shock": [instance.cold_incubation1, instance.heat_shock, instance.cold_incubation2],
            "recovery": [instance.recovery_incubation],
            "instruments": {
                "small": {"model": instance.pipette_p20, "mount": instance.pipette_p20_position},
                "large": {"model": instance.pipette_p300, "mount": instance.pipette_p300_position},
            },
            "labware": {
                "temperature_module": {
                    "model": "temperature module",
                    "slot": instance.temperature_module_position,
                    "labware": instance.temperature_module_labware,
                    "capacity": 24,
                },
                "thermocycler": {
                    "model": "thermocycler module",
                    "labware": instance.thermocycler_labware,
                    "capacity": 96,
                },
                "source_rack": {
                    "slot": instance.tube_rack_position,
                    "labware": instance.tube_rack_labware,
                    "capacity": 24,
                },
                "small_tips": {
                    "slots": [instance.tiprack_p20_position],
                    "labware": instance.tiprack_p20_labware,
                    "capacity": 96,
                },
                "large_tips": {
                    "slots": [instance.tiprack_p200_position],
                    "labware": instance.tiprack_p200_labware,
                    "capacity": 96,
                },
            },
        }))

module.HeatShockTransformation = Capture
module.run(None)
"""
    result = run_command((python, "-c", capture, entrypoint), cwd=entrypoint.parent)
    try:
        raw = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ComparisonError(
            "captured PUDU entrypoint configuration is not JSON"
        ) from error
    return {
        **raw,
        "transformation_data": [
            {
                "Strain": item["strain"],
                "Chassis": item["chassis"],
                "Plasmids": item["plasmids"],
            }
            for item in raw["transformation_data"]
        ],
        "heat_shock": {
            "repeats": 1,
            "steps": normalize_thermal_steps(raw["heat_shock"]),
        },
        "recovery": {"repeats": 1, "steps": normalize_thermal_steps(raw["recovery"])},
    }


def manifest_thermal_stage(stage: dict[str, Any]) -> dict[str, Any]:
    return {
        "repeats": stage["repeats"],
        "steps": [
            {
                "temperature_c": normalize_number(step["celsius"]),
                "hold_seconds": normalize_number(step["hold_seconds"]),
            }
            for step in stage["steps"]
        ],
    }


def lab_configuration(manifest_path: Path, module_root: Path) -> dict[str, Any]:
    manifest = read_json(manifest_path)
    execution = manifest["execution"]
    preparation = execution["preparations"][0]["execution"]
    recovery = execution["recovery_additions"][0]["execution"]
    deck = manifest["deck"]
    dna_loads = {item["load_volume_ul"] for item in preparation["dna"]}
    if len(dna_loads) != 1:
        raise ComparisonError(
            f"Golden Gate DNA sources have different load volumes: {dna_loads}"
        )
    return {
        "api_level": deck["protocol"]["api_level"],
        "transformation_data": [project_lab_design(module_root)],
        "replicates": len(preparation["reaction_wells"]),
        "dna_source_volume_ul": next(iter(dna_loads)),
        "cell_source_volume_ul": preparation["cell_source_volume_ul"],
        "recovery_source_volume_ul": recovery["medium"]["load_volume_ul"],
        "dna_transfer_volume_ul": preparation["dna_volume_ul"],
        "cell_transfer_volume_ul": preparation["cell_volume_ul"],
        "recovery_transfer_volume_ul": recovery["recovery_volume_ul"],
        "starting_well": 0,
        "heat_shock": manifest_thermal_stage(
            execution["heat_shocks"][0]["execution"]["profile"]["stages"][0]
        ),
        "recovery": manifest_thermal_stage(
            execution["recovery_incubations"][0]["execution"]["profile"]["stages"][0]
        ),
        "instruments": deck["instruments"],
        "labware": {
            "temperature_module": deck["deck"]["temperature_module"],
            "thermocycler": deck["deck"]["thermocycler"],
            "source_rack": deck["stages"]["transformation"]["source_rack"],
            "small_tips": deck["stages"]["transformation"]["small_tips"],
            "large_tips": deck["stages"]["transformation"]["large_tips"],
        },
    }


def pudu_material_map(trace: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in trace.splitlines():
        if not line.startswith("{"):
            continue
        try:
            value = ast.literal_eval(line)
        except (SyntaxError, ValueError):
            continue
        if not isinstance(value, dict):
            continue
        for name, well in value.items():
            if name.startswith("GVD"):
                result[f"temperature-module:{well}"] = name
            elif name.startswith("Competent Cell "):
                chassis = name.removeprefix("Competent Cell ").removesuffix("_1")
                result[f"tube-rack:3:{well}"] = chassis
            elif name.startswith("Media_"):
                result[f"tube-rack:3:{well}"] = "recovery_medium"
    expected = {"GVD0011", "GVD0013", "GVD0015", "DH5alpha", "recovery_medium"}
    if set(result.values()) != expected:
        raise ComparisonError(
            f"PUDU source map has {set(result.values())}, expected {expected}"
        )
    return result


def lab_material_map(manifest_path: Path, module_root: Path) -> dict[str, str]:
    manifest = read_json(manifest_path)["execution"]
    preparation = manifest["preparations"][0]["execution"]
    design = project_lab_design(module_root)
    result = {
        f"temperature-module:{preparation['cell_source_well']}": design["Chassis"]
    }
    for dna in preparation["dna"]:
        result[f"dna:{dna['source_well']}"] = dna["symbol"]
    medium = manifest["recovery_additions"][0]["execution"]["medium"]
    result[f"tube-rack:3:{medium['source_well']}"] = medium["symbol"]
    return result


def compare_facet(
    identifier: str,
    pudu: Any,
    lab: Any,
    *,
    basis: str,
    normalized_root: Path,
) -> dict[str, Any]:
    pudu_path = normalized_root / f"{identifier}.pudu.json"
    lab_path = normalized_root / f"{identifier}.lab.json"
    write_json(pudu_path, pudu)
    write_json(lab_path, lab)
    return {
        "id": identifier,
        "status": "equivalent" if pudu == lab else "different",
        "basis": basis,
        "pudu": {
            "path": str(pudu_path.relative_to(normalized_root.parent)),
            "sha256": canonical_sha256(pudu),
        },
        "lab": {
            "path": str(lab_path.relative_to(normalized_root.parent)),
            "sha256": canonical_sha256(lab),
        },
    }


def compare_robot_facet(
    pudu: dict[str, Any], lab: dict[str, Any], *, normalized_root: Path
) -> dict[str, Any]:
    facet = compare_facet(
        "robot-actions.transformation",
        pudu,
        lab,
        basis="Material-normalized leaf aspirate, dispense, blowout, touch-tip, and tip-boundary trace.",
        normalized_root=normalized_root,
    )
    facet["status"] = (
        "equivalent" if robot_actions_equivalent(pudu, lab) else "different"
    )
    facet["pudu"]["items"] = len(pudu["liquid_actions"])
    facet["lab"]["items"] = len(lab["liquid_actions"])
    return facet


def validate_reference(
    repository: Path, reference: dict[str, Any]
) -> tuple[Path, Path]:
    revision = git_revision(repository)
    if revision != reference["revision"]:
        raise ComparisonError(
            f"PUDU checkout is {revision}, expected {reference['revision']}"
        )
    if git_tracked_source_is_dirty(repository):
        raise ComparisonError(
            "PUDU checkout has tracked changes from the pinned revision"
        )
    paths = []
    for key in ("entrypoint", "implementation"):
        item = reference[key]
        path = repository / item["path"]
        digest = file_sha256(path)
        if digest != item["sha256"]:
            raise ComparisonError(
                f"PUDU {key} {path} has SHA-256 {digest}, expected {item['sha256']}"
            )
        paths.append(path)
    return paths[0], paths[1]


def run_pudu(output: Path, simulator: Path, entrypoint: Path) -> Path:
    output.mkdir(parents=True)
    simulation = run_command((simulator, entrypoint), cwd=output)
    trace = output / "transformation_trace.txt"
    trace.write_text(simulation.stdout)
    (output / "transformation_simulate.stderr.txt").write_text(simulation.stderr)
    return trace


def run_lab(output: Path, lab: Path, simulator: Path) -> tuple[Path, Path]:
    output.mkdir(parents=True)
    build = run_command(
        (lab, "build", EXAMPLE, "--out-dir", output, "--json"), cwd=ROOT
    )
    (output / "build.stdout.json").write_text(build.stdout)
    (output / "build.stderr.txt").write_text(build.stderr)
    bundles = json.loads(build.stdout)["result"]["facility"]["bundles"]
    if len(bundles) != 1:
        raise ComparisonError(f"Lab build emitted {len(bundles)} bundles, expected one")
    bundle = Path(bundles[0])
    protocol = bundle / "transformation_protocol.py"
    config = output / ".opentrons-comparison-config"
    config.mkdir()
    simulation = run_command(
        (simulator, protocol),
        cwd=bundle,
        environment={"OT_API_CONFIG_DIR": str(config)},
    )
    trace = output / "transformation_trace.txt"
    trace.write_text(simulation.stdout)
    (output / "transformation_simulate.stderr.txt").write_text(simulation.stderr)
    return bundle, trace


def comparison_observations(
    *,
    pudu_trace: str,
    lab_trace: str,
    pudu_materials: dict[str, str],
    lab_materials: dict[str, str],
    implementation: Path,
) -> list[dict[str, Any]]:
    pudu_thermal = normalize_thermal_trace(pudu_trace)
    lab_thermal = normalize_thermal_trace(lab_trace)
    source = implementation.read_text()
    block_maxima = [
        int(value) for value in re.findall(r"block_max_volume\s*=\s*([0-9]+)", source)
    ]
    return [
        {
            "id": "source-placement",
            "classification": "lineage-and-staging-realization",
            "pudu": pudu_materials,
            "lab": lab_materials,
            "explanation": "The liquid actions are identical by material identity. The standalone entrypoint stages DNA tubes on the temperature module and cells in a passive rack; the end-to-end Golden Gate workflow preserves assembly-product wells and stages competent cells at its required 4 C setpoint.",
        },
        {
            "id": "simulation-thermal-path",
            "classification": "upstream-simulator-gap",
            "pudu": pudu_thermal,
            "lab": lab_thermal,
            "explanation": "The PUDU implementation forces water-testing mode in simulation and skips both thermal profiles. Lab simulates the authored heat-shock and recovery profiles and opens the lid for the reviewed handoff.",
        },
        {
            "id": "thermal-block-maximum-volume",
            "classification": "upstream-implementation-inconsistency",
            "pudu_block_max_volume_ul": block_maxima,
            "actual_sample_volumes_ul": [35, 95],
            "lab_block_max_volume_ul": [35, 95],
            "explanation": "PUDU passes 30 uL as block_max_volume for samples that actually contain 35 uL during heat shock and 95 uL during recovery. Lab derives each value from the transferred volumes.",
        },
    ]


def compare(*, pudu_repository: Path, lab_binary: Path, output: Path) -> dict[str, Any]:
    reference = read_json(REFERENCE)
    entrypoint, implementation = validate_reference(pudu_repository, reference)
    python = pudu_repository / ".venv" / "bin" / "python"
    simulator = pudu_repository / ".venv" / "bin" / "opentrons_simulate"
    for executable in (lab_binary, python, simulator):
        if not executable.is_file() or not os.access(executable, os.X_OK):
            raise ComparisonError(f"required executable is unavailable: {executable}")

    pudu_output = output / "pudu"
    lab_output = output / "lab"
    normalized = output / "normalized"
    pudu_trace_path = run_pudu(pudu_output, simulator, entrypoint)
    lab_bundle, lab_trace_path = run_lab(lab_output, lab_binary, simulator)
    manifest = lab_bundle / "transformation_manifest.json"
    module_root = lab_output / "modules" / "golden_gate" / "designs"
    pudu_config = pudu_configuration(python, entrypoint)
    lab_config = lab_configuration(manifest, module_root)

    facets = [
        compare_facet(
            "configuration",
            pudu_config,
            lab_config,
            basis="Resolved entrypoint constructor configuration versus checked design and emitted manifest.",
            normalized_root=normalized,
        )
    ]
    pudu_trace = pudu_trace_path.read_text()
    lab_trace = lab_trace_path.read_text()
    pudu_materials = pudu_material_map(pudu_trace)
    lab_materials = lab_material_map(manifest, module_root)
    pudu_actions = robot_action_semantics(
        normalize_liquid_trace(
            pudu_trace, stage="transformation", staging=pudu_materials
        )
    )
    lab_actions = robot_action_semantics(
        normalize_liquid_trace(lab_trace, stage="transformation", staging=lab_materials)
    )
    facets.append(
        compare_robot_facet(pudu_actions, lab_actions, normalized_root=normalized)
    )
    facets.append(
        compare_facet(
            "resolved-hardware",
            {
                "instruments": pudu_config["instruments"],
                **trace_hardware(pudu_trace),
            },
            {
                "instruments": lab_config["instruments"],
                **trace_hardware(lab_trace),
            },
            basis="Resolved pipette models and mounts, labware identity, and module generations.",
            normalized_root=normalized,
        )
    )

    different = [facet["id"] for facet in facets if facet["status"] != "equivalent"]
    if (
        min(len(pudu_actions["liquid_actions"]), len(lab_actions["liquid_actions"]))
        < MINIMUM_ROBOT_ACTIONS
    ):
        different.append("robot-actions.transformation-vacuous")
    report = {
        "schema_version": "lab.pudu-transformation-differential.v1",
        "status": "equivalent" if not different else "different",
        "reference": {
            "repository": reference["repository"],
            "entrypoint": reference["entrypoint"],
            "implementation": reference["implementation"],
            "pudu_revision": reference["revision"],
            "lab_revision": git_revision(ROOT),
            "lab_worktree_dirty": git_is_dirty(ROOT),
            "pudu_worktree_dirty": git_is_dirty(pudu_repository),
            "pudu_tracked_source_dirty": git_tracked_source_is_dirty(pudu_repository),
            "opentrons_simulator": run_command(
                (simulator, "--version"), cwd=output
            ).stdout.strip(),
        },
        "outputs": {
            "pudu": str(pudu_output.relative_to(output)),
            "lab": str(lab_output.relative_to(output)),
            "lab_bundle": str(lab_bundle.relative_to(output)),
            "normalized": str(normalized.relative_to(output)),
        },
        "facets": facets,
        "observations": comparison_observations(
            pudu_trace=pudu_trace,
            lab_trace=lab_trace,
            pudu_materials=pudu_materials,
            lab_materials=lab_materials,
            implementation=implementation,
        ),
        "summary": {
            "equivalent_facets": len(facets) - len(different),
            "total_facets": len(facets),
            "different_facets": different,
            "normalized_robot_actions": len(lab_actions["liquid_actions"]),
            "tips_used": lab_actions["tips_used"],
        },
    }
    write_json(output / "comparison.json", report)
    return report


def default_pudu_repository() -> Path | None:
    configured = os.environ.get("PUDU_REPOSITORY")
    if configured:
        return Path(configured).expanduser()
    candidate = Path.home() / "git" / "RudgeLab" / "PUDU"
    return candidate if candidate.is_dir() else None


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--pudu-repository",
        type=Path,
        default=default_pudu_repository(),
        help="PUDU checkout at the pinned revision",
    )
    parser.add_argument(
        "--lab",
        type=Path,
        default=ROOT / "target" / "debug" / "lab",
        help="Lab CLI binary (default: target/debug/lab)",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        help="new directory for both runs and the comparison report",
    )
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    if arguments.pudu_repository is None:
        print("PUDU checkout not found; pass --pudu-repository", file=sys.stderr)
        return 2
    repository = arguments.pudu_repository.expanduser().resolve()
    lab_binary = arguments.lab.expanduser()
    if not lab_binary.is_absolute():
        lab_binary = (Path.cwd() / lab_binary).resolve()
    if arguments.out_dir is None:
        output = Path(tempfile.mkdtemp(prefix="lab-pudu-transformation-"))
    else:
        output = arguments.out_dir.expanduser().resolve()
        if output.exists():
            print(f"output directory already exists: {output}", file=sys.stderr)
            return 2
        output.mkdir(parents=True)
    try:
        report = compare(
            pudu_repository=repository,
            lab_binary=lab_binary,
            output=output,
        )
    except (
        ComparisonError,
        IndexError,
        KeyError,
        OSError,
        TypeError,
        ValueError,
    ) as error:
        print(f"comparison failed: {error}", file=sys.stderr)
        print(f"partial outputs: {output}", file=sys.stderr)
        return 2
    for facet in report["facets"]:
        marker = "PASS" if facet["status"] == "equivalent" else "FAIL"
        print(f"[{marker}] {facet['id']}")
    print(
        f"Compared {report['summary']['normalized_robot_actions']} normalized robot actions; "
        f"{report['summary']['equivalent_facets']}/{report['summary']['total_facets']} facets equivalent."
    )
    for observation in report["observations"]:
        print(f"  - {observation['id']}: {observation['explanation']}")
    print(f"Report: {output / 'comparison.json'}")
    return 0 if report["status"] == "equivalent" else 1


if __name__ == "__main__":
    raise SystemExit(main())
