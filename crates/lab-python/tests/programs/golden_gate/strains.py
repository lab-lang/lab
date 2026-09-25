"""Separate GFP and RFP transformations, each with its own DH5alpha cell supply."""

import lab
from lab.bio.golden_gate import Strain
from lab.units import minutes, uL

from .inventory import DH5alpha, chloramphenicol
from .plasmids import GVD0011, GVD0013

module = lab.Module("golden_gate.designs.strains", doc=__doc__)

GFP_strain = Strain.build(
    doc="Three GFP transformation replicates.",
    sbol_identity="https://SBOL2Build.org/GFP_strain",
    chassis=DH5alpha,
    plasmids=[GVD0011],
    selection=chloramphenicol,
    transformation_replicates=3,
    plating_replicates=1,
    serial_dilutions=2,
    cell_volume=20 * uL,
    dna_volume=5 * uL,
    recovery_aliquot_volume=1200 * uL,
    recovery_volume=60 * uL,
    heat_shock_duration=1 * minutes,
    medium_volume=18 * uL,
    culture_volume=2 * uL,
    colony_volume=4 * uL,
)

RFP_strain = Strain.build(
    doc="Six RFP transformation replicates.",
    sbol_identity="https://SBOL2Build.org/RFP_strain",
    chassis=DH5alpha,
    plasmids=[GVD0013],
    selection=chloramphenicol,
    transformation_replicates=6,
    plating_replicates=1,
    serial_dilutions=2,
    cell_volume=20 * uL,
    dna_volume=5 * uL,
    recovery_aliquot_volume=1200 * uL,
    recovery_volume=60 * uL,
    heat_shock_duration=1 * minutes,
    medium_volume=18 * uL,
    culture_volume=2 * uL,
    colony_volume=4 * uL,
)
