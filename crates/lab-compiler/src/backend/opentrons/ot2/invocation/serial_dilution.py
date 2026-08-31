"""Requirement-scoped serial dilution emitted by Lab."""

import json
from typing import Any

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


def _tracked_aspiration_location(
    protocol: protocol_api.ProtocolContext,
    source: Any,
    techniques: dict[str, Any],
    remaining_ul: float,
) -> Any:
    """Aspiration position for a source whose falling surface the plan already accounts for.

    `remaining_ul` is the reviewed plan's stated source load less the withdrawals this protocol has
    made, so the position is a function of the plan and the calibrated geometry below. The
    instrument is never asked what it currently holds: a run that consulted live state could reach
    a different position than the one that was reviewed.
    """
    if remaining_ul < source.max_volume * techniques["tracked_low_volume_fraction"]:
        protocol.comment(
            "Planned dilution-medium volume is low; using the labware default aspiration location"
        )
        return source
    usable_depth = source.depth - techniques["tracked_usable_depth_offset_mm"]
    liquid_height = (remaining_ul / source.max_volume) * usable_depth
    aspiration_height = max(
        liquid_height - techniques["tracked_meniscus_offset_mm"],
        techniques["tracked_minimum_height_mm"],
    )
    protocol.comment(
        f"Tracked dilution medium: {remaining_ul:.0f} uL planned remaining, aspirating at {aspiration_height:.1f} mm"
    )
    return source.bottom(aspiration_height)


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    deck = profile["deck"]
    stage = profile["stages"]["plating"]
    techniques = profile["techniques"]
    execution = PLAN["execution"]

    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    cultures = thermocycler.load_labware(deck["thermocycler"]["labware"])
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    dilution_plate_count = 1 + max(
        well["plate"] for well in execution["dilution_wells"]
    )
    dilution_plates = [
        protocol.load_labware(stage["dilution_plate"]["labware"], slot)
        for slot in stage["dilution_plate"]["slots"][:dilution_plate_count]
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
    medium_liquid = protocol.define_liquid(
        name="dilution_medium",
        description=execution["medium"]["symbol"],
        display_color="#D2B48C",
    )
    recovery_medium.load_liquid(
        liquid=medium_liquid, volume=execution["medium"]["load_volume_ul"]
    )
    chunk_size = techniques["tracked_chunk_size"]
    disposal = techniques["distribution_disposal_volume_ul"]
    # The plan states what the source is loaded with, so this run follows that number down rather
    # than asking the instrument. A fresh tip per aspirate keeps the shared source uncontaminated.
    remaining_ul = float(execution["medium"]["load_volume_ul"])
    for offset in range(0, len(dilution_wells), chunk_size):
        chunk = dilution_wells[offset : offset + chunk_size]
        p300.pick_up_tip()
        p300.distribute(
            execution["medium_volume_ul"],
            _tracked_aspiration_location(
                protocol, recovery_medium, techniques, remaining_ul
            ),
            chunk,
            disposal_volume=disposal,
            new_tip="never",
        )
        p300.drop_tip()
        remaining_ul -= execution["medium_volume_ul"] * len(chunk) + disposal
        for well in chunk:
            well.load_liquid(liquid=medium_liquid, volume=execution["medium_volume_ul"])

    culture_replicates = len(execution["culture_wells"])
    serial_dilutions = len(dilution_wells) // culture_replicates
    culture_sources = [cultures[name] for name in execution["culture_wells"]]
    for replicate, culture_source in enumerate(culture_sources):
        culture_liquid = protocol.define_liquid(
            name=f"recovered_culture_{replicate + 1}",
            description=f"Allocated recovered-culture replicate {replicate + 1}",
            display_color="#87CEEB",
        )
        culture_source.load_liquid(
            liquid=culture_liquid, volume=execution["initial_volume_ul"]
        )
        p20.pick_up_tip()
        source = culture_source
        for dilution in range(serial_dilutions):
            destination = dilution_wells[dilution * culture_replicates + replicate]
            p20.aspirate(
                execution["culture_volume_ul"],
                source,
                rate=techniques["aspiration_rate"],
            )
            p20.dispense(
                execution["culture_volume_ul"],
                destination,
                rate=techniques["dispense_rate"],
            )
            p20.mix(execution["mix_cycles"], execution["mix_volume_ul"], destination)
            source = destination
        p20.drop_tip()

    protocol.comment(
        "Serial dilution complete. Preserve every allocated dilution well for downstream plating."
    )
