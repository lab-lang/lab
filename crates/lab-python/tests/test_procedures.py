"""Python reads canonical Procedure programs without weakening their typed quantities."""

from decimal import Decimal
from typing import Any, cast

import pytest
from lab import procedures


def quantity(value: str, unit: str) -> dict[str, object]:
    return {"value": {"type": "real", "value": value}, "unit": unit}


def thermal_program() -> dict[str, object]:
    return {
        "contract": procedures.THERMAL_PROGRAM_V1,
        "body": {
            "load": {
                "input": 0,
                "output": "product",
                "sample_count": 8,
                "volume_each": quantity("20", procedures.MICROLITRE),
            },
            "lid_temperature": quantity("105", procedures.DEGREE_CELSIUS),
            "stages": [
                {
                    "id": "cycle",
                    "repeats": 30,
                    "steps": [
                        {
                            "id": "denature",
                            "temperature": quantity("95", procedures.DEGREE_CELSIUS),
                            "hold": quantity("15", procedures.SECOND),
                            "ramp_rate": quantity("2.5", procedures.DEGREE_CELSIUS_PER_SECOND),
                        }
                    ],
                }
            ],
            "final_hold": quantity("4", procedures.DEGREE_CELSIUS),
        },
    }


def test_thermal_program_preserves_exact_quantities_and_optional_ramp_control() -> None:
    program = procedures.parse_program(thermal_program())

    assert isinstance(program.body, procedures.ThermalProgramV1)
    step = program.body.stages[0].steps[0]
    assert step.temperature.value == Decimal("95")
    assert step.hold.value == Decimal("15")
    assert step.ramp_rate == procedures.TemperatureRampRate(Decimal("2.5"))


def test_program_parsing_fails_closed_on_contracts_and_units() -> None:
    with pytest.raises(ValueError, match="unknown canonical Procedure contract"):
        procedures.parse_program({"contract": "https://example.org/Unknown", "body": {}})

    invalid = thermal_program()
    body = cast(dict[str, Any], invalid["body"])
    load = cast(dict[str, Any], body["load"])
    volume = cast(dict[str, Any], load["volume_each"])
    volume["unit"] = procedures.SECOND
    with pytest.raises(ValueError, match="canonical volume must use unit"):
        procedures.parse_program(invalid)


def test_pipetting_program_exposes_portable_techniques() -> None:
    raw = {
        "contract": procedures.PIPETTING_PROGRAM_V1,
        "body": {
            "vessels": [
                {
                    "id": "medium",
                    "role": {"kind": "material_source", "material": "lb"},
                    "positions": 1,
                    "initial_volume_each": quantity("10000", procedures.MICROLITRE),
                },
                {
                    "id": "dilutions",
                    "role": {"kind": "product", "output": "culture"},
                    "positions": 1,
                },
            ],
            "outputs": [{"id": "culture"}],
            "steps": [
                {
                    "kind": "transfer",
                    "id": "add-medium",
                    "source": {"vessel": "medium", "position": 0},
                    "destination": {"vessel": "dilutions", "position": 0},
                    "volume": quantity("18", procedures.MICROLITRE),
                    "fluid_path": "isolated_destinations",
                    "fluid_path_group": "dilution-one",
                    "technique": {
                        "aspiration": {"kind": "tracked_liquid_surface"},
                        "dispense": {
                            "kind": "vessel_top",
                            "offset": quantity("-8", procedures.MILLIMETRE),
                        },
                        "air_gap": quantity("10", procedures.MICROLITRE),
                        "blow_out": True,
                        "touch_tip": True,
                    },
                }
            ],
            "constraints": {},
        },
    }

    parsed = procedures.parse_program(raw)
    assert isinstance(parsed.body, procedures.PipettingProgramV1)
    vessel = parsed.body.vessels[0]
    assert vessel.initial_volume_each == procedures.Volume(Decimal("10000"))
    step = parsed.body.steps[0]
    assert isinstance(step, procedures.Transfer)
    assert step.fluid_path_group == "dilution-one"
    assert isinstance(step.technique.aspiration, procedures.TrackedLiquidSurfaceAspiration)
    assert step.technique.dispense == procedures.VesselTopDispense(procedures.Length(Decimal("-8")))
    assert step.technique.air_gap == procedures.Volume(Decimal("10"))
    assert step.technique.blow_out
    assert step.technique.touch_tip
