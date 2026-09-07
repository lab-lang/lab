"""Use a package-authored action through generated Python bindings."""

from pathlib import Path

import lab
from contribution_example.science import homogenize
from lab import Material
from lab.bio.designs import Medium

module = lab.Module("contribution_consumer", doc=__doc__)
input_broth = Medium.buy(sbol_identity="https://example.org/materials/broth")


@lab.workflow
def main(wf: lab.Context) -> Material[Medium]:
    sample = wf.perform(lab.provision(input_broth))
    prepared = wf.perform(homogenize(sample))
    return prepared


if __name__ == "__main__":
    project = Path(__file__).parent
    program = lab.check(module, project=project)
    refined = lab.refine(program, entry_module=module.name, project=project)
    assert any(
        choice["source_operation"] == "contribution_example.science.homogenize"
        for choice in refined.planning_problem["choices"]
    )
    print("Package action compiled and refined through its pipetting Method.")
