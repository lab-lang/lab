#!/usr/bin/env python3
"""Run Lab and PUDU's documented Golden Gate workflow and compare their outputs."""

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
REFERENCE = EXAMPLE / "reference" / "pudu-workflow.json"
STAGES = ("assembly", "transformation", "plating")


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


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def canonical_sha256(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


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
        rendered = " ".join(command)
        raise ComparisonError(
            f"command failed with exit {completed.returncode}: {rendered}\n"
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
    return bool(
        run_command(
            ("git", "status", "--porcelain", "--untracked-files=no"),
            cwd=repository,
        ).stdout.strip()
    )


def strip_sbol2_version(value: Any, version: str = "1") -> Any:
    """Convert PUDU's SBOL 2 version URIs to their SBOL 3 persistent identities."""

    if isinstance(value, str) and value.startswith(("http://", "https://")):
        suffix = f"/{version}"
        return value.removesuffix(suffix)
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


def project_lab_inputs(module_root: Path) -> dict[str, Any]:
    inventory = read_json(module_root / "inventory.module.json")
    plasmids = read_json(module_root / "plasmids.module.json")
    strains = read_json(module_root / "strains.module.json")
    inventory_artifacts = artifact_declarations(inventory)
    plasmid_artifacts = artifact_declarations(plasmids)
    strain_artifacts = artifact_declarations(strains)

    inventory_identity = {
        name: declaration["sbol_identity"]
        for name, declaration in inventory_artifacts.items()
    }
    plasmid_identity = {
        name: declaration["sbol_identity"]
        for name, declaration in plasmid_artifacts.items()
    }

    assembly = []
    for declaration in plasmid_artifacts.values():
        assembly.append(
            {
                "Product": declaration["sbol_identity"],
                "Backbone": inventory_identity[
                    reference_local(property_expression(declaration, "backbone"))
                ],
                "PartsList": [
                    inventory_identity[name]
                    for name in reference_list(
                        property_expression(declaration, "components")
                    )
                ],
                "Restriction Enzyme": inventory_identity[
                    reference_local(
                        property_expression(declaration, "restriction_enzyme")
                    )
                ],
            }
        )

    transformation = []
    for declaration in strain_artifacts.values():
        transformation.append(
            {
                "Strain": declaration["sbol_identity"],
                "Chassis": inventory_identity[
                    reference_local(property_expression(declaration, "chassis"))
                ],
                "Plasmids": [
                    plasmid_identity[name]
                    for name in reference_list(
                        property_expression(declaration, "plasmids")
                    )
                ],
            }
        )

    return {"assembly": assembly, "transformation": transformation}


def project_lab_design_names(module_root: Path) -> dict[str, dict[str, Any]]:
    strains = artifact_declarations(read_json(module_root / "strains.module.json"))
    result: dict[str, dict[str, Any]] = {}
    for name, declaration in strains.items():
        result[name] = {
            "chassis": reference_local(property_expression(declaration, "chassis")),
            "plasmids": reference_list(property_expression(declaration, "plasmids")),
        }
    return result


def project_pudu_assembly_handoff(path: Path, version: str) -> dict[str, list[str]]:
    value = strip_sbol2_version(read_json(path), version)
    if not isinstance(value, dict):
        raise ComparisonError("PUDU transformation_input.json is not an object")
    return value


def project_lab_assembly_handoff(
    manifest_path: Path, module_root: Path
) -> dict[str, list[str]]:
    plasmids = artifact_declarations(read_json(module_root / "plasmids.module.json"))
    identities = {
        name: declaration["sbol_identity"] for name, declaration in plasmids.items()
    }
    manifest = read_json(manifest_path)
    result: dict[str, list[str]] = {}
    for setup in manifest["execution"]["setups"]:
        execution = setup["execution"]
        result[identities[execution["artifact"]]] = execution["reaction_wells"]
    return result


def project_pudu_cultures(path: Path) -> list[dict[str, Any]]:
    locations = read_json(path)["bacterium_locations"]
    result = []
    for well, fields in locations.items():
        if not isinstance(fields, list) or len(fields) != 4:
            raise ComparisonError(
                f"PUDU culture {well} does not have four lineage fields"
            )
        competent = fields[1]
        if not competent.startswith("Competent_Cell_"):
            raise ComparisonError(
                f"PUDU culture {well} has an unknown cell label {competent}"
            )
        result.append(
            {
                "well": well,
                "strain": fields[0],
                "chassis": competent.removeprefix("Competent_Cell_"),
                "plasmids": [fields[2]],
                "medium": "recovery_medium" if fields[3] == "Media_1" else fields[3],
            }
        )
    return sorted(result, key=lambda item: item["well"])


def project_lab_cultures(
    manifest_path: Path, module_root: Path
) -> list[dict[str, Any]]:
    designs = project_lab_design_names(module_root)
    manifest = read_json(manifest_path)
    medium_by_artifact = {
        item["execution"]["artifact"]: item["execution"]["medium"]["symbol"]
        for item in manifest["execution"]["recovery_additions"]
    }
    result = []
    for preparation in manifest["execution"]["preparations"]:
        execution = preparation["execution"]
        artifact = execution["artifact"]
        design = designs[artifact]
        for well in execution["reaction_wells"]:
            result.append(
                {
                    "well": well,
                    "strain": artifact,
                    "chassis": design["chassis"],
                    "plasmids": design["plasmids"],
                    "medium": medium_by_artifact[artifact],
                }
            )
    return sorted(result, key=lambda item: item["well"])


def project_pudu_plate_lineage(path: Path) -> list[dict[str, Any]]:
    plates = read_json(path)["agar_plates"]
    result = []
    for plate_name, dilutions in plates.items():
        plate = int(plate_name.removeprefix("plate_")) - 1
        for dilution_name, dilution in dilutions.items():
            dilution_number = int(dilution_name.removeprefix("dilution_"))
            for well, entry in dilution["wells"].items():
                result.append(
                    {
                        "subject": entry["construct"].split(", ", 1)[0],
                        "dilution": dilution_number,
                        "dilution_ratio": dilution["ratio"],
                        "culture_source_well": entry["source_well"],
                        "plating_replicate": entry["replicate"],
                        "destination": {"plate": plate, "well": well},
                    }
                )
    return sorted(
        result,
        key=lambda item: (
            item["subject"],
            item["dilution"],
            item["culture_source_well"],
            item["plating_replicate"],
            item["destination"]["plate"],
            item["destination"]["well"],
        ),
    )


def project_lab_plate_lineage(
    plate_map_path: Path, transformation_manifest_path: Path
) -> list[dict[str, Any]]:
    transformation = read_json(transformation_manifest_path)
    culture_wells = {
        item["execution"]["artifact"]: item["execution"]["reaction_wells"]
        for item in transformation["execution"]["preparations"]
    }
    plate_map = read_json(plate_map_path)
    result = []
    for entry in plate_map["entries"]:
        result.append(
            {
                "subject": entry["subject"],
                "dilution": entry["dilution"],
                "dilution_ratio": entry["dilution_ratio"],
                "culture_source_well": culture_wells[entry["subject"]][
                    entry["culture_replicate"] - 1
                ],
                "plating_replicate": entry["plating_replicate"],
                "destination": entry["destination"],
            }
        )
    return sorted(
        result,
        key=lambda item: (
            item["subject"],
            item["dilution"],
            item["culture_source_well"],
            item["plating_replicate"],
            item["destination"]["plate"],
            item["destination"]["well"],
        ),
    )


PUDU_STAGING_NAMES = {
    "Deionized Water": "nuclease_free_water",
    "T4 DNA Ligase Buffer": "T4_DNA_ligase_buffer",
    "T4 DNA Ligase": "T4_DNA_ligase",
    "Restriction Enzyme BsaI": "BsaI",
}


def pudu_staging_map(trace: str) -> dict[str, str]:
    for line in trace.splitlines():
        if not line.startswith("{"):
            continue
        try:
            value = ast.literal_eval(line)
        except (SyntaxError, ValueError):
            continue
        if isinstance(value, dict) and "Deionized Water" in value:
            return {
                f"temperature-module:{well}": PUDU_STAGING_NAMES.get(name, name)
                for name, well in value.items()
            }
    raise ComparisonError("PUDU assembly trace has no staging material map")


def lab_staging_map(manifest_path: Path) -> dict[str, str]:
    manifest = read_json(manifest_path)
    result: dict[str, str] = {}
    for setup in manifest["execution"]["setups"]:
        for addition in setup["execution"]["additions"]:
            well = addition["source_well"]
            symbol = addition["symbol"]
            previous = result.setdefault(f"temperature-module:{well}", symbol)
            if previous != symbol:
                raise ComparisonError(f"Lab staging well {well} names two materials")
    return result


def pudu_transformation_material_map(trace: str) -> dict[str, str]:
    for line in trace.splitlines():
        if not line.startswith("{"):
            continue
        try:
            value = ast.literal_eval(line)
        except (SyntaxError, ValueError):
            continue
        if not isinstance(value, dict) or "Media_1" not in value:
            continue
        result = {}
        for name, well in value.items():
            if name == "Media_1":
                material = "recovery_medium"
            elif name.startswith("Competent Cell ") and name.endswith("_1"):
                material = name.removeprefix("Competent Cell ").removesuffix("_1")
            else:
                raise ComparisonError(
                    f"PUDU transformation trace has unknown source material {name}"
                )
            result[f"tube-rack:3:{well}"] = material
        return result
    raise ComparisonError("PUDU transformation trace has no source material map")


def lab_transformation_material_map(
    manifest_path: Path, module_root: Path
) -> dict[str, str]:
    manifest = read_json(manifest_path)
    designs = project_lab_design_names(module_root)
    result: dict[str, str] = {}
    for preparation in manifest["execution"]["preparations"]:
        execution = preparation["execution"]
        material = designs[execution["artifact"]]["chassis"]
        key = f"temperature-module:{execution['cell_source_well']}"
        previous = result.setdefault(key, material)
        if previous != material:
            raise ComparisonError(f"Lab transformation position {key} names two materials")
    for addition in manifest["execution"]["recovery_additions"]:
        medium = addition["execution"]["medium"]
        key = f"tube-rack:3:{medium['source_well']}"
        previous = result.setdefault(key, medium["symbol"])
        if previous != medium["symbol"]:
            raise ComparisonError(f"Lab transformation position {key} names two materials")
    return result


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
    if stage == "plating" and slot in {"2", "3"}:
        return f"dilution:{int(slot) - 2}:{well}"
    if stage == "plating" and slot in {"5", "6"}:
        return f"agar:{int(slot) - 5}:{well}"
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
                raise ComparisonError("robot trace picks up a tip without using the prior one")
            tip_active = True
            tip_change_boundaries.append(len(liquid_actions))
        elif operation == "drop_tip":
            if not tip_active or tip_change_boundaries[-1] == len(liquid_actions):
                raise ComparisonError("robot trace drops a tip that carried no liquid actions")
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


def robot_actions_equivalent(
    pudu: dict[str, Any], lab: dict[str, Any]
) -> bool:
    return pudu["liquid_actions"] == lab["liquid_actions"] and set(
        pudu["tip_change_boundaries"]
    ).issubset(lab["tip_change_boundaries"])


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
            hold_seconds = normalize_number(step["hold_time_seconds"])
        elif "hold_time_minutes" in step:
            hold_seconds = normalize_number(float(step["hold_time_minutes"]) * 60)
        else:
            raise ComparisonError(f"thermal step has no hold time: {step}")
        result.append(
            {
                "temperature_c": normalize_number(step["temperature"]),
                "hold_seconds": hold_seconds,
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


def thermal_core(trace: str) -> dict[str, Any]:
    thermal = normalize_thermal_trace(trace)
    return {
        "temperature_module_setpoints_c": thermal["temperature_module_setpoints_c"],
        "thermocycler_block_setpoints_c": thermal["thermocycler_block_setpoints_c"],
        "thermocycler_lid_setpoints_c": thermal["thermocycler_lid_setpoints_c"],
        "profiles": thermal["profiles"],
    }


def pudu_transformation_configuration(
    python: Path, generated_protocol: Path
) -> dict[str, Any]:
    program = """
import importlib.util
import json
import sys

spec = importlib.util.spec_from_file_location("generated_pudu_transformation", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
instance = module.HeatShockTransformation(
    transformation_data=module.transformation_data,
    plasmid_locations=module.plasmid_locations,
)
print(json.dumps({
    "replicates": instance.replicates,
    "cell_volume_ul": instance.transfer_volume_competent_cell,
    "dna_volume_ul": instance.transfer_volume_dna,
    "recovery_volume_ul": instance.transfer_volume_recovery_media,
    "heat_shock": [instance.cold_incubation1, instance.heat_shock, instance.cold_incubation2],
    "recovery": [instance.recovery_incubation],
}))
"""
    output = run_command(
        (python, "-c", program, generated_protocol), cwd=generated_protocol.parent
    )
    try:
        raw = json.loads(output.stdout)
    except json.JSONDecodeError as error:
        raise ComparisonError(
            "PUDU transformation configuration is not JSON"
        ) from error
    return {
        "replicates": raw["replicates"],
        "cell_volume_ul": normalize_number(raw["cell_volume_ul"]),
        "dna_volume_ul": normalize_number(raw["dna_volume_ul"]),
        "recovery_volume_ul": normalize_number(raw["recovery_volume_ul"]),
        "heat_shock": {
            "repeats": 1,
            "steps": normalize_thermal_steps(raw["heat_shock"]),
        },
        "recovery": {"repeats": 1, "steps": normalize_thermal_steps(raw["recovery"])},
    }


def pudu_transformation_instruments(
    python: Path, generated_protocol: Path
) -> dict[str, dict[str, str]]:
    program = """
import importlib.util
import json
import sys

spec = importlib.util.spec_from_file_location("generated_pudu_transformation", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
class CaptureHardware(module.HeatShockTransformation):
    def run(self, protocol):
        print(json.dumps({
            "small": {"model": self.pipette_p20, "mount": self.pipette_p20_position},
            "large": {"model": self.pipette_p300, "mount": self.pipette_p300_position},
        }))
module.HeatShockTransformation = CaptureHardware
module.run(None)
"""
    output = run_command(
        (python, "-c", program, generated_protocol), cwd=generated_protocol.parent
    )
    try:
        raw = json.loads(output.stdout)
    except json.JSONDecodeError as error:
        raise ComparisonError("PUDU instrument configuration is not JSON") from error
    return raw


def lab_transformation_configuration(manifest_path: Path) -> dict[str, Any]:
    manifest = read_json(manifest_path)["execution"]
    preparation = manifest["preparations"][0]["execution"]
    recovery_addition = manifest["recovery_additions"][0]["execution"]
    heat_shock = manifest["heat_shocks"][0]["execution"]["profile"]["stages"][0]
    recovery = manifest["recovery_incubations"][0]["execution"]["profile"]["stages"][0]

    def manifest_stage(stage: dict[str, Any]) -> dict[str, Any]:
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

    return {
        "replicates": len(preparation["reaction_wells"]),
        "cell_volume_ul": preparation["cell_volume_ul"],
        "dna_volume_ul": preparation["dna_volume_ul"],
        "recovery_volume_ul": recovery_addition["recovery_volume_ul"],
        "heat_shock": manifest_stage(heat_shock),
        "recovery": manifest_stage(recovery),
    }


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
                description.split(" on Thermocycler Module ", maxsplit=1)[0]
            )
    temperature_module_generations = sorted(
        {int(generation) for generation in TEMPERATURE_MODULE_GENERATION.findall(trace)}
    )
    thermocycler_module_generations = sorted(
        {int(generation) for generation in THERMOCYCLER_MODULE_GENERATION.findall(trace)}
    )
    return {
        "thermocycler_labware": sorted(thermocycler_labware),
        "temperature_module_generations": temperature_module_generations,
        "thermocycler_module_generations": thermocycler_module_generations,
    }


def workflow_hardware(
    instruments: dict[str, Any], traces: dict[str, Path]
) -> dict[str, Any]:
    traced = [trace_hardware(path.read_text()) for path in traces.values()]
    return {
        "instruments": instruments,
        "temperature_module_generations": sorted(
            {
                generation
                for hardware in traced
                for generation in hardware["temperature_module_generations"]
            }
        ),
        "thermocycler_module_generations": sorted(
            {
                generation
                for hardware in traced
                for generation in hardware["thermocycler_module_generations"]
            }
        ),
    }


# Fewest leaf operations a robot-action facet can contain and still be comparing anything. The
# shipped example produces 184 in assembly, 153 in transformation and 258 in dilution/plating.
MINIMUM_ROBOT_ACTIONS_PER_FACET = 50


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
            "items": len(pudu) if isinstance(pudu, (dict, list)) else None,
        },
        "lab": {
            "path": str(lab_path.relative_to(normalized_root.parent)),
            "sha256": canonical_sha256(lab),
            "items": len(lab) if isinstance(lab, (dict, list)) else None,
        },
    }


def compare_robot_action_facet(
    identifier: str,
    pudu: dict[str, Any],
    lab: dict[str, Any],
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
        "status": "equivalent" if robot_actions_equivalent(pudu, lab) else "different",
        "basis": basis,
        "pudu": {
            "path": str(pudu_path.relative_to(normalized_root.parent)),
            "sha256": canonical_sha256(pudu),
            "items": len(pudu["liquid_actions"]),
        },
        "lab": {
            "path": str(lab_path.relative_to(normalized_root.parent)),
            "sha256": canonical_sha256(lab),
            "items": len(lab["liquid_actions"]),
        },
    }


def tip_refinement_observations(
    actions: dict[str, tuple[dict[str, Any], dict[str, Any]]],
) -> list[dict[str, Any]]:
    result = []
    for stage, (pudu, lab) in actions.items():
        if pudu["tip_change_boundaries"] == lab["tip_change_boundaries"]:
            continue
        result.append(
            {
                "id": f"{stage}-fresh-tip-refinement",
                "classification": "contamination-safety-refinement",
                "pudu": {
                    "tips_used": pudu["tips_used"],
                    "boundaries": pudu["tip_change_boundaries"],
                },
                "lab": {
                    "tips_used": lab["tips_used"],
                    "boundaries": lab["tip_change_boundaries"],
                },
                "explanation": "Lab introduces additional fresh-tip boundaries while preserving every PUDU boundary and every liquid action.",
            }
        )
    return result


def validate_reference(pudu_repository: Path, reference: dict[str, Any]) -> None:
    actual_revision = git_revision(pudu_repository)
    if actual_revision != reference["revision"]:
        raise ComparisonError(
            f"PUDU checkout is {actual_revision}, expected pinned revision {reference['revision']}"
        )
    if git_tracked_source_is_dirty(pudu_repository):
        raise ComparisonError("PUDU checkout has tracked changes from the pinned revision")
    for item in reference["inputs"].values():
        upstream = pudu_repository / item["upstream_path"]
        actual_sha256 = file_sha256(upstream)
        if actual_sha256 != item["upstream_sha256"]:
            raise ComparisonError(
                f"PUDU input {upstream} has SHA-256 {actual_sha256}, expected "
                f"{item['upstream_sha256']}"
            )
        snapshot = REFERENCE.parent / item["snapshot"]
        if read_json(upstream) != read_json(snapshot):
            raise ComparisonError(
                f"Lab snapshot {snapshot} differs from pinned PUDU input {upstream}"
            )


def run_pudu(
    *,
    output: Path,
    python: Path,
    simulator: Path,
    reference: dict[str, Any],
) -> dict[str, Path]:
    output.mkdir(parents=True)
    snapshots = {
        name: REFERENCE.parent / item["snapshot"]
        for name, item in reference["inputs"].items()
    }
    commands = (
        (
            "assembly",
            (
                python,
                "-m",
                "pudu.generate_protocol",
                snapshots["assembly"],
                "-o",
                "assembly_protocol.py",
                "--protocol-type",
                "assembly",
            ),
        ),
    )
    for stage, command in commands:
        result = run_command(command, cwd=output)
        (output / f"{stage}_generate.stdout.txt").write_text(result.stdout)
        (output / f"{stage}_generate.stderr.txt").write_text(result.stderr)
    simulation = run_command((simulator, "assembly_protocol.py"), cwd=output)
    (output / "assembly_trace.txt").write_text(simulation.stdout)
    (output / "assembly_simulate.stderr.txt").write_text(simulation.stderr)

    transformation = run_command(
        (
            python,
            "-m",
            "pudu.generate_protocol",
            snapshots["transformation"],
            "-o",
            "transformation_protocol.py",
            "--protocol-type",
            "transformation",
            "--plasmid-locations",
            "transformation_input.json",
        ),
        cwd=output,
    )
    (output / "transformation_generate.stdout.txt").write_text(transformation.stdout)
    (output / "transformation_generate.stderr.txt").write_text(transformation.stderr)
    simulation = run_command((simulator, "transformation_protocol.py"), cwd=output)
    (output / "transformation_trace.txt").write_text(simulation.stdout)
    (output / "transformation_simulate.stderr.txt").write_text(simulation.stderr)

    plating = run_command(
        (
            python,
            "-m",
            "pudu.generate_protocol",
            "plating_input.json",
            "-o",
            "plating_protocol.py",
            "--protocol-type",
            "plating",
        ),
        cwd=output,
    )
    (output / "plating_generate.stdout.txt").write_text(plating.stdout)
    (output / "plating_generate.stderr.txt").write_text(plating.stderr)
    simulation = run_command((simulator, "plating_protocol.py"), cwd=output)
    (output / "plating_trace.txt").write_text(simulation.stdout)
    (output / "plating_simulate.stderr.txt").write_text(simulation.stderr)
    return {stage: output / f"{stage}_trace.txt" for stage in STAGES}


def run_lab(
    *, output: Path, lab: Path, simulator: Path
) -> tuple[Path, dict[str, Path]]:
    output.mkdir(parents=True)
    build = run_command(
        (lab, "build", EXAMPLE, "--out-dir", output, "--json"),
        cwd=ROOT,
    )
    (output / "build.stdout.json").write_text(build.stdout)
    (output / "build.stderr.txt").write_text(build.stderr)
    result = json.loads(build.stdout)
    bundles = result["result"]["facility"]["bundles"]
    if len(bundles) != 1:
        raise ComparisonError(f"Lab build emitted {len(bundles)} bundles, expected one")
    bundle = Path(bundles[0])
    traces: dict[str, Path] = {}
    config = output / ".opentrons-comparison-config"
    config.mkdir()
    for stage in STAGES:
        protocol = bundle / f"{stage}_protocol.py"
        simulation = run_command(
            (simulator, protocol),
            cwd=bundle,
            environment={"OT_API_CONFIG_DIR": str(config)},
        )
        trace = output / f"{stage}_trace.txt"
        trace.write_text(simulation.stdout)
        (output / f"{stage}_simulate.stderr.txt").write_text(simulation.stderr)
        traces[stage] = trace
    return bundle, traces


def observations(
    *,
    pudu_traces: dict[str, Path],
    lab_traces: dict[str, Path],
) -> list[dict[str, Any]]:
    result = []
    pudu_thermal = normalize_thermal_trace(pudu_traces["transformation"].read_text())
    lab_thermal = normalize_thermal_trace(lab_traces["transformation"].read_text())
    if pudu_thermal["profiles"] != lab_thermal["profiles"]:
        result.append(
            {
                "id": "pudu-transformation-simulation-omits-thermal-programs",
                "classification": "upstream-simulator-gap",
                "pudu_profiles": pudu_thermal["profiles"],
                "lab_profiles": lab_thermal["profiles"],
                "explanation": "PUDU switches to water_testing during Opentrons simulation and skips heat shock and recovery incubation; Lab simulates the configured hardware path.",
            }
        )

    pudu_hardware = {
        stage: trace_hardware(path.read_text()) for stage, path in pudu_traces.items()
    }
    lab_hardware = {
        stage: trace_hardware(path.read_text()) for stage, path in lab_traces.items()
    }
    if (
        pudu_hardware["transformation"]["thermocycler_labware"]
        != pudu_hardware["plating"]["thermocycler_labware"]
    ):
        result.append(
            {
                "id": "pudu-plating-source-labware-discontinuity",
                "classification": "upstream-stage-handoff-gap",
                "pudu": pudu_hardware,
                "lab": lab_hardware,
                "explanation": "PUDU transforms in a NEST 100 uL PCR plate but its generated plating protocol loads a Bio-Rad 200 uL source plate; Lab preserves one NEST plate across the handoff.",
            }
        )
    if (
        pudu_hardware["assembly"]["temperature_module_generations"]
        != lab_hardware["assembly"]["temperature_module_generations"]
    ):
        result.append(
            {
                "id": "temperature-module-generation",
                "classification": "facility-configuration-difference",
                "pudu": pudu_hardware["assembly"]["temperature_module_generations"],
                "lab": lab_hardware["assembly"]["temperature_module_generations"],
                "explanation": "PUDU's generic module load resolves to GEN1; the Golden Gate facility explicitly configures a GEN2 staging module. The Thermocycler is GEN1 in both outputs.",
            }
        )
    if (
        pudu_hardware["transformation"]["temperature_module_generations"]
        != lab_hardware["transformation"]["temperature_module_generations"]
    ):
        result.append(
            {
                "id": "competent-cell-temperature-control",
                "classification": "facility-safety-improvement",
                "pudu": pudu_hardware["transformation"][
                    "temperature_module_generations"
                ],
                "lab": lab_hardware["transformation"][
                    "temperature_module_generations"
                ],
                "explanation": "PUDU's simulated transformation stages competent cells in a passive tube rack; Lab uses the facility's GEN1 Temperature Module at the required 4 C setpoint.",
            }
        )

    for stage in ("assembly", "transformation"):
        pudu_state = normalize_thermal_trace(pudu_traces[stage].read_text())
        lab_state = normalize_thermal_trace(lab_traces[stage].read_text())
        if (
            pudu_state["thermocycler_lid_opens"] != lab_state["thermocycler_lid_opens"]
            or pudu_state["thermocycler_lid_closes"]
            != lab_state["thermocycler_lid_closes"]
        ):
            result.append(
                {
                    "id": f"{stage}-final-lid-state",
                    "classification": "safe-handoff-difference",
                    "pudu": {
                        "opens": pudu_state["thermocycler_lid_opens"],
                        "closes": pudu_state["thermocycler_lid_closes"],
                    },
                    "lab": {
                        "opens": lab_state["thermocycler_lid_opens"],
                        "closes": lab_state["thermocycler_lid_closes"],
                    },
                    "explanation": "Lab opens the GEN1 Thermocycler after the final thermal work so the reviewed plate handoff is physically possible.",
                }
            )
    return result


def compare(
    *,
    pudu_repository: Path,
    lab_binary: Path,
    output: Path,
) -> dict[str, Any]:
    reference = read_json(REFERENCE)
    validate_reference(pudu_repository, reference)
    pudu_python = pudu_repository / ".venv" / "bin" / "python"
    simulator = pudu_repository / ".venv" / "bin" / "opentrons_simulate"
    for executable in (lab_binary, pudu_python, simulator):
        if not executable.is_file() or not os.access(executable, os.X_OK):
            raise ComparisonError(f"required executable is unavailable: {executable}")

    pudu_output = output / "pudu"
    lab_output = output / "lab"
    normalized = output / "normalized"
    pudu_traces = run_pudu(
        output=pudu_output,
        python=pudu_python,
        simulator=simulator,
        reference=reference,
    )
    lab_bundle, lab_traces = run_lab(
        output=lab_output,
        lab=lab_binary,
        simulator=simulator,
    )
    module_root = lab_output / "modules" / "golden_gate" / "designs"
    version = reference["identity_normalization"]["strip_terminal_version"]
    pudu_inputs = {
        name: strip_sbol2_version(
            read_json(REFERENCE.parent / item["snapshot"]),
            version,
        )
        for name, item in reference["inputs"].items()
    }
    lab_inputs = project_lab_inputs(module_root)

    facets = [
        compare_facet(
            "input.assembly",
            pudu_inputs["assembly"],
            lab_inputs["assembly"],
            basis="Checked Lab module output versus the pinned PUDU input after removing only the SBOL 2 /1 version segment.",
            normalized_root=normalized,
        ),
        compare_facet(
            "input.transformation",
            pudu_inputs["transformation"],
            lab_inputs["transformation"],
            basis="Checked Lab module output versus the pinned PUDU input after removing only the SBOL 2 /1 version segment.",
            normalized_root=normalized,
        ),
        compare_facet(
            "handoff.assembly-products",
            project_pudu_assembly_handoff(
                pudu_output / "transformation_input.json", version
            ),
            project_lab_assembly_handoff(
                lab_bundle / "assembly_manifest.json",
                module_root,
            ),
            basis="Generated product-identity to reaction-well handoff.",
            normalized_root=normalized,
        ),
        compare_facet(
            "handoff.transformed-cultures",
            project_pudu_cultures(pudu_output / "plating_input.json"),
            project_lab_cultures(
                lab_bundle / "transformation_manifest.json",
                module_root,
            ),
            basis="Generated strain, chassis, plasmid, medium, and thermocycler-well lineage.",
            normalized_root=normalized,
        ),
        compare_facet(
            "output.plate-lineage",
            project_pudu_plate_lineage(pudu_output / "plating_layout.json"),
            project_lab_plate_lineage(
                lab_bundle / "plate_map.json",
                lab_bundle / "transformation_manifest.json",
            ),
            basis="Generated end-to-end culture source, dilution, replicate, and agar destination lineage.",
            normalized_root=normalized,
        ),
    ]

    pudu_assembly_trace = pudu_traces["assembly"].read_text()
    lab_assembly_trace = lab_traces["assembly"].read_text()
    pudu_materials = {
        "assembly": pudu_staging_map(pudu_assembly_trace),
        "transformation": pudu_transformation_material_map(
            pudu_traces["transformation"].read_text()
        ),
    }
    lab_materials = {
        "assembly": lab_staging_map(lab_bundle / "assembly_manifest.json"),
        "transformation": lab_transformation_material_map(
            lab_bundle / "transformation_manifest.json", module_root
        ),
    }
    robot_actions: dict[str, tuple[dict[str, Any], dict[str, Any]]] = {}
    for stage in STAGES:
        pudu_actions = robot_action_semantics(
            normalize_liquid_trace(
                pudu_traces[stage].read_text(),
                stage=stage,
                staging=pudu_materials.get(stage),
            )
        )
        lab_actions = robot_action_semantics(
            normalize_liquid_trace(
                lab_traces[stage].read_text(),
                stage=stage,
                staging=lab_materials.get(stage),
            )
        )
        robot_actions[stage] = (pudu_actions, lab_actions)
        facets.append(
            compare_robot_action_facet(
                f"robot-actions.{stage}",
                pudu_actions,
                lab_actions,
                basis="Exact leaf Opentrons aspirate, dispense, blowout, and touch-tip actions; Lab may only refine PUDU's contamination boundaries by taking a fresh tip sooner.",
                normalized_root=normalized,
            )
        )

    facets.extend(
        (
            compare_facet(
                "hardware.workflow",
                workflow_hardware(
                    pudu_transformation_instruments(
                        pudu_python,
                        pudu_output / "transformation_protocol.py",
                    ),
                    pudu_traces,
                ),
                workflow_hardware(
                    read_json(lab_bundle / "transformation_manifest.json")["deck"][
                        "instruments"
                    ],
                    lab_traces,
                ),
                basis="Resolved P20 and P300 models and mounts plus Temperature Module and Thermocycler Module generations across the simulated workflow.",
                normalized_root=normalized,
            ),
            compare_facet(
                "thermal.assembly",
                thermal_core(pudu_assembly_trace),
                thermal_core(lab_assembly_trace),
                basis="Thermocycler and staging setpoints plus normalized generated assembly profiles.",
                normalized_root=normalized,
            ),
            compare_facet(
                "thermal.transformation-intent",
                pudu_transformation_configuration(
                    pudu_python,
                    pudu_output / "transformation_protocol.py",
                ),
                lab_transformation_configuration(
                    lab_bundle / "transformation_manifest.json"
                ),
                basis="Resolved configuration of PUDU's generated protocol versus Lab's generated transformation manifest.",
                normalized_root=normalized,
            ),
        )
    )

    different = [facet["id"] for facet in facets if facet["status"] != "equivalent"]
    # Two empty traces compare equal. If a normalizer stops recognizing the simulator's output,
    # every robot-action facet would report "equivalent" while comparing nothing at all, so each
    # one has to carry the operations it is supposed to be comparing.
    vacuous = [
        facet["id"]
        for facet in facets
        if facet["id"].startswith("robot-actions.")
        and (facet["lab"]["items"] or 0) < MINIMUM_ROBOT_ACTIONS_PER_FACET
    ]
    different.extend(vacuous)
    report = {
        "schema_version": "lab.pudu-differential.v1",
        "status": "equivalent" if not different else "different",
        "reference": {
            "guide": reference["guide"],
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
        "observations": observations(pudu_traces=pudu_traces, lab_traces=lab_traces)
        + tip_refinement_observations(robot_actions),
        "summary": {
            "equivalent_facets": len(facets) - len(different),
            "total_facets": len(facets),
            "different_facets": different,
            "vacuous_facets": vacuous,
            "normalized_robot_actions": sum(
                facet["lab"]["items"]
                for facet in facets
                if facet["id"].startswith("robot-actions.")
            ),
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
        help="PUDU checkout at the pinned revision (default: PUDU_REPOSITORY or ~/git/RudgeLab/PUDU)",
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
        help="new directory for both runs and the comparison report (default: a retained temporary directory)",
    )
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    if arguments.pudu_repository is None:
        print("PUDU checkout not found; pass --pudu-repository", file=sys.stderr)
        return 2
    pudu_repository = arguments.pudu_repository.expanduser().resolve()
    lab_binary = arguments.lab.expanduser()
    if not lab_binary.is_absolute():
        lab_binary = (Path.cwd() / lab_binary).resolve()
    if arguments.out_dir is None:
        output = Path(tempfile.mkdtemp(prefix="lab-pudu-golden-gate-"))
    else:
        output = arguments.out_dir.expanduser().resolve()
        if output.exists():
            print(f"output directory already exists: {output}", file=sys.stderr)
            return 2
        output.mkdir(parents=True)

    try:
        report = compare(
            pudu_repository=pudu_repository,
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
    if report["observations"]:
        print("Observed output differences:")
        for observation in report["observations"]:
            print(f"  - {observation['id']}: {observation['explanation']}")
    print(f"Report: {output / 'comparison.json'}")
    return 0 if report["status"] == "equivalent" else 1


if __name__ == "__main__":
    raise SystemExit(main())
