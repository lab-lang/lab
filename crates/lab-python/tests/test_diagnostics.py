"""What a rejected program reports.

The compiler checks Lab, so its diagnostics carry spans into the Lab this SDK
emitted. A reader edits Python, so a diagnostic has to arrive with the Python
statement responsible for it as well as the compiler's own excerpt.
"""

import unittest
from inspect import currentframe

import lab
from lab import dna
from lab.bio import golden_gate
from lab.bio.designs import Backbone, Plasmid
from lab.units import ng, uL


class DiagnosticTests(unittest.TestCase):
    def test_a_misspelled_property_is_reported_against_its_python(self) -> None:
        module = lab.Module("scratch.designs", uses=[golden_gate])
        declared = currentframe().f_lineno + 1  # type: ignore[union-attr]
        Plasmid.build(
            module=module,
            name="p_typo",
            sequence=dna("ACGT"),
            reaction_volme=20 * uL,
        )

        with self.assertRaises(lab.LabError) as raised:
            lab.check(module)

        [diagnostic] = raised.exception.diagnostics
        self.assertIn("reaction_volme", diagnostic.message)
        self.assertIn("did you mean 'reaction_volume'?", diagnostic.help)
        assert diagnostic.origin is not None
        self.assertEqual(diagnostic.origin.file, __file__)
        self.assertEqual(diagnostic.origin.line, declared)

    def test_asking_for_no_evidence_is_refused(self) -> None:
        module = lab.Module("scratch.designs")
        Plasmid.build(
            module=module,
            name="p_unbelieved",
            sequence=dna("ACGT"),
            accept=[lambda plasmid: plasmid.concentration >= 100 * ng / uL],
            across=0,
        )

        with self.assertRaises(lab.LabError) as raised:
            lab.check(module)

        [diagnostic] = raised.exception.diagnostics
        self.assertIn("across 0 biological replicates", diagnostic.rendered)

    def test_an_unbound_declaration_says_how_to_name_it(self) -> None:
        module = lab.Module("scratch.designs")
        Backbone.buy(module=module)

        with self.assertRaises(LookupError) as raised:
            module.source()

        self.assertIn("pass name= when declaring it", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
