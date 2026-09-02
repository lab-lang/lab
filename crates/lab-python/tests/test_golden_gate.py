"""The Golden Gate example, written twice.

`examples/golden-gate/src/designs/` is the hand-written Lab; the package under
`programs/golden_gate/` is the same designs written with this SDK. The two must
compile to the same checked module, which is what makes the SDK a way of
writing Lab rather than a second dialect of it.
"""

import re
import unittest
from pathlib import Path
from typing import Any

import lab
from programs.golden_gate import inventory, plasmids, strains

REPOSITORY = Path(__file__).resolve().parents[3]
DESIGNS = REPOSITORY / "examples" / "golden-gate" / "src" / "designs"

#: The Lab module each Python module stands for, in dependency order.
MODULES = (
    ("golden_gate.designs.inventory", inventory.module, "inventory.lab"),
    ("golden_gate.designs.plasmids", plasmids.module, "plasmids.lab"),
    ("golden_gate.designs.strains", strains.module, "strains.lab"),
)

#: A source declaration's identity carries the byte offset it was declared at,
#: so two spellings of the same program agree on everything but this.
_OFFSET = re.compile(r"^(?P<name>.+)@\d+$")


def normalize(value: Any) -> Any:
    """Checked IR with source positions removed."""

    if isinstance(value, dict):
        if set(value) == {"module", "local"} and isinstance(value["local"], str):
            matched = _OFFSET.match(value["local"])
            if matched:
                return {**value, "local": matched.group("name")}
        return {key: normalize(item) for key, item in value.items()}
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


class GoldenGateTests(unittest.TestCase):
    def test_python_designs_check(self) -> None:
        program = lab.check(*(module for _, module, _ in MODULES))

        self.assertEqual(len(program.checked), 3)

    def test_python_designs_match_the_lab_sources(self) -> None:
        written = lab.check_sources(
            {name: (DESIGNS / file).read_text() for name, _, file in MODULES}
        )
        emitted = lab.check(*(module for _, module, _ in MODULES))

        for name, _, _ in MODULES:
            with self.subTest(module=name):
                self.assertEqual(
                    normalize(emitted.checked[name]),
                    normalize(written.checked[name]),
                )

    def test_declarations_are_named_after_their_python_bindings(self) -> None:
        self.assertEqual(plasmids.GVD0011.name, "GVD0011")
        self.assertEqual(inventory.BsaI.name, "BsaI")


if __name__ == "__main__":
    unittest.main()
