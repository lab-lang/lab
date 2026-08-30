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

    temperature.set_temperature(4)
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    for scheduled in execution["setups"]:
        setup = scheduled["execution"]
        protocol.comment(f"Assembly task {scheduled['task']}: {setup['artifact']}")
        for destination_name in setup["reaction_wells"]:
            destination = reaction_plate[destination_name]
            for addition in setup["additions"]:
                if addition["volume_ul"] > 0:
                    pipette.transfer(
                        addition["volume_ul"],
                        sources[addition["source_well"]],
                        destination,
                        new_tip="always",
                    )
            pipette.pick_up_tip()
            pipette.mix(setup["mix_cycles"], setup["mix_volume_ul"], destination)
            pipette.drop_tip()

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
