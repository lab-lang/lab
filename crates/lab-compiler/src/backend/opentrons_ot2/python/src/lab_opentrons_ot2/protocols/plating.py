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
    thermocycler = cast(
        protocol_api.ThermocyclerContext,
        protocol.load_module("thermocycler module gen2"),
    )
    cultures = thermocycler.load_labware("nest_96_wellplate_100ul_pcr_full_skirt")
    tips20 = protocol.load_labware("opentrons_96_filtertiprack_20ul", "9")
    tips200 = protocol.load_labware("opentrons_96_filtertiprack_200ul", "1")
    p20 = protocol.load_instrument("p20_single_gen2", "left", tip_racks=[tips20])
    p300 = protocol.load_instrument("p300_single_gen2", "right", tip_racks=[tips200])
    dilution_plate = protocol.load_labware("nest_96_wellplate_100ul_pcr_full_skirt", "2")
    agar_plate = protocol.load_labware("nest_96_wellplate_100ul_pcr_full_skirt", "5")
    media_rack = protocol.load_labware("opentrons_15_tuberack_falcon_15ml_conical", "4")
    recovery_medium = media_rack["A1"]
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()

    all_dilution_wells = [
        well
        for construct in PLAN["constructs"]
        for layout in construct["plating"]
        for well in layout["dilution_wells"]
    ]
    p300.distribute(
        18,
        recovery_medium,
        [dilution_plate[name] for name in all_dilution_wells],
        disposal_volume=0,
    )

    for construct in PLAN["constructs"]:
        protocol.comment("Plating " + construct["artifact"] + " on " + construct["selection"])
        for layout in construct["plating"]:
            source = cultures[layout["culture_well"]]
            for dilution_index, dilution_name in enumerate(layout["dilution_wells"]):
                dilution = dilution_plate[dilution_name]
                p20.transfer(2, source, dilution, new_tip="always", mix_after=(5, 19))
                for agar_name in layout["agar_wells"][dilution_index]:
                    p20.transfer(4, dilution, agar_plate[agar_name].top(-8), new_tip="always")
                source = dilution

    protocol.comment(
        "Plating complete. Incubate the selective agar under host-appropriate conditions."
    )
