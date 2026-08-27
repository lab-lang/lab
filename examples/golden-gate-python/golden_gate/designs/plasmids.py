"""Two composite transcription units assembled through Golden Gate."""

import lab
from lab.bio.golden_gate import Plasmid
from lab.units import C, minutes, uL

from .inventory import (
    B0015,
    B0034,
    GFP,
    J23101,
    J23106,
    RFP,
    BsaI,
    designs,
    pSB1C3,
)

module = lab.Module("golden_gate.designs.plasmids", doc=__doc__)

composite_plasmid_1_sequence = designs.dna_sequence(
    elements=(
        "TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGCAAAGAGGAGAAA"
        "ATGACCATGATTACGCCAAGCTTGGTACCGAGCTC"
        "CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
    ),
)
composite_plasmid_1_design = designs.plasmid(
    components=[J23101, B0034, GFP, B0015],
    sequence=composite_plasmid_1_sequence,
    description="A GFP transcription unit in the pSB1C3 backbone.",
)
composite_plasmid_1 = Plasmid.build(
    design=composite_plasmid_1_design,
    backbone=pSB1C3,
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
    accept=[lambda built: built.sequence == built.design.sequence],
)

composite_plasmid_2_sequence = designs.dna_sequence(
    elements=(
        "TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGCAAAGAGGAGAAA"
        "ATGGCCTCCTCCGAGGACGTCATCAAGGAGTTCATG"
        "CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"
    ),
)
composite_plasmid_2_design = designs.plasmid(
    components=[J23106, B0034, RFP, B0015],
    sequence=composite_plasmid_2_sequence,
    description="An RFP transcription unit in the pSB1C3 backbone.",
)
composite_plasmid_2 = Plasmid.build(
    design=composite_plasmid_2_design,
    backbone=pSB1C3,
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
    accept=[lambda built: built.sequence == built.design.sequence],
)
