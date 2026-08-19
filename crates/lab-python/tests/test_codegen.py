"""The Python mirror of the standard library, against the compiler's catalog.

The mirror is checked in so that an editor and a typechecker can see it without
running the generator. That only works if it is regenerated when the standard
library changes, which is what this asserts.
"""

import unittest
from pathlib import Path

from lab import codegen
from lab.bio import designs, golden_gate

ROOT = Path(codegen.__file__).resolve().parent


class CodegenTests(unittest.TestCase):
    def test_the_checked_in_mirror_is_current(self) -> None:
        stale = [
            module.path
            for module in codegen.generate()
            if not (ROOT / module.path).exists()
            or (ROOT / module.path).read_text() != module.source
        ]

        self.assertEqual(stale, [], "run `python -m lab.codegen` to regenerate the mirror")

    def test_a_kind_carries_the_word_its_declarations_are_written_with(self) -> None:
        self.assertEqual(designs.RestrictionEnzyme.word, "restriction_enzyme")
        self.assertEqual(designs.RestrictionEnzyme.produces, "RestrictionEnzyme")

    def test_a_kind_carries_every_module_using_it_has_to_import(self) -> None:
        # Golden Gate contributes reaction chemistry to a plasmid declared by
        # `std.bio.designs`, so a design built this way imports both.
        self.assertEqual(designs.Plasmid.uses, ("std.bio.designs",))
        self.assertEqual(golden_gate.Plasmid.uses, ("std.bio.designs", "std.bio.golden_gate"))

    def test_a_kind_knows_the_properties_its_module_contributes(self) -> None:
        self.assertIn("sequence", designs.Plasmid.properties)
        self.assertIn("reaction_volume", golden_gate.Plasmid.properties)


if __name__ == "__main__":
    unittest.main()
