"""Build three GFP and six RFP transformation replicates with separate cell supplies."""

import lab
from lab import Material, Strain

from ..designs.strains import GFP_strain, RFP_strain
from ..workflows.assemble import assemble_GVD0011, assemble_GVD0013
from ..workflows.build_strains import build_GVD_strain

module = lab.Module("golden_gate.programs.reporter_panel", doc=__doc__)


@lab.workflow
def main(wf: lab.Context) -> tuple[Material[Strain], Material[Strain]]:
    GVD0011 = wf.perform(assemble_GVD0011())
    GVD0013 = wf.perform(assemble_GVD0013())
    gfp, gfp_plate = wf.perform(build_GVD_strain(GFP_strain, GVD0011))
    rfp, rfp_plate = wf.perform(build_GVD_strain(RFP_strain, GVD0013))
    wf.perform(lab.dispose(gfp_plate))
    wf.perform(lab.dispose(rfp_plate))
    return gfp, rfp
