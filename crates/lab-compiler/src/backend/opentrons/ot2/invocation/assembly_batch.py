"""Allocated Golden Gate assembly and cycling run emitted by Lab."""

import json
from typing import Any

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab Golden Gate assembly",
    "author": "Lab Compiler",
    "description": "One reviewed multi-task run on an allocated OT-2",
}
requirements = {
    "robotType": "OT-2",
    "apiLevel": "2.21",  # LAB:API_LEVEL
}
PLAN_JSON = "{}"  # LAB:INVOCATION_PLAN
PLAN = json.loads(PLAN_JSON)


def _quantity_value(quantity: dict[str, Any]) -> float:
    return float(quantity["value"]["value"])


def _aspiration_location(well: Any, strategy: dict[str, Any]) -> Any:
    kind = strategy["kind"]
    if kind == "liquid":
        return well
    if kind == "vessel_bottom":
        return well.bottom(_quantity_value(strategy["offset"]))
    raise RuntimeError(f"Unsupported assembly aspiration strategy: {kind}")


def _dispense_location(
    well: Any, strategy: dict[str, Any], techniques: dict[str, Any]
) -> Any:
    kind = strategy["kind"]
    if kind == "liquid":
        return well
    if kind == "above_liquid":
        return well.top(techniques["above_liquid_offset_mm"])
    if kind == "vessel_bottom":
        return well.bottom(_quantity_value(strategy["offset"]))
    if kind == "vessel_top":
        return well.top(_quantity_value(strategy["offset"]))
    if kind == "material_surface":
        return well.top(techniques["material_surface_offset_mm"])
    raise RuntimeError(f"Unsupported assembly dispense strategy: {kind}")


def _finish_technique(
    pipette: Any,
    well: Any,
    technique: dict[str, Any],
    techniques: dict[str, Any],
) -> None:
    if technique["blow_out"]:
        pipette.blow_out()
    if technique["touch_tip"]:
        pipette.touch_tip(
            well,
            radius=techniques["touch_tip_radius"],
            v_offset=techniques["touch_tip_vertical_offset_mm"],
            speed=techniques["touch_tip_speed_mm_s"],
        )


def _execute_mix(
    pipette: Any,
    well: Any,
    mixing: dict[str, Any],
    techniques: dict[str, Any],
) -> None:
    for _ in range(mixing["cycles"]):
        pipette.aspirate(
            mixing["volume_ul"],
            _aspiration_location(well, mixing["technique"]["aspiration"]),
        )
        pipette.dispense(
            mixing["volume_ul"],
            _dispense_location(
                well, mixing["technique"]["dispense"], techniques
            ),
        )
        _finish_technique(pipette, well, mixing["technique"], techniques)


def _execute_thermal_program(
    thermocycler: Any, execution: dict[str, Any]
) -> None:
    thermocycler.close_lid()
    if execution["lid_temperature_c"] is not None:
        thermocycler.set_lid_temperature(execution["lid_temperature_c"])
    for stage in execution["profile"]["stages"]:
        thermocycler.execute_profile(
            steps=[
                {
                    "temperature": step["celsius"],
                    "hold_time_seconds": step["hold_seconds"],
                }
                for step in stage["steps"]
            ],
            repetitions=stage["repeats"],
            block_max_volume=execution["volume_each_ul"],
        )
    if execution["final_hold_celsius"] is not None:
        thermocycler.set_block_temperature(execution["final_hold_celsius"])
    thermocycler.deactivate_lid()
    thermocycler.open_lid()


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    deck = profile["deck"]
    stage = profile["stages"]["assembly"]
    techniques = profile["techniques"]
    execution = PLAN["execution"]

    temperature = protocol.load_module(
        deck["temperature_module"]["model"],
        deck["temperature_module"]["slot"],
    )
    sources = temperature.load_labware(deck["temperature_module"]["labware"])
    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    reaction_plate = thermocycler.load_labware(deck["thermocycler"]["labware"])
    tips = [
        protocol.load_labware(stage["small_tips"]["labware"], slot)
        for slot in stage["small_tips"]["slots"]
    ]
    pipette = protocol.load_instrument(
        profile["instruments"]["small"]["model"],
        profile["instruments"]["small"]["mount"],
        tip_racks=tips,
    )

    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    if execution["source_temperature_c"] is not None:
        temperature.set_temperature(execution["source_temperature_c"])
    for scheduled in execution["setups"]:
        setup = scheduled["execution"]
        protocol.comment(f"Assembly task {scheduled['task']}: {setup['artifact']}")
        for destination_name in setup["reaction_wells"]:
            destination = reaction_plate[destination_name]
            reused_final_tip = False
            for addition_index, addition in enumerate(setup["additions"]):
                source = sources[addition["source_well"]]
                pipette.pick_up_tip()
                source_mix = addition.get("source_mix")
                if source_mix is not None:
                    _execute_mix(pipette, source, source_mix, techniques)
                pipette.aspirate(
                    addition["volume_ul"],
                    _aspiration_location(
                        source, addition["transfer_technique"]["aspiration"]
                    ),
                    rate=techniques["aspiration_rate"],
                )
                pipette.dispense(
                    addition["volume_ul"],
                    _dispense_location(
                        destination,
                        addition["transfer_technique"]["dispense"],
                        techniques,
                    ),
                    rate=techniques["dispense_rate"],
                )
                _finish_technique(
                    pipette,
                    destination,
                    addition["transfer_technique"],
                    techniques,
                )
                if addition["reuse_tip_for_final_mix"]:
                    if addition_index + 1 != len(setup["additions"]):
                        raise RuntimeError(
                            "Only the final assembly addition may share its path with final mixing"
                        )
                    _execute_mix(pipette, destination, setup["final_mix"], techniques)
                    reused_final_tip = True
                pipette.drop_tip()
            if not reused_final_tip:
                pipette.pick_up_tip()
                _execute_mix(pipette, destination, setup["final_mix"], techniques)
                pipette.drop_tip()

    if execution["source_temperature_c"] is not None:
        protocol.comment("Assembly sources may now be removed from the temperature module.")
        temperature.deactivate()

    thermal_programs = execution["thermal_programs"]
    if not thermal_programs:
        raise RuntimeError("The allocated assembly run has no thermal program")
    thermal = thermal_programs[0]["execution"]
    protocol.comment(
        "Cycling all allocated assembly wells: "
        + ", ".join(
            well
            for scheduled in thermal_programs
            for well in scheduled["execution"]["sample_wells"]
        )
    )
    _execute_thermal_program(thermocycler, thermal)
    protocol.comment(
        "Assembly complete. Move this product plate to the DNA-source slot named in the transformation manifest."
    )
