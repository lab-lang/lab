"""Allocated Golden Gate transformation and recovery run emitted by Lab."""

import json
from collections import OrderedDict
from typing import Any

from opentrons import protocol_api

# The block is brought to a known cold state before the lid opens and plates are handled.
# This is a device state transition, not a scientific setpoint: every temperature the
# science requires comes from the reviewed plan below.
_BLOCK_IDLE_CELSIUS = 4

metadata = {
    "protocolName": "Lab Golden Gate transformation",
    "author": "Lab Compiler",
    "description": "One reviewed multi-task run on an allocated OT-2",
}
requirements = {
    "robotType": "OT-2",
    "apiLevel": "2.21",  # LAB:API_LEVEL
}
PLAN_JSON = "{}"  # LAB:INVOCATION_PLAN
PLAN = json.loads(PLAN_JSON)


def _quantity_value(quantity: dict[str, Any]) -> float:
    return float(quantity["value"]["value"])


def _aspiration_location(well: Any, strategy: dict[str, Any]) -> Any:
    kind = strategy["kind"]
    if kind == "liquid":
        return well
    if kind == "vessel_bottom":
        return well.bottom(_quantity_value(strategy["offset"]))
    raise ValueError(f"Unsupported transformation aspiration strategy: {kind}")


def _dispense_location(
    well: Any, strategy: dict[str, Any], techniques: dict[str, Any]
) -> Any:
    kind = strategy["kind"]
    if kind == "liquid":
        return well
    if kind == "above_liquid":
        return well.top(techniques["above_liquid_offset_mm"])
    if kind == "vessel_bottom":
        return well.bottom(_quantity_value(strategy["offset"]))
    if kind == "vessel_top":
        return well.top(_quantity_value(strategy["offset"]))
    if kind == "material_surface":
        return well.top(techniques["material_surface_offset_mm"])
    raise ValueError(f"Unsupported transformation dispense strategy: {kind}")


def _finish_technique(
    pipette: Any,
    destination: Any,
    technique: dict[str, Any],
    techniques: dict[str, Any],
) -> None:
    if technique["blow_out"]:
        pipette.blow_out()
    if technique["touch_tip"]:
        pipette.touch_tip(
            destination,
            radius=techniques["touch_tip_radius"],
            v_offset=techniques["touch_tip_vertical_offset_mm"],
            speed=techniques["touch_tip_speed_mm_s"],
        )


def _execute_thermal_program(
    thermocycler: Any, execution: dict[str, Any]
) -> None:
    thermocycler.close_lid()
    if execution["lid_temperature_c"] is not None:
        thermocycler.set_lid_temperature(execution["lid_temperature_c"])
    for stage in execution["profile"]["stages"]:
        thermocycler.execute_profile(
            steps=[
                {
                    "temperature": step["celsius"],
                    "hold_time_seconds": step["hold_seconds"],
                }
                for step in stage["steps"]
            ],
            repetitions=stage["repeats"],
            block_max_volume=execution["volume_each_ul"],
        )
    if execution["final_hold_celsius"] is not None:
        thermocycler.set_block_temperature(execution["final_hold_celsius"])
    if execution["lid_temperature_c"] is not None:
        thermocycler.deactivate_lid()
    thermocycler.open_lid()


