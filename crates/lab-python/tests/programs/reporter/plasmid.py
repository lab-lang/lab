"""The GFP reporter plasmid, designed through Lab's typed SBOL layer."""

import lab
from lab import sbol
from lab.bio.designs import CDS, Backbone, Part, Promoter, RestrictionEnzyme
from lab.bio.golden_gate import Plasmid
from lab.units import C, minutes, ng, uL

module = lab.Module("reporter.plasmid", doc=__doc__)

# The typed document owns the pySBOL3 graph underneath. Registry identities are
# preserved, while each Python value keeps the biological kind it names.
designs = sbol.Document(namespace="https://synbiohub.org/user/marpaia/reporter")
IGEM = "https://synbiohub.org/public/igem"
J23101 = Promoter.buy(
    design=designs.promoter(identity=f"{IGEM}/BBa_J23101/1"),
)
B0034 = Part.buy(
    design=designs.rbs(identity=f"{IGEM}/BBa_B0034/1"),
)
GFP = CDS.buy(
    design=designs.cds(identity=f"{IGEM}/BBa_E0040/1"),
)
B0015 = Part.buy(
    design=designs.terminator(identity=f"{IGEM}/BBa_B0015/1"),
)

reporter_sequence = designs.dna_sequence(elements="ACGTACGT")
design = designs.plasmid(
    components=[J23101, B0034, GFP, B0015],
    sequence=reporter_sequence,
    description="The GFP reporter under a strong constitutive promoter.",
)

# What SBOL has no vocabulary for is lab's part: where a material comes
# from, and the evidence a built artifact is accepted on.
pSB1C3 = Backbone.buy(
    design=designs.backbone(identity=f"{IGEM}/pSB1C3/1"),
)
BsaI = RestrictionEnzyme.buy(
    identity="NEB-R0535",
    digest_temperature=37 * C,
    digest_duration=2 * minutes,
)

reporter = Plasmid.build(
    design=design,
    backbone=pSB1C3,
    restriction_enzyme=BsaI,
    reaction_volume=20 * uL,
    across=3,
    accept=[
        lambda built: built.concentration >= 100 * ng / uL,
        lab.Claim(lambda built: built.volume >= 20 * uL, across=1),
    ],
)
