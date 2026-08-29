"""Requirement-scoped serial dilution emitted by Lab."""

import json

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab serial dilution",
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
    stage = profile["stages"]["plating"]
    execution = PLAN["execution"]

    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    cultures = thermocycler.load_labware(deck["thermocycler"]["labware"])
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    dilution_plates = [
        protocol.load_labware(stage["dilution_plate"]["labware"], slot)
        for slot in stage["dilution_plate"]["slots"]
    ]
    media_rack = protocol.load_labware(
        stage["media_rack"]["labware"], stage["media_rack"]["slot"]
    )
    small_tips = [
        protocol.load_labware(stage["small_tips"]["labware"], slot)
        for slot in stage["small_tips"]["slots"]
    ]
    large_tips = [
        protocol.load_labware(stage["large_tips"]["labware"], slot)
        for slot in stage["large_tips"]["slots"]
    ]
    p20 = protocol.load_instrument(
        profile["instruments"]["small"]["model"],
        profile["instruments"]["small"]["mount"],
        tip_racks=small_tips,
    )
    p300 = protocol.load_instrument(
        profile["instruments"]["large"]["model"],
        profile["instruments"]["large"]["mount"],
        tip_racks=large_tips,
    )

    dilution_wells = [
        dilution_plates[well["plate"]][well["well"]]
        for well in execution["dilution_wells"]
    ]
    recovery_medium = media_rack[execution["medium"]["source_well"]]
    p300.distribute(
        execution["medium_volume_ul"],
        recovery_medium,
        dilution_wells,
        disposal_volume=0,
    )

    source = cultures[execution["culture_well"]]
    for dilution in dilution_wells:
        p20.transfer(
            execution["culture_volume_ul"],
            source,
            dilution,
            new_tip="always",
            mix_after=(execution["mix_cycles"], execution["mix_volume_ul"]),
        )
        source = dilution

    protocol.comment(
        "Serial dilution complete. Preserve the allocated dilution wells for downstream Procedure tasks."
    )
