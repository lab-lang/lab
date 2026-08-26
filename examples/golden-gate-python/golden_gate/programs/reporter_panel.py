"""Build the four-strain reporter panel."""

import lab
from lab import Material, Strain

from ..workflows.assemble import (
    assemble_composite_plasmid_1,
    assemble_composite_plasmid_2,
)
from ..workflows.build_strains import (
    build_composite_strain_1,
    build_composite_strain_2,
    build_composite_strain_3,
    build_composite_strain_4,
)

module = lab.Module("golden_gate.programs.reporter_panel", doc=__doc__)


@lab.workflow
def main(
    wf: lab.Context,
) -> tuple[Material[Strain], Material[Strain], Material[Strain], Material[Strain]]:
    composite_plasmid_1 = wf.perform(assemble_composite_plasmid_1())
    composite_plasmid_2 = wf.perform(assemble_composite_plasmid_2())

    for_dh5alpha_1, for_bl21_1 = wf.perform(lab.split(composite_plasmid_1))
    for_dh5alpha_2, for_bl21_2 = wf.perform(lab.split(composite_plasmid_2))

    strain_1, plate_1 = wf.perform(build_composite_strain_1(for_dh5alpha_1))
    strain_2, plate_2 = wf.perform(build_composite_strain_2(for_dh5alpha_2))
    strain_3, plate_3 = wf.perform(build_composite_strain_3(for_bl21_1))
    strain_4, plate_4 = wf.perform(build_composite_strain_4(for_bl21_2))

    wf.perform(lab.dispose(plate_1))
    wf.perform(lab.dispose(plate_2))
    wf.perform(lab.dispose(plate_3))
    wf.perform(lab.dispose(plate_4))
    return strain_1, strain_2, strain_3, strain_4
