"""Requirement-scoped chemical-transformation setup emitted by Lab."""

import json
from typing import Any

from opentrons import protocol_api

metadata = {
    "protocolName": "Lab chemical-transformation setup",
    "author": "Lab Compiler",
    "description": "One facility-allocated Procedure task",
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

    temperature.set_temperature(4)
    thermocycler.set_block_temperature(4)
    thermocycler.open_lid()
    reactions = [reaction_plate[name] for name in execution["reaction_wells"]]
    cells = cell_rack[execution["cell_source_well"]]
    cell_liquid = protocol.define_liquid(
        name="competent_cells",
        description="Allocated competent-cell input",
        display_color="#87CEEB",
    )
    cells.load_liquid(liquid=cell_liquid, volume=execution["cell_source_volume_ul"])

    large.distribute(
        volume=execution["cell_volume_ul"],
        source=cells,
        dest=reactions,
        mix_before=(
            execution["cell_mix_cycles"],
            execution["cell_mix_volume_ul"],
        ),
        disposal_volume=0,
        new_tip="once",
    )

    for dna_index, placement in enumerate(execution["dna"]):
        source = dna_plate[placement["source_well"]]
        dna_liquid = protocol.define_liquid(
            name=f"dna_{dna_index + 1}",
            description=placement["symbol"],
            display_color="#9370DB",
        )
        source.load_liquid(liquid=dna_liquid, volume=placement["load_volume_ul"])
        for destination in reactions:
            small.pick_up_tip()
            small.mix(
                execution["dna_mix_cycles"],
                execution["dna_mix_volume_ul"],
                _aspiration_location(
                    source, execution["dna_mix_technique"]["aspiration"]
                ),
            )
            small.aspirate(
                execution["dna_volume_ul"],
                _aspiration_location(
                    source, execution["dna_transfer_technique"]["aspiration"]
                ),
                rate=techniques["aspiration_rate"],
            )
            small.dispense(
                execution["dna_volume_ul"],
                _dispense_location(
                    destination,
                    execution["dna_transfer_technique"]["dispense"],
                    techniques,
                ),
                rate=techniques["dispense_rate"],
            )
            _finish_technique(
                small, destination, execution["dna_transfer_technique"], techniques
            )
            for _ in range(execution["bubble_clear_cycles"]):
                small.aspirate(
                    execution["bubble_clear_volume_ul"],
                    _aspiration_location(
                        destination,
                        execution["bubble_clear_technique"]["aspiration"],
                    ),
                    rate=techniques["dispense_rate"],
                )
                small.dispense(
                    execution["bubble_clear_volume_ul"],
                    _dispense_location(
                        destination,
                        execution["bubble_clear_technique"]["dispense"],
                        techniques,
                    ),
                    rate=techniques["dispense_rate"],
                )
            _finish_technique(
                small, destination, execution["bubble_clear_technique"], techniques
            )
            small.drop_tip()

    protocol.comment(
        "Transformation setup complete. Keep the plate at 4 C for the allocated heat-shock task."
    )
