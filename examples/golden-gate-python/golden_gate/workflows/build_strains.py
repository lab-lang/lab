"""Transform, recover, dilute, and plate one reporter strain."""

import lab
from lab import Material, Plasmid, Strain
from lab.bio.designs import Medium, inoculated
from lab.units import h

from ..designs.inventory import DH5alpha, LB_chloramphenicol_agar

module = lab.Module("golden_gate.workflows.build_strains", doc=__doc__)


@lab.workflow
def build_GVD_strain(
    wf: lab.Context,
    design: Strain,
    plasmid: Material[Plasmid],
) -> tuple[Material[Strain], Material[inoculated[Medium]]]:
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(lab.transform(design, plasmids=[plasmid], cells=cells))
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    agar = wf.perform(lab.provision(LB_chloramphenicol_agar))
    plate = wf.perform(lab.plate(culture, medium=agar))
    return strain, plate
