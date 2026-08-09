"""Golden Gate assembly protocol emitted by the Lab OT-2 backend."""

import json
from typing import cast

from opentrons import protocol_api

from lab_opentrons_ot2.plan_types import Ot2ExecutionPlan

metadata = {
    "protocolName": "Lab Golden Gate assembly",
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
    stage = profile["stages"]["assembly"]

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

    source_wells = PLAN["assembly_source_wells"]
    for construct in PLAN["assemblies"]:
        chemistry = construct["chemistry"]
        part_volume = chemistry["part_volume_ul"]
        additions = [
            ("reagent:nuclease_free_water", construct["water_volume_ul"]),
            ("reagent:T4_DNA_ligase_buffer", chemistry["buffer_volume_ul"]),
            ("reagent:T4_DNA_ligase", chemistry["ligase_volume_ul"]),
            ("enzyme:" + construct["restriction_enzyme"], chemistry["enzyme_volume_ul"]),
            ("dna:" + construct["backbone"], part_volume),
        ] + [("dna:" + component, part_volume) for component in construct["components"]]
        for destination_name in construct["assembly_wells"]:
            destination = reaction_plate[destination_name]
            for source_name, volume in additions:
                pipette.transfer(
                    volume,
                    sources[source_wells[source_name]],
                    destination,
                    new_tip="always",
                )
            pipette.pick_up_tip()
            pipette.mix(3, 15, destination)
            pipette.drop_tip()

    # Every assembly in a batch shares one thermal profile, so the first
    # construct's chemistry drives the block.
    profile_chemistry = PLAN["assemblies"][0]["chemistry"]
    thermocycler.close_lid()
    thermocycler.set_lid_temperature(105)
    thermocycler.execute_profile(
        steps=[
            {
                "temperature": profile_chemistry["digest_temperature_c"],
                "hold_time_minutes": profile_chemistry["digest_minutes"],
            },
            {
                "temperature": profile_chemistry["ligate_temperature_c"],
                "hold_time_minutes": profile_chemistry["ligate_minutes"],
            },
        ],
        repetitions=profile_chemistry["cycles"],
        block_max_volume=profile_chemistry["reaction_volume_ul"],
    )
    thermocycler.set_block_temperature(50, hold_time_minutes=5)
    thermocycler.set_block_temperature(80, hold_time_minutes=10)
    thermocycler.set_block_temperature(4)
    thermocycler.deactivate_lid()
    thermocycler.open_lid()
    protocol.comment(
        "Assembly complete. Preserve the reaction plate for transformation_protocol.py."
    )
