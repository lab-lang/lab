"""Making competent cells, written in Python.

The same protocol the compiler's own tests build in Lab, expressed through the
SDK: growing cells to a target optical density, chilling them, spinning them
into a pellet, and washing them into cold buffer. Every verb comes from
`std.lab.competence` as a declared action, so the SDK mirrors it without any
Python written by hand for it.
"""

import lab
from lab import Material
from lab.bio.designs import Chassis, competent
from lab.competence import Buffer, centrifuge, chill, grow, resuspend
from lab.units import OD600, C, minutes, mM, rcf

module = lab.Module("competence.protocol", doc=__doc__)

cold_cacl2 = Buffer.buy(identity="SIGMA-C1016", concentration=100 * mM)

DH5a_competent = Chassis.build(
    doc="A cloning strain grown up and washed into competence.",
    heat_shock_temperature=42 * C,
)


@lab.workflow
def prepare(wf: lab.Context) -> Material[competent[Chassis]]:
    """Grow, chill, pellet, and wash a chassis into competence."""
    cells = wf.perform(lab.realize(DH5a_competent))
    wash = wf.perform(lab.provision(cold_cacl2))
    culture = wf.perform(grow(cells, temperature=37 * C, target=0.40 * OD600))
    chilled = wf.perform(chill(culture, duration=10 * minutes))
    pellet = wf.perform(centrifuge(chilled, force=4000 * rcf, duration=10 * minutes))
    ready = wf.perform(resuspend(pellet, buffer=wash))
    return ready
