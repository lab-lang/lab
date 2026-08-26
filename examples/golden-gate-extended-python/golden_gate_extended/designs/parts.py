"""Registry DNA parts represented through Lab's typed SBOL authoring layer."""

import lab
from lab import sbol
from lab.bio.designs import CDS, Part, Promoter

module = lab.Module("golden_gate_extended.designs.parts", doc=__doc__)
designs = sbol.Document(namespace="https://synbiohub.org/public/igem")

J23101 = Promoter.buy(
    design=designs.promoter(
        identity="J23101",
        sequence="TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGC",
        description="Anderson constitutive promoter, strong",
    ),
)
J23106 = Promoter.buy(
    design=designs.promoter(
        identity="J23106",
        sequence="TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGC",
        description="Anderson constitutive promoter, medium",
    ),
)
B0034 = Part.buy(
    design=designs.rbs(
        identity="B0034",
        sequence="AAAGAGGAGAAA",
        description="Ribosome binding site",
    ),
)
B0015 = Part.buy(
    design=designs.terminator(
        identity="B0015",
        sequence="CCAGGCATCAAATAAAACGAAAGGCTCAGTCG",
        description="Double terminator",
    ),
)
GFP = CDS.buy(
    design=designs.cds(
        identity="GFP",
        sequence="ATGACCATGATTACGCCAAGCTTGGTACCGAGCTC",
        description="Green fluorescent protein coding sequence",
    ),
)
RFP = CDS.buy(
    design=designs.cds(
        identity="RFP",
        sequence="ATGGCCTCCTCCGAGGACGTCATCAAGGAGTTCATG",
        description="Red fluorescent protein coding sequence",
    ),
)
