"""Four engineered organisms: each composite plasmid in each of two chassis.

The same plasmid appearing in two strains is the point. A strain is its own
artifact, so DH5alpha carrying composite_plasmid_1 and BL21 carrying the same
plasmid are two distinct things to build, accept, and store. Neither is a
property of the plasmid.

The heat-shock and plating parameters are shared across all four, because one
robot run holds every strain in a single thermocycler block.
"""

import lab
from lab.bio.golden_gate import Strain
from lab.units import minutes, uL

from .inventory import BL21, DH5alpha, chloramphenicol
from .plasmids import composite_plasmid_1, composite_plasmid_2

module = lab.Module("golden_gate.designs.strains", doc=__doc__)

#: Heat shock, recovery, and plating are the same for every strain in the
#: panel, so the four declarations state one chemistry rather than four.
TRANSFORMATION = {
    "cell_volume": 20 * uL,
    "dna_volume": 2 * uL,
    "recovery_volume": 60 * uL,
    "heat_shock_duration": 1 * minutes,
    "medium_volume": 18 * uL,
    "culture_volume": 2 * uL,
    "colony_volume": 4 * uL,
}

composite_strain_1 = Strain.build(
    doc="The GFP reporter carried in the DH5alpha cloning strain.",
    sbol_identity="https://SBOL2Build.org/composite_strain_1",
    chassis=DH5alpha,
    plasmids=[composite_plasmid_1],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)

composite_strain_2 = Strain.build(
    doc="The RFP reporter carried in the DH5alpha cloning strain.",
    sbol_identity="https://SBOL2Build.org/composite_strain_2",
    chassis=DH5alpha,
    plasmids=[composite_plasmid_2],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)

composite_strain_3 = Strain.build(
    doc="The GFP reporter carried in the BL21 expression strain.",
    sbol_identity="https://SBOL2Build.org/composite_strain_3",
    chassis=BL21,
    plasmids=[composite_plasmid_1],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)

composite_strain_4 = Strain.build(
    doc="The RFP reporter carried in the BL21 expression strain.",
    sbol_identity="https://SBOL2Build.org/composite_strain_4",
    chassis=BL21,
    plasmids=[composite_plasmid_2],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)
