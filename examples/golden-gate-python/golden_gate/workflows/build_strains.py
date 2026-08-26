"""Transform, recover, dilute, and plate each engineered strain."""

import lab
from lab import Material, Plasmid, Plate, Strain
from lab.units import h

from ..designs.inventory import BL21, DH5alpha, chloramphenicol
from ..designs.strains import (
    composite_strain_1,
    composite_strain_2,
    composite_strain_3,
    composite_strain_4,
)

module = lab.Module("golden_gate.workflows.build_strains", doc=__doc__)


@lab.workflow
def build_composite_strain_1(
    wf: lab.Context,
    composite_plasmid_1: Material[Plasmid],
) -> tuple[Material[Strain], Material[Plate]]:
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(
        lab.transform(composite_strain_1, plasmids=[composite_plasmid_1], cells=cells)
    )
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    plate = wf.perform(lab.plate(culture, antibiotic=chloramphenicol))
    return strain, plate


@lab.workflow
def build_composite_strain_2(
    wf: lab.Context,
    composite_plasmid_2: Material[Plasmid],
) -> tuple[Material[Strain], Material[Plate]]:
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(
        lab.transform(composite_strain_2, plasmids=[composite_plasmid_2], cells=cells)
    )
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    plate = wf.perform(lab.plate(culture, antibiotic=chloramphenicol))
    return strain, plate


@lab.workflow
def build_composite_strain_3(
    wf: lab.Context,
    composite_plasmid_1: Material[Plasmid],
) -> tuple[Material[Strain], Material[Plate]]:
    cells = wf.perform(lab.provision(BL21))
    strain, culture = wf.perform(
        lab.transform(composite_strain_3, plasmids=[composite_plasmid_1], cells=cells)
    )
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    plate = wf.perform(lab.plate(culture, antibiotic=chloramphenicol))
    return strain, plate


@lab.workflow
def build_composite_strain_4(
    wf: lab.Context,
    composite_plasmid_2: Material[Plasmid],
) -> tuple[Material[Strain], Material[Plate]]:
    cells = wf.perform(lab.provision(BL21))
    strain, culture = wf.perform(
        lab.transform(composite_strain_4, plasmids=[composite_plasmid_2], cells=cells)
    )
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    plate = wf.perform(lab.plate(culture, antibiotic=chloramphenicol))
    return strain, plate
