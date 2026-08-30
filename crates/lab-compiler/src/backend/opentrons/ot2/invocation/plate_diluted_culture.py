"""Requirement-scoped selective plating emitted by Lab."""

import json
from itertools import groupby

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab selective plating",
    "author": "Lab Compiler",
    "description": "One facility-allocated Procedure task",
}
requirements = {
    "robotType": "OT-2",
    "apiLevel": "2.21",  # LAB:API_LEVEL
}
PLAN_JSON = "{}"  # LAB:INVOCATION_PLAN
PLAN = json.loads(PLAN_JSON)


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    stage = profile["stages"]["plating"]
    techniques = profile["techniques"]
    execution = PLAN["execution"]

    dilution_plate_count = 1 + max(
        well["plate"] for well in execution["dilution_wells"]
    )
    agar_plate_count = 1 + max(well["plate"] for well in execution["agar_wells"])
    dilution_plates = [
        protocol.load_labware(stage["dilution_plate"]["labware"], slot)
        for slot in stage["dilution_plate"]["slots"][:dilution_plate_count]
    ]
    agar_plates = [
        protocol.load_labware(stage["agar_plate"]["labware"], slot)
        for slot in stage["agar_plate"]["slots"][:agar_plate_count]
    ]
    small_tips = [
        protocol.load_labware(stage["small_tips"]["labware"], slot)
        for slot in stage["small_tips"]["slots"]
    ]
    pipette = protocol.load_instrument(
        profile["instruments"]["small"]["model"],
        profile["instruments"]["small"]["mount"],
        tip_racks=small_tips,
    )

    for dilution in range(execution["serial_dilutions"]):
        volume = execution["initial_volume_by_dilution_ul"][dilution]
        for replicate in range(execution["culture_replicates"]):
            logical_index = dilution * execution["culture_replicates"] + replicate
            allocation = execution["dilution_wells"][logical_index]
            well = dilution_plates[allocation["plate"]][allocation["well"]]
            culture = protocol.define_liquid(
                name=f"dilution_{dilution + 1}_culture_{replicate + 1}",
                description="Allocated diluted-culture input",
                display_color="#87CEEB",
            )
            well.load_liquid(liquid=culture, volume=volume)

    key = lambda entry: (entry["dilution"], entry["culture_replicate"])
    for (dilution, culture_replicate), entries in groupby(
        execution["plate_map"], key=key
    ):
        entries = list(entries)
        source_allocation = entries[0]["source"]
        source = dilution_plates[source_allocation["plate"]][
            source_allocation["well"]
        ]
        pipette.pick_up_tip()
        for entry in entries:
            destination_allocation = entry["destination"]
            destination = agar_plates[destination_allocation["plate"]][
                destination_allocation["well"]
            ]
            pipette.aspirate(
                execution["colony_volume_ul"],
                source,
                rate=techniques["aspiration_rate"],
            )
            pipette.dispense(
                execution["colony_volume_ul"],
                destination.top(techniques["material_surface_offset_mm"]),
                rate=techniques["dispense_rate"],
            )
            if execution["technique"]["blow_out"]:
                pipette.blow_out()
        pipette.drop_tip()
        protocol.comment(
            f"Plated dilution {dilution}, culture replicate {culture_replicate}"
        )

    protocol.comment(
        "Selective plating complete. The static plate_map.json records every source and destination."
    )
