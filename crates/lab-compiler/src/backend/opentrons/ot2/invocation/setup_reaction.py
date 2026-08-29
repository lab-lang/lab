"""Requirement-scoped Golden Gate reaction setup emitted by Lab."""

import json

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab Golden Gate reaction setup",
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
    thermocycler.open_lid()

    for destination_name in execution["reaction_wells"]:
        destination = reaction_plate[destination_name]
        for addition in execution["additions"]:
            volume = addition["volume_ul"]
            if volume > 0:
                pipette.transfer(
                    volume,
                    sources[addition["source_well"]],
                    destination,
                    new_tip="always",
                )
        pipette.pick_up_tip()
        pipette.mix(execution["mix_cycles"], execution["mix_volume_ul"], destination)
        pipette.drop_tip()

    protocol.comment(
        "Reaction setup complete. Preserve the reaction plate for the allocated thermal-cycling requirement."
    )
