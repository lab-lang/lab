import unittest

from lab import compile_lab_module


class CompilationTests(unittest.TestCase):
    def test_compiles_to_checked_module(self) -> None:
        source = """
plasmid p_python:
  sequence = dna("ACGT")
  accept sequence == design.sequence
        """

        module = compile_lab_module(source)

        self.assertEqual(module["declarations"][0]["name"], "p_python")


if __name__ == "__main__":
    unittest.main()
