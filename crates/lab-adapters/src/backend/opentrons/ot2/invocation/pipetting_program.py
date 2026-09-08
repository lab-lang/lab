"""Direct interpreter for one facility-allocated canonical PipettingProgramV1."""

import json
from typing import Any

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab canonical pipetting program",
    "author": "Lab Compiler",
    "description": "One facility-allocated canonical Procedure program",
}
requirements = {
    "robotType": "OT-2",
    "apiLevel": "2.21",  # LAB:API_LEVEL
}
PLAN_JSON = "{}"  # LAB:INVOCATION_PLAN
PLAN = json.loads(PLAN_JSON)


def _quantity(quantity: dict[str, Any]) -> float:
    return float(quantity["value"]["value"])


def _tracked_offset(profile: dict[str, Any], remaining: float) -> float:
    calibration = profile["techniques"]
    loaded = float(calibration["tracked_source_volume_ul"])
    usable = max(remaining, 0.0)
    fraction = usable / loaded
    height = fraction * calibration["tracked_usable_depth_offset_mm"]
    height -= calibration["tracked_meniscus_offset_mm"]
    return float(max(height, calibration["tracked_minimum_height_mm"]))


def _aspiration_location(
    well: Any,
    strategy: dict[str, Any],
    profile: dict[str, Any],
    remaining: float,
) -> Any:
    kind = strategy["kind"]
    if kind == "liquid":
        return well
    if kind == "tracked_liquid_surface":
        return well.bottom(_tracked_offset(profile, remaining))
    if kind == "vessel_bottom":
        return well.bottom(_quantity(strategy["offset"]))
    raise RuntimeError(f"Unsupported canonical aspiration strategy: {kind}")


def _dispense_location(
    well: Any, strategy: dict[str, Any], profile: dict[str, Any]
) -> Any:
    kind = strategy["kind"]
    calibration = profile["techniques"]
    if kind == "liquid":
        return well
    if kind == "above_liquid":
        return well.top(calibration["above_liquid_offset_mm"])
    if kind == "vessel_bottom":
        return well.bottom(_quantity(strategy["offset"]))
    if kind == "vessel_top":
        return well.top(_quantity(strategy["offset"]))
    if kind == "material_surface":
        return well.top(calibration["material_surface_offset_mm"])
    raise RuntimeError(f"Unsupported canonical dispense strategy: {kind}")


