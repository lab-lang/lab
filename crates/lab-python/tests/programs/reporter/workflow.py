"""Assemble the reporter, transform it, and plate what recovers."""

import lab
from lab import Material
from lab.bio.designs import Antibiotic, Chassis, Medium, Strain, competent, inoculated, poured
from lab.units import C, cfu, h, minutes, ug

from .plasmid import reporter

module = lab.Module("reporter.workflow", doc=__doc__)

DH5alpha = Chassis.buy(
    competence=competent,
    efficiency=10**9 * cfu / ug,
    identity="ATCC-53868",
    heat_shock_temperature=42 * C,
    recovery_duration=60 * minutes,
)
chloramphenicol = Antibiotic.buy(identity="SIGMA-C0378")
LB_chloramphenicol_agar = Medium.buy(
    identity="LB-CAM-AGAR",
    pouring=poured,
    selection=chloramphenicol,
)

reporter_host = Strain.build(
    doc="The reporter carried in a cloning strain.",
    chassis=DH5alpha,
    plasmids=[reporter],
    selection=chloramphenicol,
)


@lab.workflow
def build_reporter(wf: lab.Context) -> tuple[Material[Strain], Material[inoculated[Medium]]]:
    """Assemble the reporter, transform it, and plate what recovers."""
    product = wf.perform(lab.realize(reporter))
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(lab.transform(reporter_host, plasmids=[product], cells=cells))
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    culture = wf.perform(lab.dilute(culture))
    agar = wf.perform(lab.provision(LB_chloramphenicol_agar))
    plate = wf.perform(lab.plate(culture, medium=agar))
    return strain, plate
