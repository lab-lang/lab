"""Execute the emitted OT-2 interpreter against an instrument command recorder."""

from __future__ import annotations

import importlib.util
import sys
import types
import unittest
from pathlib import Path
from unittest.mock import patch

TEMPLATE = (
    Path(__file__).resolve().parents[2]
    / "crates/lab-adapters/src/backend/opentrons/ot2/invocation/pipetting_program.py"
)


class Well:
    def __init__(self, name):
        self.name = name

    def bottom(self, offset):
        return (self.name, "bottom", offset)

    def top(self, offset):
        return (self.name, "top", offset)


class Labware:
    def __getitem__(self, name):
        return Well(name)


class Module:
    def __init__(self):
        self.temperature = None

    def load_labware(self, name):
        return Labware()

    def open_lid(self):
        pass

    def set_temperature(self, value):
        self.temperature = value

    def set_block_temperature(self, value):
        self.temperature = value


class Pipette:
    def __init__(self, maximum):
        self.max_volume = maximum
        self.contents = 0
        self.commands = []
        self.tips = 0

    def pick_up_tip(self):
        self.tips += 1

    def drop_tip(self):
        self.commands.append(("drop", self.contents))
        self.contents = 0

    def aspirate(self, volume, location, rate):
        self.contents += volume
        # A p300 with 200 uL tips has a smaller working volume than its piston.
        assert self.contents <= min(self.max_volume, 200)
        self.commands.append(("aspirate", volume, rate))

    def air_gap(self, volume):
        self.contents += volume
        assert self.contents <= min(self.max_volume, 200)
        self.commands.append(("air_gap", volume))

    def dispense(self, volume, location, rate):
        self.contents -= volume
        assert self.contents >= 0
        self.commands.append(("dispense", volume, rate))

    def blow_out(self, well):
        self.commands.append(("blow_out", self.contents))
        self.contents = 0

    def touch_tip(self, well, **options):
        pass


class Protocol:
    def __init__(self):
        self.modules = []
        self.pipettes = []

    def load_module(self, *args):
        module = Module()
        self.modules.append(module)
        return module

    def load_labware(self, *args):
        return Labware()

    def load_instrument(self, name, *args, **kwargs):
        pipette = Pipette(20 if name == "p20" else 300)
        self.pipettes.append(pipette)
        return pipette

    def comment(self, text):
        pass


def quantity(value):
    return {"value": {"value": str(value)}}


class CanonicalInterpreterTests(unittest.TestCase):
    def test_shared_distribution_clears_air_gaps_and_respects_tip_capacity(self):
        stub = types.ModuleType("opentrons")
        stub.protocol_api = types.SimpleNamespace(ProtocolContext=Protocol)
        spec = importlib.util.spec_from_file_location("canonical_ot2", TEMPLATE)
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {"opentrons": stub}):
            spec.loader.exec_module(module)
        resource = {
            "model": "module",
            "slot": "1",
            "slots": ["2"],
            "labware": "labware",
        }
        technique = {
            "aspiration": {"kind": "liquid"},
            "dispense": {"kind": "above_liquid"},
            "air_gap": quantity(10),
            "blow_out": False,
            "touch_tip": False,
        }
        source = {"vessel": "medium", "position": 0}
        destinations = [{"vessel": "cultures", "position": i} for i in range(4)]
        module.PLAN = {
            "deck": {
                "resources": {
                    name: resource
                    for name in [
                        "sources",
                        "work",
                        "bulk",
                        "surface",
                        "small_tips",
                        "large_tips",
                    ]
                },
                "instruments": {
                    "small": {"model": "p20", "mount": "left"},
                    "large": {"model": "p300", "mount": "right"},
                },
                "techniques": {
                    "aspiration_rate": 0.5,
                    "distribution_aspiration_rate": 1.0,
                    "mix_aspiration_rate": 1.0,
                    "dispense_rate": 1.0,
                    "above_liquid_offset_mm": 2.0,
                },
            },
            "execution": {
                "staging_temperatures": {"work": 4.0},
                "initial_volumes_ul": {"medium": [1200], "cultures": [35] * 4},
                "locations": {
                    "medium": [{"resource": {"kind": "sources"}, "well": "A1"}],
                    "cultures": [
                        {"resource": {"kind": "work"}, "well": f"{row}1"}
                        for row in "ABCD"
                    ],
                },
                "program": {
                    "steps": [
                        {
                            "kind": "distribute",
                            "source": source,
                            "destinations": destinations,
                            "volume_each": quantity(60),
                            "fluid_path": "shared_source_no_reentry",
                            "fluid_path_group": "medium",
                            "technique": technique,
                        }
                    ]
                },
            },
        }
        protocol = Protocol()
        module.run(protocol)
        large = protocol.pipettes[1]
        self.assertEqual(
            [c for c in large.commands if c[0] == "aspirate"],
            [("aspirate", 180.0, 1.0), ("aspirate", 60.0, 1.0)],
        )
        self.assertEqual(
            [c for c in large.commands if c[0] == "dispense"],
            [("dispense", 70.0, 1.0)] * 4,
        )
        self.assertEqual(
            [c for c in large.commands if c[0] == "air_gap"], [("air_gap", 10.0)] * 4
        )
        self.assertEqual(large.commands[-1], ("drop", 0.0))
        self.assertEqual(large.tips, 1)
        self.assertEqual(protocol.modules[1].temperature, 4.0)

        # Each requested blowout must happen only after that destination's liquid
        # is dispensed, with a fresh path before returning to the shared source.
        step = module.PLAN["execution"]["program"]["steps"][0]
        step.pop("fluid_path_group")
        step["technique"]["blow_out"] = True
        protocol = Protocol()
        module.run(protocol)
        large = protocol.pipettes[1]
        self.assertEqual(
            [c for c in large.commands if c[0] == "aspirate"],
            [("aspirate", 60.0, 1.0)] * 4,
        )
        self.assertEqual(
            [c for c in large.commands if c[0] == "blow_out"],
            [("blow_out", 0.0)] * 4,
        )
        self.assertEqual(large.tips, 4)
