"""Requirement-scoped Golden Gate thermal cycle emitted by Lab."""

import json

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab Golden Gate thermal cycle",
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
    deck = profile["deck"]
    execution = PLAN["execution"]

    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    thermocycler.load_labware(deck["thermocycler"]["labware"])
    protocol.comment(
        "Cycle only the staged reactions in wells " + ", ".join(execution["reaction_wells"])
    )
    thermocycler.close_lid()
    thermocycler.set_lid_temperature(execution["lid_temperature_c"])
    thermocycler.execute_profile(
        steps=[
            {
                "temperature": execution["digest_temperature_c"],
                "hold_time_minutes": execution["digest_minutes"],
            },
            {
                "temperature": execution["ligate_temperature_c"],
                "hold_time_minutes": execution["ligate_minutes"],
            },
        ],
        repetitions=execution["cycles"],
        block_max_volume=execution["reaction_volume_ul"],
    )
    thermocycler.set_block_temperature(
        execution["final_digest_temperature_c"],
        hold_time_minutes=execution["final_digest_minutes"],
    )
    thermocycler.set_block_temperature(
        execution["heat_inactivation_temperature_c"],
        hold_time_minutes=execution["heat_inactivation_minutes"],
    )
    thermocycler.set_block_temperature(execution["hold_temperature_c"])
    thermocycler.deactivate_lid()
    thermocycler.open_lid()
    protocol.comment("Thermal cycling complete. Recover the named Procedure output before continuing.")
