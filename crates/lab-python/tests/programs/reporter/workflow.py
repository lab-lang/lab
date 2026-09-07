"""Assemble the reporter, transform it, and plate what recovers."""

from typing import cast

import lab
from lab import Material
from lab._types import DesignReference
from lab.bio.designs import Antibiotic, Chassis, Medium, Strain, competent, inoculated, poured
from lab.units import C, cfu, h, minutes, ug

from .plasmid import reporter

module = lab.Module("reporter.workflow", doc=__doc__)

# Facet properties are checked by Lab; the dynamic factory cannot infer them for mypy.
DH5alpha = cast(
    DesignReference[competent[lab.Chassis]],
    Chassis.buy(
        competence=competent,
        efficiency=10**9 * cfu / ug,
        supplier_identity="ATCC-53868",
        heat_shock_temperature=42 * C,
        recovery_duration=60 * minutes,
    ),
)
chloramphenicol = Antibiotic.buy(supplier_identity="SIGMA-C0378")
LB_chloramphenicol_agar = cast(
    DesignReference[poured[lab.Medium]],
    Medium.buy(
        supplier_identity="LB-CAM-AGAR",
        pouring=poured,
        selection=chloramphenicol,
    ),
)

reporter_host = Strain.build(
    doc="The reporter carried in a cloning strain.",
    chassis=DH5alpha,
    plasmids=[reporter],
    selection=chloramphenicol,
)


@lab.workflow
def build_reporter(
    wf: lab.Context,
) -> tuple[Material[lab.Strain], Material[inoculated[lab.Medium]]]:
    """Assemble the reporter, transform it, and plate what recovers."""
    product = wf.perform(lab.realize(reporter))
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(lab.transform(reporter_host, plasmids=[product], cells=cells))
    recovered_culture = wf.perform(lab.recover(culture, duration=1 * h))
    diluted_culture = wf.perform(lab.dilute(recovered_culture))
    agar = wf.perform(lab.provision(LB_chloramphenicol_agar))
    plate = wf.perform(lab.plate(diluted_culture, medium=agar))
    return strain, plate
