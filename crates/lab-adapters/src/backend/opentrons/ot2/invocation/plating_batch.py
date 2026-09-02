"""Allocated Golden Gate dilution and selective-plating run emitted by Lab."""

import json
from typing import Any

from opentrons import protocol_api

# The block is brought to a known cold state before the lid opens and plates are handled.
# This is a device state transition, not a scientific setpoint: every temperature the
# science requires comes from the reviewed plan below.
_BLOCK_IDLE_CELSIUS = 4

metadata = {
    "protocolName": "Lab Golden Gate dilution and plating",
    "author": "Lab Compiler",
    "description": "One reviewed multi-task run on an allocated OT-2",
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


def _well(plates: list[Any], allocation: dict[str, Any]) -> Any:
    return plates[allocation["plate"]][allocation["well"]]


def _column_major_allocation_order(allocation: dict[str, Any]) -> tuple[int, int, int]:
    well = allocation["well"]
    return (allocation["plate"], int(well[1:]), ord(well[0]) - ord("A"))


def _plate_entries(
    plating: dict[str, Any], dilution: int, culture_replicate: int
) -> list[dict[str, Any]]:
    return [
        entry
        for entry in plating["plate_map"]
        if entry["dilution"] == dilution
        and entry["culture_replicate"] == culture_replicate
    ]


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    deck = profile["deck"]
    stage = profile["stages"]["plating"]
    techniques = profile["techniques"]
    execution = PLAN["execution"]
    dilution_programs = execution["dilutions"]
    plating_programs = execution["platings"]
    if not dilution_programs or len(dilution_programs) != len(plating_programs):
        raise RuntimeError("The allocated plating run has mismatched dilution and plating tasks")

    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    cultures = thermocycler.load_labware(deck["thermocycler"]["labware"])
    all_dilution_allocations = sorted(
        (
            allocation
            for scheduled in dilution_programs
            for allocation in scheduled["execution"]["dilution_wells"]
        ),
        key=_column_major_allocation_order,
    )
    all_agar_allocations = [
        allocation
        for scheduled in plating_programs
        for allocation in scheduled["execution"]["agar_wells"]
    ]
    dilution_plate_count = 1 + max(
        allocation["plate"] for allocation in all_dilution_allocations
    )
    agar_plate_count = 1 + max(
        allocation["plate"] for allocation in all_agar_allocations
    )
    dilution_plates = [
        protocol.load_labware(stage["dilution_plate"]["labware"], slot)
        for slot in stage["dilution_plate"]["slots"][:dilution_plate_count]
    ]
    agar_plates = [
        protocol.load_labware(stage["agar_plate"]["labware"], slot)
        for slot in stage["agar_plate"]["slots"][:agar_plate_count]
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

    thermocycler.set_block_temperature(_BLOCK_IDLE_CELSIUS)
    thermocycler.open_lid()
    first_dilution = dilution_programs[0]["execution"]
    for scheduled in dilution_programs[1:]:
        dilution = scheduled["execution"]
        comparable = (
            dilution["medium"]["source_well"],
            dilution["medium_volume_ul"],
            dilution["culture_volume_ul"],
            dilution["mix_cycles"],
            dilution["mix_volume_ul"],
        )
        expected = (
            first_dilution["medium"]["source_well"],
            first_dilution["medium_volume_ul"],
            first_dilution["culture_volume_ul"],
            first_dilution["mix_cycles"],
            first_dilution["mix_volume_ul"],
        )
        if comparable != expected:
            raise RuntimeError("The fused dilution tasks have incompatible programs")

    recovery_medium = media_rack[first_dilution["medium"]["source_well"]]
    medium = protocol.define_liquid(
        name="dilution_medium",
        description=first_dilution["medium"]["symbol"],
        display_color="#D2B48C",
    )
    recovery_medium.load_liquid(
        liquid=medium, volume=first_dilution["medium"]["load_volume_ul"]
    )
    all_dilution_wells = [
        _well(dilution_plates, allocation) for allocation in all_dilution_allocations
    ]
    chunk_size = techniques["tracked_chunk_size"]
    disposal = techniques["distribution_disposal_volume_ul"]
    # The plan states what the source is loaded with, so this run follows that number down rather
    # than asking the instrument. A fresh tip per aspirate keeps the shared source uncontaminated.
    remaining_ul = float(first_dilution["medium"]["load_volume_ul"])
    for offset in range(0, len(all_dilution_wells), chunk_size):
        chunk = all_dilution_wells[offset : offset + chunk_size]
        p300.pick_up_tip()
        p300.distribute(
            first_dilution["medium_volume_ul"],
            _tracked_aspiration_location(
                protocol, recovery_medium, techniques, remaining_ul
            ),
            chunk,
            disposal_volume=disposal,
            new_tip="never",
        )
        p300.drop_tip()
        remaining_ul -= first_dilution["medium_volume_ul"] * len(chunk) + disposal
        for well in chunk:
            well.load_liquid(liquid=medium, volume=first_dilution["medium_volume_ul"])

    culture_ordinal = 0
    for dilution_scheduled, plating_scheduled in zip(
        dilution_programs, plating_programs
    ):
        dilution = dilution_scheduled["execution"]
        plating = plating_scheduled["execution"]
        culture_count = len(dilution["culture_wells"])
        if len(dilution["dilution_wells"]) != 2 * culture_count:
            raise RuntimeError(
                "Interleaved dilution/plating scheduling requires exactly two dilutions"
            )
        for replicate, culture_name in enumerate(dilution["culture_wells"]):
            culture_ordinal += 1
            culture_source = cultures[culture_name]
            culture = protocol.define_liquid(
                name=f"recovered_culture_{culture_ordinal}",
                description=dilution["artifact"],
                display_color="#87CEEB",
            )
            culture_source.load_liquid(
                liquid=culture, volume=dilution["initial_volume_ul"]
            )
            dilution_1 = _well(
                dilution_plates, dilution["dilution_wells"][replicate]
            )
            dilution_2 = _well(
                dilution_plates,
                dilution["dilution_wells"][culture_count + replicate],
            )

            p20.pick_up_tip()
            p20.aspirate(
                dilution["culture_volume_ul"],
                culture_source,
                rate=techniques["aspiration_rate"],
            )
            p20.dispense(
                dilution["culture_volume_ul"],
                dilution_1,
                rate=techniques["dispense_rate"],
            )
            p20.mix(dilution["mix_cycles"], dilution["mix_volume_ul"], dilution_1)
            p20.aspirate(
                dilution["culture_volume_ul"],
                dilution_1,
                rate=techniques["aspiration_rate"],
            )
            p20.dispense(
                dilution["culture_volume_ul"],
                dilution_2,
                rate=techniques["dispense_rate"],
            )
            p20.mix(dilution["mix_cycles"], dilution["mix_volume_ul"], dilution_2)
            for entry in _plate_entries(plating, 1, replicate + 1):
                destination = _well(agar_plates, entry["destination"])
                p20.aspirate(
                    plating["colony_volume_ul"],
                    dilution_1,
                    rate=techniques["aspiration_rate"],
                )
                p20.dispense(
                    plating["colony_volume_ul"],
                    destination.top(techniques["material_surface_offset_mm"]),
                    rate=techniques["dispense_rate"],
                )
                if plating["technique"]["blow_out"]:
                    p20.blow_out()
            p20.drop_tip()

            p20.pick_up_tip()
            for entry in _plate_entries(plating, 2, replicate + 1):
                destination = _well(agar_plates, entry["destination"])
                p20.aspirate(
                    plating["colony_volume_ul"],
                    dilution_2,
                    rate=techniques["aspiration_rate"],
                )
                p20.dispense(
                    plating["colony_volume_ul"],
                    destination.top(techniques["material_surface_offset_mm"]),
                    rate=techniques["dispense_rate"],
                )
                if plating["technique"]["blow_out"]:
                    p20.blow_out()
            p20.drop_tip()

    protocol.comment(
        "Dilution and plating complete. The generated plate_map.json records every spot."
    )
