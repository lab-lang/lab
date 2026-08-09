"""Serial dilution and plating protocol emitted by the Lab OT-2 backend."""

import json
from typing import cast

from opentrons import protocol_api

from lab_opentrons_ot2.plan_types import Ot2ExecutionPlan

metadata = {
    "protocolName": "Lab serial dilution and plating",
    "author": "Lab Compiler",
    "description": "Generated concept protocol",
}
requirements = {
    "robotType": "OT-2",
    "apiLevel": "2.21",  # LAB:API_LEVEL
}
PLAN_JSON = "{}"  # LAB:EXECUTION_PLAN
PLAN = cast(Ot2ExecutionPlan, json.loads(PLAN_JSON))


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    deck = profile["deck"]
    stage = profile["stages"]["plating"]

    thermocycler = cast(
        protocol_api.ThermocyclerContext,
        protocol.load_module(deck["thermocycler"]["model"]),
    )
    cultures = thermocycler.load_labware(deck["thermocycler"]["labware"])
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
    dilution_plates = [
        protocol.load_labware(stage["dilution_plate"]["labware"], slot)
        for slot in stage["dilution_plate"]["slots"]
    ]
    agar_plates = [
        protocol.load_labware(stage["agar_plate"]["labware"], slot)
        for slot in stage["agar_plate"]["slots"]
    ]
    media_rack = protocol.load_labware(stage["media_rack"]["labware"], stage["media_rack"]["slot"])
    recovery_medium = media_rack[stage["media_rack"]["medium_well"]]
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()

    all_dilution_wells = [
        dilution_plates[well["plate"]][well["well"]]
        for construct in PLAN["strains"]
        for layout in construct["plating"]
        for well in layout["dilution_wells"]
    ]
    medium_volume = PLAN["strains"][0]["chemistry"]["medium_volume_ul"]
    p300.distribute(medium_volume, recovery_medium, all_dilution_wells, disposal_volume=0)

    for construct in PLAN["strains"]:
        chemistry = construct["chemistry"]
        protocol.comment("Plating " + construct["artifact"] + " on " + construct["selection"])
        for layout in construct["plating"]:
            source = cultures[layout["culture_well"]]
            for dilution_index, dilution_well in enumerate(layout["dilution_wells"]):
                dilution = dilution_plates[dilution_well["plate"]][dilution_well["well"]]
                p20.transfer(
                    chemistry["culture_volume_ul"],
                    source,
                    dilution,
                    new_tip="always",
                    mix_after=(5, 19),
                )
                for agar_well in layout["agar_wells"][dilution_index]:
                    agar = agar_plates[agar_well["plate"]][agar_well["well"]]
                    p20.transfer(
                        chemistry["colony_volume_ul"],
                        dilution,
                        agar.top(-8),
                        new_tip="always",
                    )
                source = dilution

    protocol.comment(
        "Plating complete. Incubate the selective agar under host-appropriate conditions."
    )
