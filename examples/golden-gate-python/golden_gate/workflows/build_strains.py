"""Cotransform, recover, dilute, and plate the engineered strain."""

import lab
from lab.bio.designs import Medium, inoculated
from lab import Material, Plasmid
Strain
from lab.units import h

from ..designs.inventory import DH5alpha, chloramphenicol
from ..designs.strains import GVD_strain

module = lab.Module("golden_gate.workflows.build_strains", doc=__doc__)


@lab.workflow
def build_GVD_strain(
    wf: lab.Context,
    GVD0011: Material[Plasmid],
    GVD0013: Material[Plasmid],
    GVD0015: Material[Plasmid],
) -> tuple[Material[Strain], Material[inoculated[Medium]]]:
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(
        lab.transform(GVD_strain, plasmids=[GVD0011, GVD0013, GVD0015], cells=cells)
    )
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    agar = wf.perform(lab.provision(LB_chloramphenicol_agar))
    plate = wf.perform(lab.plate(culture, medium=agar))
    return strain, plate
