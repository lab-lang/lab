"""Build the three-plasmid cotransformation."""

import lab
from lab import Material, Strain

from ..workflows.assemble import assemble_GVD0011, assemble_GVD0013, assemble_GVD0015
from ..workflows.build_strains import build_GVD_strain

module = lab.Module("golden_gate.programs.reporter_panel", doc=__doc__)


@lab.workflow
def main(wf: lab.Context) -> Material[Strain]:
    GVD0011 = wf.perform(assemble_GVD0011())
    GVD0013 = wf.perform(assemble_GVD0013())
    GVD0015 = wf.perform(assemble_GVD0015())
    strain, plate = wf.perform(build_GVD_strain(GVD0011, GVD0013, GVD0015))
    wf.perform(lab.dispose(plate))
    return strain
