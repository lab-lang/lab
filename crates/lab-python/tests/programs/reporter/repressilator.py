"""The repressilator, written as a LOICA genetic network.

Three transcription units in a ring, each repressor shutting off the next.
Nothing induces it from outside and nothing reports out of it: the whole
network is regulators wired to each other, which is what makes it the
sharpest test that the wiring is carried by the types.
"""

import lab
import loica
import sbol3
from lab.bio.parts import B0015, B0034

module = lab.Module("reporter.repressilator", doc=__doc__)

sbol3.set_namespace("https://example.org/repressilator")


def _promoter(name: str) -> sbol3.Component:
    return sbol3.Component(name, sbol3.SBO_DNA, roles=[sbol3.SO_PROMOTER])


def _coding(name: str) -> sbol3.Component:
    return sbol3.Component(name, sbol3.SBO_DNA, roles=[sbol3.SO_CDS])


tetR = loica.Regulator(name="TetR", sbol_comp=_coding("TetR"))
lacI = loica.Regulator(name="LacI", sbol_comp=_coding("LacI"))
cI = loica.Regulator(name="CI", sbol_comp=_coding("CI"))

# Each promoter is repressed by the regulator before it: the basal rate
# exceeds the regulated one, which is how LOICA states inhibition.
p_lac = loica.Hill1(input=lacI, output=tetR, alpha=[100, 0], K=1, n=2, sbol_comp=_promoter("pLac"))
p_tet = loica.Hill1(input=tetR, output=cI, alpha=[100, 0], K=1, n=2, sbol_comp=_promoter("pTet"))
p_ci = loica.Hill1(input=cI, output=lacI, alpha=[100, 0], K=1, n=2, sbol_comp=_promoter("pCI"))


@lab.circuit
def repressilator() -> lab.Layout:
    """Three repressors in a ring, each shutting off the next."""
    network = loica.GeneticNetwork()
    network.add_operator([p_lac, p_tet, p_ci])
    network.add_regulator([tetR, lacI, cI])
    return lab.layout(network, rbs=B0034, terminator=B0015)


ring = repressilator()
