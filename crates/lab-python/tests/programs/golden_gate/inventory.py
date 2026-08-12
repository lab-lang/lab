"""External inventory identities, kept apart from the designs that use them.

A typed symbol here says only that a name refers to a catalogued item. It is
not a claim that a suitable lot is on the shelf; that remains an inventory
resolution and a runtime evidence question.
"""

import lab
from lab.bio.designs import Antibiotic, Backbone, Chassis, Part, RestrictionEnzyme
from lab.units import C, minutes

module = lab.Module("golden_gate.designs.inventory", doc=__doc__)

# Constitutive promoters of differing strength, the shared ribosome binding
# site and terminator, and the fluorescent reporters.
J23101 = Part.buy()
J23106 = Part.buy()
B0034 = Part.buy()
B0015 = Part.buy()
GFP = Part.buy()
RFP = Part.buy()

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
