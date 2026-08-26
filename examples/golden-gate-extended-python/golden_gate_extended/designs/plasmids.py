"""Two locally assembled plasmids and one ordered reference plasmid."""

import lab
from lab.bio.golden_gate import Plasmid
from lab.units import kb, ng, uL

from .inventory import BsaI, pSB1C3
from .parts import B0015, B0034, GFP, J23101, J23106, RFP, designs

module = lab.Module("golden_gate_extended.designs.plasmids", doc=__doc__)

GFP_SEQUENCE = (
    "TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGCAAAGAGGAGAAA"
    "ATGACCATGATTACGCCAAGCTTGGTACCGAGCTC"
    "CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
)
RFP_SEQUENCE = (
    "TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGCAAAGAGGAGAAA"
    "ATGGCCTCCTCCGAGGACGTCATCAAGGAGTTCATG"
    "CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
)

gfp_design = designs.plasmid(
    components=[J23101, B0034, GFP, B0015],
    sequence=GFP_SEQUENCE,
    description="The GFP reporter under a strong constitutive promoter.",
)
composite_plasmid_1 = Plasmid.build(
    design=gfp_design,
    backbone=pSB1C3,
    restriction_enzyme=BsaI,
    assembly_replicates=1,
    reaction_volume=20 * uL,
    part_volume=2 * uL,
    enzyme_volume=2 * uL,
    ligase_volume=4 * uL,
    buffer_volume=2 * uL,
    assembly_cycles=75,
    require=[
        lambda plasmid: plasmid.sites(BsaI) == 0,
        lambda plasmid: plasmid.length <= 12 * kb,
    ],
    across=3,
    accept=[
        lambda built: built.sequence == built.design.sequence,
        lambda built: built.concentration >= 100 * ng / uL,
        lab.Claim(lambda built: built.volume >= 20 * uL, across=1),
    ],
)

rfp_design = designs.plasmid(
    components=[J23106, B0034, RFP, B0015],
    sequence=RFP_SEQUENCE,
    description="The RFP reporter under a weaker constitutive promoter.",
)
composite_plasmid_2 = Plasmid.build(
    design=rfp_design,
    backbone=pSB1C3,
    restriction_enzyme=BsaI,
    assembly_replicates=1,
    reaction_volume=20 * uL,
    part_volume=2 * uL,
    enzyme_volume=2 * uL,
    ligase_volume=4 * uL,
    buffer_volume=2 * uL,
    assembly_cycles=75,
    require=[lambda plasmid: plasmid.sites(BsaI) == 0],
    across=3,
    accept=[
        lambda built: built.sequence == built.design.sequence,
        lambda built: built.concentration >= 60 * ng / uL,
    ],
)

reference_design = designs.plasmid(
    identity="reference_gfp",
    sequence=GFP_SEQUENCE,
    description="A reference GFP plasmid ordered from a repository.",
)
reference_gfp = Plasmid.buy(
    design=reference_design,
    identity="Addgene-#134516",
)
