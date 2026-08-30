from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "compare_pudu_golden_gate.py"
SPEC = importlib.util.spec_from_file_location("compare_pudu_golden_gate", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot import {SCRIPT}")
comparison = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = comparison
SPEC.loader.exec_module(comparison)


class ComparisonNormalizationTests(unittest.TestCase):
    def test_sbol2_normalization_removes_only_the_pinned_terminal_version(self) -> None:
        value = {
            "product": "https://SBOL2Build.org/composite_plasmid_1/1",
            "nested": [
                "https://sbolcanvas.org/J23101/1",
                "https://sbolcanvas.org/J23101/10",
                "not-an-iri/1",
            ],
        }

        self.assertEqual(
            comparison.strip_sbol2_version(value),
            {
                "product": "https://SBOL2Build.org/composite_plasmid_1",
                "nested": [
                    "https://sbolcanvas.org/J23101",
                    "https://sbolcanvas.org/J23101/10",
                    "not-an-iri/1",
                ],
            },
        )

    def test_staging_wells_compare_by_material_identity(self) -> None:
        pudu = """\
Picking up tip from A1 of Opentrons OT-2 96 Tip Rack 20 µL on slot 2
Aspirating 2.0 uL from A1 of Opentrons 24 Well Aluminum Block with NEST 1.5 mL Snapcap on Temperature Module GEN1 on slot 1 at 3.78 uL/sec
Dispensing 2.0 uL into A1 of NEST 96 Well Plate 100 µL PCR Full Skirt on Thermocycler Module GEN1 on slot 7 at 7.56 uL/sec
Blowing out at A1 of NEST 96 Well Plate 100 µL PCR Full Skirt on Thermocycler Module GEN1 on slot 7
Touching tip
Dropping tip into Trash Bin on slot 12
"""
        lab = pudu.replace(
            "from A1 of Opentrons 24", "from B3 of Opentrons 24"
        ).replace("Temperature Module GEN1", "Temperature Module GEN2")

        self.assertEqual(
            comparison.normalize_liquid_trace(
                pudu,
                stage="assembly",
                staging={"A1": "nuclease_free_water"},
            ),
            comparison.normalize_liquid_trace(
                lab,
                stage="assembly",
                staging={"B3": "nuclease_free_water"},
            ),
        )

    def test_thermal_trace_normalizes_seconds_and_minutes(self) -> None:
        trace = """\
Setting Temperature Module temperature to 4.0 °C (rounded off to nearest integer)
Setting Thermocycler lid temperature to 42.0 °C
Thermocycler starting 1 repetitions of cycle composed of the following steps: [{'temperature': 60, 'hold_time_minutes': 10}, {'temperature': 80, 'hold_time_seconds': 600}]
Opening Thermocycler lid
"""

        self.assertEqual(
            comparison.normalize_thermal_trace(trace),
            {
                "temperature_module_setpoints_c": [4],
                "thermocycler_block_setpoints_c": [],
                "thermocycler_lid_setpoints_c": [42],
                "profiles": [
                    {
                        "repeats": 1,
                        "steps": [
                            {"temperature_c": 60, "hold_seconds": 600},
                            {"temperature_c": 80, "hold_seconds": 600},
                        ],
                    }
                ],
                "thermocycler_lid_opens": 1,
                "thermocycler_lid_closes": 0,
            },
        )


if __name__ == "__main__":
    unittest.main()
