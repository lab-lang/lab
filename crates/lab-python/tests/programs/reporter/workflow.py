"""Assemble the reporter, transform it, and plate what recovers."""

import lab
from lab import Material, Plate
from lab.bio.designs import Antibiotic, Chassis, Strain, competent
from lab.units import C, h, minutes

from .plasmid import reporter

module = lab.Module("reporter.workflow", doc=__doc__)

DH5alpha = Chassis.buy(
    competence=competent,
    identity="ATCC-53868",
    heat_shock_temperature=42 * C,
    recovery_duration=60 * minutes,
)
chloramphenicol = Antibiotic.buy(identity="SIGMA-C0378")

reporter_host = Strain.build(
    doc="The reporter carried in a cloning strain.",
    chassis=DH5alpha,
    plasmids=[reporter],
    selection=chloramphenicol,
)


@lab.workflow
def build_reporter(wf: lab.Context) -> tuple[Material[Strain], Material[Plate]]:
    """Assemble the reporter, transform it, and plate what recovers."""
    product = wf.perform(lab.realize(reporter))
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(lab.transform(reporter_host, plasmids=[product], cells=cells))
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    plate = wf.perform(lab.plate(culture, antibiotic=chloramphenicol))
    return strain, plate
