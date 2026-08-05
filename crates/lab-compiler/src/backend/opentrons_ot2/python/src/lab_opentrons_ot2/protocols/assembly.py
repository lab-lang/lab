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
    temperature = cast(
        protocol_api.TemperatureModuleContext,
        protocol.load_module("temperature module gen2", "1"),
    )
    sources = temperature.load_labware("opentrons_24_aluminumblock_nest_1.5ml_snapcap")
    thermocycler = cast(
        protocol_api.ThermocyclerContext,
        protocol.load_module("thermocycler module gen2"),
    )
    reaction_plate = thermocycler.load_labware("nest_96_wellplate_100ul_pcr_full_skirt")
    tips = protocol.load_labware("opentrons_96_tiprack_20ul", "2")
    pipette = protocol.load_instrument("p20_single_gen2", "left", tip_racks=[tips])
    temperature.set_temperature(4)
    thermocycler.open_lid()

    source_wells = PLAN["assembly_source_wells"]
    for construct in PLAN["constructs"]:
        additions = [
            ("reagent:nuclease_free_water", construct["water_volume_ul"]),
            ("reagent:T4_DNA_ligase_buffer", 2),
            ("reagent:T4_DNA_ligase", 4),
            ("enzyme:" + construct["restriction_enzyme"], 2),
            ("dna:" + construct["backbone"], 2),
        ] + [("dna:" + component, 2) for component in construct["components"]]
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

    thermocycler.close_lid()
    thermocycler.set_lid_temperature(105)
    thermocycler.execute_profile(
        steps=[
            {"temperature": 37, "hold_time_minutes": 2},
            {"temperature": 16, "hold_time_minutes": 5},
        ],
        repetitions=75,
        block_max_volume=20,
    )
    thermocycler.set_block_temperature(50, hold_time_minutes=5)
    thermocycler.set_block_temperature(80, hold_time_minutes=10)
    thermocycler.set_block_temperature(4)
    thermocycler.deactivate_lid()
    thermocycler.open_lid()
    protocol.comment(
        "Assembly complete. Preserve the reaction plate for transformation_protocol.py."
    )