def run(protocol: protocol_api.ProtocolContext) -> None:
    profile = PLAN["deck"]
    deck = profile["deck"]
    stage = profile["stages"]["transformation"]
    techniques = profile["techniques"]
    execution = PLAN["execution"]

    temperature = protocol.load_module(
        deck["temperature_module"]["model"],
        deck["temperature_module"]["slot"],
    )
    cell_rack = temperature.load_labware(deck["temperature_module"]["labware"])
    dna_plate = protocol.load_labware(
        stage["dna_plate"]["labware"], stage["dna_plate"]["slots"][0]
    )
    source_rack = protocol.load_labware(
        stage["source_rack"]["labware"], stage["source_rack"]["slot"]
    )
    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    reaction_plate = thermocycler.load_labware(deck["thermocycler"]["labware"])
    small_tips = [
        protocol.load_labware(stage["small_tips"]["labware"], slot)
        for slot in stage["small_tips"]["slots"]
    ]
    large_tips = [
        protocol.load_labware(stage["large_tips"]["labware"], slot)
        for slot in stage["large_tips"]["slots"]
    ]
    small = protocol.load_instrument(
        profile["instruments"]["small"]["model"],
        profile["instruments"]["small"]["mount"],
        tip_racks=small_tips,
    )
    large = protocol.load_instrument(
        profile["instruments"]["large"]["model"],
        profile["instruments"]["large"]["mount"],
        tip_racks=large_tips,
    )

    temperature.set_temperature(execution["cell_staging_temperature_c"])
    thermocycler.set_block_temperature(_BLOCK_IDLE_CELSIUS)
    thermocycler.open_lid()

    preparations = execution["preparations"]
    cell_groups: OrderedDict[str, dict[str, Any]] = OrderedDict()
    for scheduled in preparations:
        preparation = scheduled["execution"]
        key = preparation["cell_source_well"]
        group = cell_groups.setdefault(
            key,
            {
                "load_volume_ul": preparation["cell_source_volume_ul"],
                "cell_volume_ul": preparation["cell_volume_ul"],
                "mix_cycles": preparation["cell_mix_cycles"],
                "mix_volume_ul": preparation["cell_mix_volume_ul"],
                "destinations": [],
            },
        )
        signature = (
            group["cell_volume_ul"],
            group["mix_cycles"],
            group["mix_volume_ul"],
        )
        candidate = (
            preparation["cell_volume_ul"],
            preparation["cell_mix_cycles"],
            preparation["cell_mix_volume_ul"],
        )
        if signature != candidate:
            raise RuntimeError("One competent-cell source has incompatible transfer programs")
        group["load_volume_ul"] = max(
            group["load_volume_ul"], preparation["cell_source_volume_ul"]
        )
        group["destinations"].extend(preparation["reaction_wells"])

    for ordinal, (source_name, group) in enumerate(cell_groups.items(), start=1):
        source = cell_rack[source_name]
        cells = protocol.define_liquid(
            name=f"competent_cells_{ordinal}",
            description="Allocated competent-cell input",
            display_color="#87CEEB",
        )
        source.load_liquid(liquid=cells, volume=group["load_volume_ul"])
        large.distribute(
            volume=group["cell_volume_ul"],
            source=source,
            dest=[reaction_plate[name] for name in group["destinations"]],
            mix_before=(group["mix_cycles"], group["mix_volume_ul"]),
            disposal_volume=0,
            new_tip="once",
        )

    loaded_dna: set[str] = set()
    for scheduled in preparations:
        preparation = scheduled["execution"]
        destinations = [reaction_plate[name] for name in preparation["reaction_wells"]]
        for dna_index, placement in enumerate(preparation["dna"]):
            source = dna_plate[placement["source_well"]]
            if placement["source_well"] not in loaded_dna:
                dna = protocol.define_liquid(
                    name=f"assembly_product_{len(loaded_dna) + 1}",
                    description=placement["symbol"],
                    display_color="#9370DB",
                )
                source.load_liquid(liquid=dna, volume=placement["load_volume_ul"])
                loaded_dna.add(placement["source_well"])
            for destination in destinations:
                small.pick_up_tip()
                small.mix(
                    preparation["dna_mix_cycles"],
                    preparation["dna_mix_volume_ul"],
                    _aspiration_location(
                        source, preparation["dna_mix_technique"]["aspiration"]
                    ),
                )
                small.aspirate(
                    preparation["dna_volume_ul"],
                    _aspiration_location(
                        source, preparation["dna_transfer_technique"]["aspiration"]
                    ),
                    rate=techniques["aspiration_rate"],
                )
                small.dispense(
                    preparation["dna_volume_ul"],
                    _dispense_location(
                        destination,
                        preparation["dna_transfer_technique"]["dispense"],
                        techniques,
                    ),
                    rate=techniques["dispense_rate"],
                )
                _finish_technique(
                    small,
                    destination,
                    preparation["dna_transfer_technique"],
                    techniques,
                )
                for _ in range(preparation["bubble_clear_cycles"]):
                    small.aspirate(
                        preparation["bubble_clear_volume_ul"],
                        _aspiration_location(
                            destination,
                            preparation["bubble_clear_technique"]["aspiration"],
                        ),
                        rate=techniques["dispense_rate"],
                    )
                    small.dispense(
                        preparation["bubble_clear_volume_ul"],
                        _dispense_location(
                            destination,
                            preparation["bubble_clear_technique"]["dispense"],
                            techniques,
                        ),
                        rate=techniques["dispense_rate"],
                    )
                _finish_technique(
                    small,
                    destination,
                    preparation["bubble_clear_technique"],
                    techniques,
                )
                small.drop_tip()

    heat_shocks = execution["heat_shocks"]
    if not heat_shocks:
        raise RuntimeError("The allocated transformation run has no heat-shock program")
    _execute_thermal_program(thermocycler, heat_shocks[0]["execution"])

    recovery_groups: OrderedDict[str, dict[str, Any]] = OrderedDict()
    for scheduled in execution["recovery_additions"]:
        recovery = scheduled["execution"]
        key = recovery["medium"]["source_well"]
        group = recovery_groups.setdefault(
            key,
            {
                "medium": recovery["medium"],
                "volume_ul": recovery["recovery_volume_ul"],
                "technique": recovery["technique"],
                "destinations": [],
            },
        )
        if (
            group["volume_ul"] != recovery["recovery_volume_ul"]
            or group["technique"] != recovery["technique"]
        ):
            raise RuntimeError("One recovery-medium source has incompatible transfer programs")
        group["destinations"].extend(recovery["culture_wells"])

    for ordinal, (source_name, group) in enumerate(recovery_groups.items(), start=1):
        source = source_rack[source_name]
        liquid = protocol.define_liquid(
            name=f"recovery_medium_{ordinal}",
            description=group["medium"]["symbol"],
            display_color="#D2B48C",
        )
        source.load_liquid(
            liquid=liquid, volume=group["medium"]["load_volume_ul"]
        )
        transfer = group["technique"]
        air_gap = transfer.get("air_gap")
        large.distribute(
            volume=group["volume_ul"],
            source=source,
            dest=[
                _dispense_location(reaction_plate[name], transfer["dispense"], techniques)
                for name in group["destinations"]
            ],
            disposal_volume=0,
            new_tip="once",
            air_gap=0 if air_gap is None else _quantity_value(air_gap),
        )

    incubations = execution["recovery_incubations"]
    if not incubations:
        raise RuntimeError("The allocated transformation run has no recovery incubation")
    _execute_thermal_program(thermocycler, incubations[0]["execution"])
    protocol.comment(
        "Transformation complete. Preserve this plate in the thermocycler for the plating run."
    )
