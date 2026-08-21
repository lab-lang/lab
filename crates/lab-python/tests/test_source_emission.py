"""Source emission rejects Python text Lab cannot represent safely."""

import unittest

import lab


class SourceEmissionTests(unittest.TestCase):
    def test_documentation_terminator_is_rejected_before_emitting_invalid_lab(self) -> None:
        module = lab.Module(
            "scratch.docs",
            doc="A literal documentation terminator looks like */ in prose.",
        )

        with self.assertRaises(ValueError) as raised:
            module.source()

        self.assertEqual(
            str(raised.exception),
            "Lab documentation cannot contain '*/'; it closes the documentation block",
        )


if __name__ == "__main__":
    unittest.main()
