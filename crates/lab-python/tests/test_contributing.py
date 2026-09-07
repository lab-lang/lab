"""Contributors consume generated bindings with runtime and static checking."""

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
EXAMPLE = ROOT / "examples/contributing/scientific-package"


def environment() -> dict[str, str]:
    env = os.environ.copy()
    bindings = str(EXAMPLE / "bindings/python")
    env["PYTHONPATH"] = os.pathsep.join((bindings, env.get("PYTHONPATH", "")))
    env["MYPYPATH"] = bindings
    return env


def test_generated_package_action_compiles_from_a_python_workflow() -> None:
    result = subprocess.run(
        [sys.executable, str(EXAMPLE / "protocol.py")],
        capture_output=True,
        text=True,
        env=environment(),
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "compiled and refined" in result.stdout


def test_generated_types_infer_declaration_subjects_and_reject_wrong_materials(
    tmp_path: Path,
) -> None:
    source = tmp_path / "consumer.py"
    source.write_text(
        "from typing import assert_type\n"
        "import lab\n"
        "from lab import Material\n"
        "from lab._effects import Effect\n"
        "from lab.bio.designs import Medium, Plasmid\n"
        "from lab.plasmid import Exact, SequenceCheck\n"
        "from contribution_example.science import homogenize\n"
        "broth = Medium.buy(sbol_identity='https://example.org/broth')\n"
        "assert_type(lab.provision(broth), Effect[Material[Medium]])\n"
        "def use(wf: lab.Context, medium: Material[Medium], plasmid: Material[Plasmid]) -> None:\n"
        "    assert_type(wf.perform(homogenize(medium)), Material[Medium])\n"
        "    assert_type(Exact(evidence=[], material=plasmid), SequenceCheck)\n",
        encoding="utf-8",
    )
    command = [sys.executable, "-m", "mypy", "--strict", str(source), str(EXAMPLE / "protocol.py")]
    valid = subprocess.run(command, env=environment(), capture_output=True, text=True, check=False)
    assert valid.returncode == 0, valid.stdout + valid.stderr
    source.write_text(source.read_text() + "    homogenize(plasmid)\n", encoding="utf-8")
    invalid = subprocess.run(
        command, env=environment(), capture_output=True, text=True, check=False
    )
    assert invalid.returncode == 1, invalid.stdout + invalid.stderr
    assert 'incompatible type "Material[Plasmid]"' in invalid.stdout
