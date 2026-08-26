"""The repository's Python examples remain executable compiler inputs."""

import subprocess
import sys
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[3]
EXAMPLES = (
    ("golden-gate-python", "golden_gate", "Checked Golden Gate Python example (6 modules)"),
    (
        "golden-gate-extended-python",
        "golden_gate_extended",
        "Checked extended Golden Gate Python example (9 modules)",
    ),
)


class PythonExampleTests(unittest.TestCase):
    def test_examples_check_as_complete_programs(self) -> None:
        for directory, module, expected in EXAMPLES:
            with self.subTest(example=directory):
                completed = subprocess.run(
                    [sys.executable, "-m", module],
                    cwd=REPOSITORY / "examples" / directory,
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(completed.returncode, 0, completed.stderr)
                self.assertIn(expected, completed.stdout)


if __name__ == "__main__":
    unittest.main()
