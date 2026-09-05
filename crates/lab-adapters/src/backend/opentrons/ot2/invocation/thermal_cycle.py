"""Requirement-scoped thermal program emitted by Lab."""

import json

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab thermal program",
    "author": "Lab Compiler",
    "description": "One facility-allocated Procedure requirement",
}
requirements = {
    "robotType": "OT-2",
    "apiLevel": "2.21",  # LAB:API_LEVEL
}
PLAN_JSON = "{}"  # LAB:INVOCATION_PLAN
PLAN = json.loads(PLAN_JSON)


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    execution = PLAN["execution"]

    work = profile["resources"]["work"]
    thermocycler = protocol.load_module(work["model"])
    thermocycler.load_labware(work["labware"])
    protocol.comment(execution["title"])
    protocol.comment(
        "Process only the staged samples in wells "
        + ", ".join(execution["sample_wells"])
    )
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
    protocol.comment(
        "Thermal program complete. Preserve the named Procedure output before continuing."
    )
