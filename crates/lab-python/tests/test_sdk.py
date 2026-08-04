import unittest

from lab import compile_lab_lang


class CompilationTests(unittest.TestCase):
    def test_compiles_to_ir_and_python_plan(self) -> None:
        source = """
plasmid p_python:
  sequence = dna("ACGT")
  accept sequence == design.sequence
        """

        compilation = compile_lab_lang(source)

        self.assertIn("protocol.accept", compilation.ir)
        self.assertEqual(compilation.plan["artifact"], "p_python")
        self.assertGreater(len(compilation.plan["steps"]), 0)


if __name__ == "__main__":
    unittest.main()
