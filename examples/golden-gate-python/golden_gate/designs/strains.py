"""A single cotransformation in which DH5alpha receives all three GVD plasmids."""

import lab
from lab.bio.golden_gate import Strain
from lab.units import minutes, uL

from .inventory import DH5alpha, chloramphenicol
from .plasmids import GVD0011, GVD0013, GVD0015

module = lab.Module("golden_gate.designs.strains", doc=__doc__)

GVD_strain = Strain.build(
    doc="DH5alpha cotransformed with GVD0011, GVD0013, and GVD0015.",
    sbol_identity="https://SBOL2Build.org/GVD_strain",
    chassis=DH5alpha,
    plasmids=[GVD0011, GVD0013, GVD0015],
    selection=chloramphenicol,
    transformation_replicates=3,
    plating_replicates=1,
    serial_dilutions=2,
    cell_aliquot_volume=150 * uL,
    cell_volume=20 * uL,
    dna_volume=5 * uL,
    recovery_aliquot_volume=1200 * uL,
    recovery_volume=60 * uL,
    heat_shock_duration=1 * minutes,
    medium_volume=18 * uL,
    culture_volume=2 * uL,
    colony_volume=4 * uL,
)
