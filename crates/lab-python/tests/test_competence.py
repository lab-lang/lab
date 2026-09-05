"""The competent-cell protocol checks and refines through the shared compiler.

The program lives in `programs.competence`, written in the object model. This
reads the Lab it emits and drives it through refinement, the same protocol the
compiler's own tests build in Lab.
"""

import unittest

import lab
from programs import competence


class CompetenceProtocolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.source = competence.module.source()

    def test_the_declared_verbs_emit_their_phrases(self) -> None:
        self.assertIn("culture <- grow cells at 37 C to 0.4 OD600", self.source)
        self.assertIn("chilled <- chill culture for 10 min", self.source)
        self.assertIn("pellet <- centrifuge chilled at 4000 rcf for 10 min", self.source)
        self.assertIn("ready <- resuspend pellet in wash", self.source)

    def test_the_protocol_checks_and_refines(self) -> None:
        program = lab.check(competence.module)
        self.assertIn("competence.protocol", program.checked)
        refined = lab.refine(program)
        operations = {choice["source_operation"] for choice in refined.planning_problem["choices"]}
        self.assertIn("std.lab.competence.grow", operations)
        self.assertIn("std.lab.competence.chill", operations)
        self.assertIn("std.lab.competence.centrifuge", operations)
        self.assertIn("std.lab.competence.resuspend", operations)


if __name__ == "__main__":
    unittest.main()
