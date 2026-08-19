"""The GFP reporter plasmid, designed in pySBOL3."""

import lab
import sbol3
from lab.bio import golden_gate
from lab.bio.designs import Backbone, RestrictionEnzyme
from lab.units import C, minutes, ng, uL

module = lab.Module("reporter.plasmid", doc=__doc__)

sbol3.set_namespace("https://synbiohub.org/user/marpaia/reporter")

# The design is plain pySBOL3: parts referenced at the identity the registry
# already gave them, ordered by SBOL's own constraints.
IGEM = "https://synbiohub.org/public/igem"
J23101 = sbol3.SubComponent(f"{IGEM}/BBa_J23101/1")
B0034 = sbol3.SubComponent(f"{IGEM}/BBa_B0034/1")
GFP = sbol3.SubComponent(f"{IGEM}/BBa_E0040/1")
B0015 = sbol3.SubComponent(f"{IGEM}/BBa_B0015/1")

sequence = sbol3.Sequence("reporter_seq", elements="ACGTACGT", encoding=sbol3.IUPAC_DNA_ENCODING)

design = sbol3.Component(
    "reporter",
    [sbol3.SBO_DNA, sbol3.SO_CIRCULAR],
    roles=[sbol3.SO_ENGINEERED_REGION],
    sequences=[sequence],
    features=[J23101, B0034, GFP, B0015],
    description="The GFP reporter under a strong constitutive promoter.",
)
design.constraints = [
    sbol3.Constraint(sbol3.SBOL_MEETS, J23101, B0034),
    sbol3.Constraint(sbol3.SBOL_MEETS, B0034, GFP),
    sbol3.Constraint(sbol3.SBOL_MEETS, GFP, B0015),
]

document = sbol3.Document()
document.add(design)
document.add(sequence)

# What SBOL has no vocabulary for is lab's part: where a material comes
# from, and the evidence a built artifact is accepted on.
pSB1C3 = Backbone.buy(identity=f"{IGEM}/pSB1C3/1")
BsaI = RestrictionEnzyme.buy(
    identity="NEB-R0535",
    digest_temperature=37 * C,
    digest_duration=2 * minutes,
)

reporter = golden_gate.Plasmid.build(
    design,
    backbone=pSB1C3,
    restriction_enzyme=BsaI,
    reaction_volume=20 * uL,
    across=3,
    accept=[
        lambda built: built.concentration >= 100 * ng / uL,
        lab.Claim(lambda built: built.volume >= 20 * uL, across=1),
    ],
)
