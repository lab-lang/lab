"""External inventory identities, kept apart from the designs that use them.

A typed symbol here says only that a name refers to a catalogued item. It is
not a claim that a suitable lot is on the shelf; that remains an inventory
resolution and a runtime evidence question.
"""

import lab
from lab import dna
from lab.bio.designs import CDS, Antibiotic, Backbone, Chassis, Part, Promoter, RestrictionEnzyme
from lab.units import C, minutes

module = lab.Module("golden_gate.designs.inventory", doc=__doc__)

J23101_sequence = lab.Binding(
    module=module,
    name="J23101_sequence",
    annotation="DNA",
    value=dna("TTGACAGCTAGCTCAGTCCTAGGTATTATGCTAGC"),
)
J23106_sequence = lab.Binding(
    module=module,
    name="J23106_sequence",
    annotation="DNA",
    value=dna("TTTACGGCTAGCTCAGTCCTAGGTATAGTGCTAGC"),
)
B0034_sequence = lab.Binding(
    module=module,
    name="B0034_sequence",
    annotation="DNA",
    value=dna("AAAGAGGAGAAA"),
)
B0015_sequence = lab.Binding(
    module=module,
    name="B0015_sequence",
    annotation="DNA",
    value=dna("CCAGGCATCAAATAAAACGAAAGGCTCAGTCG"),
)
GFP_sequence = lab.Binding(
    module=module,
    name="GFP_sequence",
    annotation="DNA",
    value=dna("ATGACCATGATTACGCCAAGCTTGGTACCGAGCTC"),
)
RFP_sequence = lab.Binding(
    module=module,
    name="RFP_sequence",
    annotation="DNA",
    value=dna("ATGGCCTCCTCCGAGGACGTCATCAAGGAGTTCATG"),
)
for sequence_binding in (
    J23101_sequence,
    J23106_sequence,
    B0034_sequence,
    B0015_sequence,
    GFP_sequence,
    RFP_sequence,
):
    module.declare(sequence_binding)

# Constitutive promoters of differing strength. Each is a promoter rather
# than a bare part, so the compiler knows what it is without being told
# again wherever it is used.
J23101 = Promoter.buy(sequence=J23101_sequence)
J23106 = Promoter.buy(sequence=J23106_sequence)

# The shared ribosome binding site and terminator. Neither has a narrower
# kind here, so both are parts; a package that declares one may say more.
B0034 = Part.buy(sequence=B0034_sequence)
B0015 = Part.buy(sequence=B0015_sequence)

# The fluorescent reporters, each a coding sequence.
GFP = CDS.buy(sequence=GFP_sequence)
RFP = CDS.buy(sequence=RFP_sequence)

# Assembly backbone and the type IIS enzyme that opens it.
pSB1C3 = Backbone.buy()

# BsaI cuts at 37 C; every plasmid it opens digests the same way.
BsaI = RestrictionEnzyme.buy(
    digest_temperature=37 * C,
    digest_duration=2 * minutes,
)

# Host organisms. DH5alpha is a cloning strain; BL21 is an expression strain.
# Both are transformed the way competent cells are: chilled, shocked, recovered.
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
