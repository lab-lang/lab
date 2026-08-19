"""Two composite transcription units, each a promoter driving a fluorescent
reporter through a shared RBS and terminator.

Sequences are synthetic compiler fixtures, not qualified biological designs.
Each composite sequence is exactly the concatenation of the parts listed
under `components`, in that order, so the design stays true once the compiler
computes an assembled sequence rather than taking one on trust.

The reaction chemistry in each design is scientific intent and travels with
the artifact; where the reaction physically happens is a target profile's
concern.
"""

import lab
from lab import circular, dna
from lab.bio.golden_gate import Plasmid
from lab.units import C, minutes, uL

from .inventory import B0015, B0034, GFP, J23101, J23106, RFP, BsaI, pSB1C3

module = lab.Module("golden_gate.designs.plasmids", doc=__doc__)

composite_plasmid_1 = Plasmid.build(
    doc="""A GFP transcription unit in the pSB1C3 backbone.

    J23101 drives GFP through the shared RBS and terminator, assembled by Golden
    Gate with BsaI. Accepted only if the built sequence matches the design.
    """,
    sequence=dna(
        "TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGCAAAGAGGAGAAAATGACCATGATTACGCCAAGCTTGGTACC"
        "GAGCTCCCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
    ),
    backbone=pSB1C3,
    components=[J23101, B0034, GFP, B0015],
    restriction_enzyme=BsaI,
    assembly_replicates=1,
    reaction_volume=20 * uL,
    part_volume=2 * uL,
    enzyme_volume=2 * uL,
    ligase_volume=4 * uL,
    buffer_volume=2 * uL,
    assembly_cycles=75,
    ligate_temperature=16 * C,
    ligate_duration=5 * minutes,
    require=[lambda plasmid: plasmid.topology == circular],
    accept=[lambda plasmid: plasmid.sequence == plasmid.design.sequence],
)

composite_plasmid_2 = Plasmid.build(
    doc="""An RFP transcription unit in the pSB1C3 backbone.

    Identical in construction to `composite_plasmid_1` but driven by the weaker
    J23106 promoter, so the panel reports two promoter strengths against two
    reporters.
    """,
    sequence=dna(
        "TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGCAAAGAGGAGAAAATGGCCTCCTCCGAGGACGTCATCAAGG"
        "AGTTCATGCCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
    ),
    backbone=pSB1C3,
    components=[J23106, B0034, RFP, B0015],
    restriction_enzyme=BsaI,
    assembly_replicates=1,
    reaction_volume=20 * uL,
    part_volume=2 * uL,
    enzyme_volume=2 * uL,
    ligase_volume=4 * uL,
    buffer_volume=2 * uL,
    assembly_cycles=75,
    ligate_temperature=16 * C,
    ligate_duration=5 * minutes,
    require=[lambda plasmid: plasmid.topology == circular],
    accept=[lambda plasmid: plasmid.sequence == plasmid.design.sequence],
)
