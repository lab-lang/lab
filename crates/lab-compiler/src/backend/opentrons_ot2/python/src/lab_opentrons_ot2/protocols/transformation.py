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
    tips20 = protocol.load_labware("opentrons_96_tiprack_20ul", "2")
    tips200 = protocol.load_labware("opentrons_96_filtertiprack_200ul", "3")
    p20 = protocol.load_instrument("p20_single_gen2", "left", tip_racks=[tips20])
    p300 = protocol.load_instrument("p300_single_gen2", "right", tip_racks=[tips200])
    source_wells = PLAN["transformation_source_wells"]
    temperature.set_temperature(4)
    thermocycler.open_lid()

    for construct in PLAN["constructs"]:
        cells = sources[source_wells["cells:" + construct["host"]]]
        for reaction in construct["transformations"]:
            destination = reaction_plate[reaction["culture_well"]]
            p20.transfer(20, cells, destination, new_tip="always")
            p20.transfer(
                2,
                reaction_plate[reaction["assembly_well"]],
                destination,
                new_tip="always",
                mix_after=(3, 15),
            )

    thermocycler.close_lid()
    thermocycler.set_block_temperature(4, hold_time_minutes=30)
    thermocycler.set_block_temperature(42, hold_time_minutes=1)
    thermocycler.set_block_temperature(4, hold_time_minutes=2)
    thermocycler.open_lid()
    recovery = sources[source_wells["reagent:recovery_medium"]]
    for construct in PLAN["constructs"]:
        for reaction in construct["transformations"]:
            p300.transfer(
                60,
                recovery,
                reaction_plate[reaction["culture_well"]],
                new_tip="always",
            )
    thermocycler.close_lid()
    thermocycler.set_block_temperature(37, hold_time_minutes=60)
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    protocol.comment(
        "Transformation complete. Preserve the reaction plate for plating_protocol.py."
    )
