"""Heat-shock transformation protocol emitted by the Lab OT-2 backend."""

import json
from typing import cast

from opentrons import protocol_api

from lab_opentrons_ot2.plan_types import Ot2ExecutionPlan

metadata = {
    "protocolName": "Lab heat-shock transformation",
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
    stage = profile["stages"]["transformation"]

    temperature = cast(
        protocol_api.TemperatureModuleContext,
        protocol.load_module(
            deck["temperature_module"]["model"], deck["temperature_module"]["slot"]
        ),
    )
    sources = temperature.load_labware(deck["temperature_module"]["labware"])
    thermocycler = cast(
        protocol_api.ThermocyclerContext,
        protocol.load_module(deck["thermocycler"]["model"]),
    )
    reaction_plate = thermocycler.load_labware(deck["thermocycler"]["labware"])
    dna_plates = [
        protocol.load_labware(stage["dna_plate"]["labware"], slot)
        for slot in stage["dna_plate"]["slots"]
    ]
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
    source_wells = PLAN["transformation_source_wells"]
    temperature.set_temperature(4)
    thermocycler.open_lid()

    for construct in PLAN["strains"]:
        chemistry = construct["chemistry"]
        cells = sources[source_wells["cells:" + construct["host"]]]
        for reaction in construct["transformations"]:
            destination = reaction_plate[reaction["culture_well"]]
            p20.transfer(chemistry["cell_volume_ul"], cells, destination, new_tip="always")
            for source in reaction["source_wells"]:
                p20.transfer(
                    chemistry["dna_volume_ul"],
                    dna_plates[source["plate"]][source["well"]],
                    destination,
                    new_tip="always",
                    mix_after=(3, 15),
                )

    # Every strain in a batch shares one heat-shock profile.
    shock = PLAN["strains"][0]["chemistry"]
    thermocycler.close_lid()
    thermocycler.set_block_temperature(4, hold_time_minutes=shock["cold_minutes"])
    thermocycler.set_block_temperature(
        shock["heat_shock_temperature_c"],
        hold_time_minutes=shock["heat_shock_minutes"],
    )
    thermocycler.set_block_temperature(4, hold_time_minutes=2)
    thermocycler.open_lid()
    recovery = sources[source_wells["reagent:recovery_medium"]]
    for construct in PLAN["strains"]:
        for reaction in construct["transformations"]:
            p300.transfer(
                construct["chemistry"]["recovery_volume_ul"],
                recovery,
                reaction_plate[reaction["culture_well"]],
                new_tip="always",
            )
    thermocycler.close_lid()
    thermocycler.set_block_temperature(
        shock["recovery_temperature_c"], hold_time_minutes=shock["recovery_minutes"]
    )
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    protocol.comment(
        "Transformation complete. Preserve the reaction plate for plating_protocol.py."
    )
