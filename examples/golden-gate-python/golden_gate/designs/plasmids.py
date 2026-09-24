"""Reporter plasmids for independent GFP and RFP transformations."""

import lab
from lab.bio.golden_gate import Plasmid
from lab.units import C, minutes, uL

from .inventory import B0015, B0034, GFP, J23101, J23106, RFP, BsaI, designs, pSB1C3

module = lab.Module("golden_gate.designs.plasmids", doc=__doc__)


def plasmid_properties(scale: int = 1) -> dict[str, object]:
    return {
        "backbone": pSB1C3,
        "restriction_enzyme": BsaI,
        "assembly_replicates": 1,
        "reaction_volume": 25 * scale * uL,
        "part_volume": 2 * scale * uL,
        "enzyme_volume": 2 * scale * uL,
        "ligase_volume": 4 * scale * uL,
        "buffer_volume": 2 * scale * uL,
        "assembly_cycles": 75,
        "digest_temperature": 42 * C,
        "digest_duration": 2 * minutes,
        "ligate_temperature": 16 * C,
        "ligate_duration": 5 * minutes,
        "lid_temperature": 42 * C,
        "final_digest_temperature": 60 * C,
        "final_digest_duration": 10 * minutes,
        "heat_inactivation_temperature": 80 * C,
        "heat_inactivation_duration": 10 * minutes,
        "hold_temperature": 4 * C,
    }


GVD0011_sequence = designs.dna_sequence(
    elements=(
        "TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGCAAAGAGGAGAAA"
        "ATGACCATGATTACGCCAAGCTTGGTACCGAGCTC"
        "CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
    ),
)
GVD0011_design = designs.plasmid(
    identity="https://SBOL2Build.org/GVD0011",
    components=[J23101, B0034, GFP, B0015],
    sequence=GVD0011_sequence,
)
GVD0011 = Plasmid.build(
    design=GVD0011_design,
    doc="Synthetic Golden Gate fixture named `GVD0011`.",
    properties=plasmid_properties(),
    accept=[lambda built: built.sequence == built.design.sequence],
)

GVD0013_sequence = designs.dna_sequence(
    elements=(
        "TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGCAAAGAGGAGAAA"
        "ATGGCCTCCTCCGAGGACGTCATCAAGGAGTTCATG"
        "CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
    ),
)
GVD0013_design = designs.plasmid(
    identity="https://SBOL2Build.org/GVD0013",
    components=[J23106, B0034, RFP, B0015],
    sequence=GVD0013_sequence,
)
GVD0013 = Plasmid.build(
    design=GVD0013_design,
    doc="Synthetic Golden Gate fixture named `GVD0013`.",
    properties=plasmid_properties(scale=2),
    accept=[lambda built: built.sequence == built.design.sequence],
)
