"""Three plasmids for a single three-way cotransformation.

The sequences below are synthetic compiler fixtures, not qualified
biological designs.
Each sequence is exactly the concatenation of its declared parts.
"""

import lab
from lab import circular, dna
from lab.bio.golden_gate import Plasmid
from lab.units import C, minutes, uL

from .inventory import B0015, B0034, GFP, J23101, J23106, RFP, BsaI, pSB1C3

module = lab.Module("golden_gate.designs.plasmids", doc=__doc__)


def sequence_binding(name: str, sequence: str) -> lab.Binding:
    binding = lab.Binding(module=module, name=name, annotation="DNA", value=dna(sequence))
    module.declare(binding)
    return binding


GVD0011_sequence = sequence_binding(
    "GVD0011_sequence",
    "TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGCAAAGAGGAGAAAATGACCATGATTACGCCAAGCTTGGTACC"
    "GAGCTCCCAGGCATCAAATAAAACGAAAGGCTCAGTCG",
)
GVD0013_sequence = sequence_binding(
    "GVD0013_sequence",
    "TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGCAAAGAGGAGAAAATGGCCTCCTCCGAGGACGTCATCAAGG"
    "AGTTCATGCCAGGCATCAAATAAAACGAAAGGCTCAGTCG",
)
GVD0015_sequence = sequence_binding(
    "GVD0015_sequence",
    "TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGCAAAGAGGAGAAAATGACCATGATTACGCCAAGCTTGGTACC"
    "GAGCTCCCAGGCATCAAATAAAACGAAAGGCTCAGTCG",
)


def build_plasmid(
    name: str,
    documentation: str,
    sequence: lab.Binding,
    components: list[object],
) -> lab.BuildDeclaration[Plasmid]:
    return Plasmid.build(
        name=name,
        doc=documentation,
        sbol_identity=f"https://SBOL2Build.org/{name}",
        sequence=sequence,
        backbone=pSB1C3,
        components=components,
        restriction_enzyme=BsaI,
        assembly_replicates=1,
        reaction_volume=25 * uL,
        part_volume=2 * uL,
        enzyme_volume=2 * uL,
        ligase_volume=4 * uL,
        buffer_volume=2 * uL,
        assembly_cycles=75,
        digest_temperature=42 * C,
        digest_duration=2 * minutes,
        ligate_temperature=16 * C,
        ligate_duration=5 * minutes,
        lid_temperature=42 * C,
        final_digest_temperature=60 * C,
        final_digest_duration=10 * minutes,
        heat_inactivation_temperature=80 * C,
        heat_inactivation_duration=10 * minutes,
        hold_temperature=4 * C,
        require=[lambda plasmid: plasmid.topology == circular],
        accept=[lambda plasmid: plasmid.sequence == plasmid.design.sequence],
    )


GVD0011 = build_plasmid(
    "GVD0011",
    "Synthetic Golden Gate fixture named `GVD0011`.",
    GVD0011_sequence,
    [J23101, B0034, GFP, B0015],
)
GVD0013 = build_plasmid(
    "GVD0013",
    "Synthetic Golden Gate fixture named `GVD0013`.",
    GVD0013_sequence,
    [J23106, B0034, RFP, B0015],
)
GVD0015 = build_plasmid(
    "GVD0015",
    "Synthetic Golden Gate fixture named `GVD0015`.",
    GVD0015_sequence,
    [J23106, B0034, GFP, B0015],
)
