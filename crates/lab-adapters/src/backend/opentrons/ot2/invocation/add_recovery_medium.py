"""Requirement-scoped recovery-medium addition emitted by Lab."""

import json
from typing import Any

from opentrons import protocol_api

# The block is brought to a known cold state before the lid opens and plates are handled.
# This is a device state transition, not a scientific setpoint: every temperature the
# science requires comes from the reviewed plan below.
_BLOCK_IDLE_CELSIUS = 4

metadata = {
    "protocolName": "Lab recovery-medium addition",
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


def _destination(
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
    raise ValueError(f"Unsupported recovery-medium dispense strategy: {kind}")


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
    sources = temperature.load_labware(deck["temperature_module"]["labware"])
    thermocycler = protocol.load_module(deck["thermocycler"]["model"])
    culture_plate = thermocycler.load_labware(deck["thermocycler"]["labware"])
    tips = [
        protocol.load_labware(stage["large_tips"]["labware"], slot)
        for slot in stage["large_tips"]["slots"]
    ]
    pipette = protocol.load_instrument(
        profile["instruments"]["large"]["model"],
        profile["instruments"]["large"]["mount"],
        tip_racks=tips,
    )

    thermocycler.set_block_temperature(_BLOCK_IDLE_CELSIUS)
    thermocycler.open_lid()
    medium = sources[execution["medium"]["source_well"]]
    recovery_liquid = protocol.define_liquid(
        name="recovery_medium",
        description=execution["medium"]["symbol"],
        display_color="#D2B48C",
    )
    medium.load_liquid(
        liquid=recovery_liquid, volume=execution["medium"]["load_volume_ul"]
    )
    cultures = [culture_plate[name] for name in execution["culture_wells"]]
    destinations = [
        _destination(well, execution["technique"]["dispense"], techniques)
        for well in cultures
    ]
    air_gap = execution["technique"].get("air_gap")
    pipette.distribute(
        volume=execution["recovery_volume_ul"],
        source=medium,
        dest=destinations,
        disposal_volume=0,
        new_tip="once",
        air_gap=0 if air_gap is None else _quantity_value(air_gap),
    )
    protocol.comment(
        "Recovery medium added above each culture. Keep the plate staged for recovery incubation."
    )
