"""The tet reporter circuit, written as a LOICA genetic network."""

import lab
import loica
import sbol3
from lab.bio.parts import B0015, B0034

module = lab.Module("reporter.circuit", doc=__doc__)

sbol3.set_namespace("https://synbiohub.org/user/marpaia/reporter")

# Ordinary LOICA: an inducer, a reporter, and the characterized operator
# that connects them.
aTc = loica.Supplement(
    name="aTc",
    sbol_comp=sbol3.Component("aTc", sbol3.SBO_SIMPLE_CHEMICAL),
)
sfGFP = loica.Reporter(
    name="sfGFP",
    signal_id="green",
    sbol_comp=sbol3.Component("sfGFP", sbol3.SBO_DNA, roles=[sbol3.SO_CDS]),
)
pTet = loica.Receiver(
    input=aTc,
    output=sfGFP,
    alpha=[0, 100],
    K=1,
    n=2,
    sbol_comp=sbol3.Component("pTet", sbol3.SBO_DNA, roles=[sbol3.SO_PROMOTER]),
)


@lab.circuit
def regulated_expression() -> lab.Network:
    """A promoter driving a coding sequence through a shared RBS and terminator."""
    network = loica.GeneticNetwork()
    network.add_operator(pTet)
    network.add_reporter(sfGFP)
    return lab.layout(network, rbs=B0034, terminator=B0015)


# aTc's component is a simple chemical, so it is the Trigger; sfGFP's is the
# coding sequence of the Product. Circuit<ATc, SfGFP> is read off the
# network's own SBOL rather than declared a second time.
tet_reporter = regulated_expression()
