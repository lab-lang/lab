"""External inventory identities kept apart from the designs that use them."""

import lab
from lab import sbol
from lab.bio.designs import (
    CDS,
    Antibiotic,
    Backbone,
    Chassis,
    Part,
    Promoter,
    RestrictionEnzyme,
)
from lab.units import C, minutes

module = lab.Module("golden_gate.designs.inventory", doc=__doc__)
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

pSB1C3 = Backbone.buy()
BsaI = RestrictionEnzyme.buy(
    digest_temperature=37 * C,
    digest_duration=2 * minutes,
)
DH5alpha = Chassis.buy(
    heat_shock_temperature=42 * C,
    cold_incubation=30 * minutes,
    recovery_temperature=37 * C,
    recovery_duration=60 * minutes,
)
BL21 = Chassis.buy(
    heat_shock_temperature=42 * C,
    cold_incubation=30 * minutes,
    recovery_temperature=37 * C,
    recovery_duration=60 * minutes,
)
chloramphenicol = Antibiotic.buy()
