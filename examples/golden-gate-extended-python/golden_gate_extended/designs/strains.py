"""Four strains carrying built or bought reporter plasmids."""

import lab
from lab.bio.golden_gate import Strain
from lab.units import minutes, uL

from .inventory import BL21, DH5alpha, chloramphenicol
from .plasmids import composite_plasmid_1, composite_plasmid_2, reference_gfp

module = lab.Module("golden_gate_extended.designs.strains", doc=__doc__)

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
    chassis=DH5alpha,
    plasmids=[composite_plasmid_2],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)
expression_strain = Strain.build(
    doc="The GFP reporter carried in the BL21 expression strain.",
    chassis=BL21,
    plasmids=[composite_plasmid_1],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)
reference_strain = Strain.build(
    doc="The ordered reference plasmid carried in BL21.",
    chassis=BL21,
    plasmids=[reference_gfp],
    selection=chloramphenicol,
    transformation_replicates=2,
    plating_replicates=1,
    serial_dilutions=2,
    properties=TRANSFORMATION,
)