def _finish(
    pipette: Any,
    well: Any,
    technique: dict[str, Any],
    profile: dict[str, Any],
) -> None:
    calibration = profile["techniques"]
    if technique["blow_out"]:
        pipette.blow_out(well)
    if technique["touch_tip"]:
        pipette.touch_tip(
            well,
            radius=calibration["touch_tip_radius"],
            v_offset=calibration["touch_tip_vertical_offset_mm"],
            speed=calibration["touch_tip_speed_mm_s"],
        )


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    execution = PLAN["execution"]
    program = execution["program"]

    temperature = protocol.load_module(
        profile["resources"]["sources"]["model"],
        profile["resources"]["sources"]["slot"],
    )
    source_labware = temperature.load_labware(
        profile["resources"]["sources"]["labware"]
    )
    thermocycler = protocol.load_module(profile["resources"]["work"]["model"])
    work_labware = thermocycler.load_labware(profile["resources"]["work"]["labware"])
    thermocycler.open_lid()
    staging = execution["staging_temperatures"]
    if "sources" in staging:
        temperature.set_temperature(staging["sources"])
    if "work" in staging:
        thermocycler.set_block_temperature(staging["work"])
    bulk_labware = protocol.load_labware(
        profile["resources"]["bulk"]["labware"], profile["resources"]["bulk"]["slot"]
    )
    surface_labware = protocol.load_labware(
        profile["resources"]["surface"]["labware"],
        profile["resources"]["surface"]["slot"],
    )
    small_tip_racks = [
        protocol.load_labware(profile["resources"]["small_tips"]["labware"], slot)
        for slot in profile["resources"]["small_tips"]["slots"]
    ]
    large_tip_racks = [
        protocol.load_labware(profile["resources"]["large_tips"]["labware"], slot)
        for slot in profile["resources"]["large_tips"]["slots"]
    ]
    small = protocol.load_instrument(
        profile["instruments"]["small"]["model"],
        profile["instruments"]["small"]["mount"],
        tip_racks=small_tip_racks,
    )
    large = protocol.load_instrument(
        profile["instruments"]["large"]["model"],
        profile["instruments"]["large"]["mount"],
        tip_racks=large_tip_racks,
    )

    def physical(location: dict[str, Any]) -> Any:
        allocated = execution["locations"][location["vessel"]][location["position"]]
        resource = allocated["resource"]
        kind = resource["kind"]
        if kind == "sources":
            labware = source_labware
        elif kind == "work":
            labware = work_labware
        elif kind == "bulk":
            labware = bulk_labware
        elif kind == "surface":
            labware = surface_labware
        else:
            raise RuntimeError(f"Unknown canonical physical resource: {kind}")
        return labware[allocated["well"]]

    def step_volume(step: dict[str, Any]) -> float:
        kind = step["kind"]
        if kind == "transfer":
            result = _quantity(step["volume"])
        elif kind == "distribute":
            result = _quantity(step["volume_each"])
        elif kind == "mix":
            result = _quantity(step["volume"])
        else:
            return 0.0
        technique = step.get("technique", {})
        if technique.get("air_gap") is not None:
            result += _quantity(technique["air_gap"])
        return result

    remaining = {
        (vessel, position): volume
        for vessel, volumes in execution["initial_volumes_ul"].items()
        for position, volume in enumerate(volumes)
    }

    def aspirate(
        pipette: Any,
        source_ref: dict[str, Any],
        volume: float,
        technique: dict[str, Any],
        distribution: bool = False,
    ) -> None:
        source = physical(source_ref)
        key = (source_ref["vessel"], source_ref["position"])
        remaining_before = remaining[key]
        pipette.aspirate(
            volume,
            _aspiration_location(
                source, technique["aspiration"], profile, remaining_before
            ),
            rate=profile["techniques"][
                "distribution_aspiration_rate" if distribution else "aspiration_rate"
            ],
        )
        remaining[key] -= volume
        air_gap = technique.get("air_gap")
        if air_gap is not None:
            pipette.air_gap(_quantity(air_gap))

    def dispense(
        pipette: Any,
        destination_ref: dict[str, Any],
        volume: float,
        technique: dict[str, Any],
    ) -> None:
        destination = physical(destination_ref)
        pipette.dispense(
            volume
            + (
                _quantity(technique["air_gap"])
                if technique.get("air_gap") is not None
                else 0.0
            ),
            _dispense_location(destination, technique["dispense"], profile),
            rate=profile["techniques"]["dispense_rate"],
        )
        key = (destination_ref["vessel"], destination_ref["position"])
        remaining[key] += volume
        _finish(pipette, destination, technique, profile)

    def execute_mix(
        pipette: Any, target_ref: dict[str, Any], step: dict[str, Any]
    ) -> None:
        target = physical(target_ref)
        technique = step["technique"]
        volume = _quantity(step["volume"])
        key = (target_ref["vessel"], target_ref["position"])
        for _ in range(step["cycles"]):
            pipette.aspirate(
                volume,
                _aspiration_location(
                    target,
                    technique["aspiration"],
                    profile,
                    remaining[key],
                ),
                rate=profile["techniques"]["mix_aspiration_rate"],
            )
            pipette.dispense(
                volume,
                _dispense_location(target, technique["dispense"], profile),
                rate=profile["techniques"]["dispense_rate"],
            )
        _finish(pipette, target, technique, profile)

    group_maximum: dict[str, float] = {}
    for planned_step in program["steps"]:
        planned_group = planned_step.get("fluid_path_group")
        if planned_group is not None:
            group_maximum[planned_group] = max(
                group_maximum.get(planned_group, 0.0), step_volume(planned_step)
            )

    held: Any | None = None
    held_group: str | None = None
    for step in program["steps"]:
        kind = step["kind"]
        group = step.get("fluid_path_group")
        if group != held_group:
            if held is not None:
                held.drop_tip()
                held = None
            held_group = group
        if kind == "barrier":
            if held is not None:
                held.drop_tip()
                held = None
            protocol.comment(step["reason"])
            continue
        required_volume = group_maximum.get(group, step_volume(step))
        pipette = small if required_volume <= small.max_volume else large
        if held is not None and held is not pipette:
            raise RuntimeError("A canonical fluid-path group crosses pipette classes")

        if kind == "transfer":
            if held is None:
                pipette.pick_up_tip()
                held = pipette
            elif held is not pipette:
                raise RuntimeError(
                    "A canonical fluid-path group crosses pipette classes"
                )
            volume = _quantity(step["volume"])
            aspirate(pipette, step["source"], volume, step["technique"])
            dispense(pipette, step["destination"], volume, step["technique"])
            if group is None:
                pipette.drop_tip()
                held = None
        elif kind == "distribute":
            each = _quantity(step["volume_each"])
            isolated = step["fluid_path"] == "isolated_destinations"
            if isolated:
                for destination in step["destinations"]:
                    if held is None:
                        pipette.pick_up_tip()
                        held = pipette
                    aspirate(
                        pipette,
                        step["source"],
                        each,
                        step["technique"],
                        distribution=True,
                    )
                    dispense(pipette, destination, each, step["technique"])
                    if group is None:
                        pipette.drop_tip()
                        held = None
            else:
                available = min(pipette.max_volume, 20.0 if pipette is small else 200.0)
                air_gap = step["technique"].get("air_gap")
                if air_gap is not None:
                    available -= _quantity(air_gap)
                per_load = max(int(available // each), 1)
                # Blowout empties the tip. Never preload later destinations' liquid
                # when each destination requires its own blowout.
                if step["technique"]["blow_out"]:
                    per_load = 1
                for start in range(0, len(step["destinations"]), per_load):
                    chunk = step["destinations"][start : start + per_load]
                    if held is None:
                        pipette.pick_up_tip()
                        held = pipette
                    aspirate(
                        pipette,
                        step["source"],
                        each * len(chunk),
                        step["technique"],
                        distribution=True,
                    )
                    for index, destination in enumerate(chunk):
                        if index > 0 and air_gap is not None:
                            pipette.air_gap(_quantity(air_gap))
                        dispense(pipette, destination, each, step["technique"])
                    if group is None:
                        pipette.drop_tip()
                        held = None
        elif kind == "mix":
            for target in step["targets"]:
                if held is None:
                    pipette.pick_up_tip()
                    held = pipette
                elif held is not pipette:
                    raise RuntimeError(
                        "A canonical fluid-path group crosses pipette classes"
                    )
                execute_mix(pipette, target, step)
                if group is None:
                    pipette.drop_tip()
                    held = None
        else:
            raise RuntimeError(f"Unknown canonical pipetting step: {kind}")

    if held is not None:
        held.drop_tip()
    protocol.comment("Canonical PipettingProgramV1 complete.")
