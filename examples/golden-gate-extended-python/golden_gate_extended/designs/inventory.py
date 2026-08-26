"""Items this laboratory orders rather than makes."""

import lab
from lab.bio.designs import Antibiotic, Backbone, Chassis, RestrictionEnzyme
from lab.units import C, minutes

module = lab.Module("golden_gate_extended.designs.inventory", doc=__doc__)

pSB1C3 = Backbone.buy()
BsaI = RestrictionEnzyme.buy(
    identity="NEB-R0535",
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
