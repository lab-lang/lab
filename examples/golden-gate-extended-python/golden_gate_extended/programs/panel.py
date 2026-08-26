"""Build the four-strain characterization panel and observe its first plate."""

import lab
from lab import Material, Strain

from ..workflows.assemble import (
    assemble_composite_plasmid_1,
    assemble_composite_plasmid_2,
)
from ..workflows.build_strains import (
    build_composite_strain_1,
    build_composite_strain_2,
    build_expression_strain,
    build_reference_strain,
)
from ..workflows.observe import ColonyGrowth, await_colonies

module = lab.Module("golden_gate_extended.programs.panel", doc=__doc__)


@lab.workflow
def main(
    wf: lab.Context,
) -> tuple[Material[Strain], Material[Strain], Material[Strain], Material[Strain]]:
    composite_plasmid_1 = wf.perform(assemble_composite_plasmid_1())
    composite_plasmid_2 = wf.perform(assemble_composite_plasmid_2())
    for_cloning, for_expression = wf.perform(lab.split(composite_plasmid_1))

    cloning_gfp, plate_1 = wf.perform(build_composite_strain_1(for_cloning))
    cloning_rfp, plate_2 = wf.perform(build_composite_strain_2(composite_plasmid_2))
    expressing_gfp, plate_3 = wf.perform(build_expression_strain(for_expression))
    reference, plate_4 = wf.perform(build_reference_strain())

    growth = wf.perform(await_colonies(plate_1))
    match growth:
        case ColonyGrowth.Ready():
            wf.perform(lab.dispose(growth.plate))
        case ColonyGrowth.TimedOut():
            wf.perform(lab.dispose(growth.plate))

    wf.perform(lab.dispose(plate_2))
    wf.perform(lab.dispose(plate_3))
    wf.perform(lab.dispose(plate_4))
    return cloning_gfp, cloning_rfp, expressing_gfp, reference
