"""Three inducible GFP circuits represented as one LOICA network."""

from typing import TYPE_CHECKING

import lab
import loica
import sbol3
from lab import Circuit, Evidential, Material, Signal
from lab.bio.parts import B0015, B0034

if TYPE_CHECKING:

    class ATc(Signal):
        pass

    class Arabinose_Signal(Signal):
        pass

    class IPTG(Signal):
        pass

    class SfGFP:
        pass


module = lab.Module("golden_gate_extended.designs.circuits", doc=__doc__)

sbol3.set_namespace("https://example.org/golden-gate-extended/circuits")
aTc = loica.Supplement(
    name="aTc",
    sbol_comp=sbol3.Component("aTc", sbol3.SBO_SIMPLE_CHEMICAL),
)
arabinose = loica.Supplement(
    name="arabinose",
    sbol_comp=sbol3.Component("arabinose", sbol3.SBO_SIMPLE_CHEMICAL),
)
iptg = loica.Supplement(
    name="IPTG",
    sbol_comp=sbol3.Component("IPTG", sbol3.SBO_SIMPLE_CHEMICAL),
)
sfGFP = loica.Reporter(
    name="sfGFP",
    signal_id="green",
    sbol_comp=sbol3.Component("sfGFP", sbol3.SBO_DNA, roles=[sbol3.SO_CDS]),
)


def _receiver(name: str, inducer: object) -> loica.Receiver:
    return loica.Receiver(
        input=inducer,
        output=sfGFP,
        alpha=[0, 100],
        K=1,
        n=2,
        sbol_comp=sbol3.Component(name, sbol3.SBO_DNA, roles=[sbol3.SO_PROMOTER]),
    )


pTet = _receiver("pTet", aTc)
pBAD = _receiver("pBAD", arabinose)
pLac = _receiver("pLac", iptg)


@lab.circuit
def regulated_expression() -> lab.Network:
    """Three sensors that vary their trigger while keeping GFP as the product."""
    network = loica.GeneticNetwork()
    network.add_operator([pTet, pBAD, pLac])
    network.add_reporter(sfGFP)
    return lab.layout(network, rbs=B0034, terminator=B0015)


panel = regulated_expression()


@lab.record
class Reading(Evidential):
    """One fluorescence reading and the gain used to record it."""

    value: float
    gain: int


@lab.workflow
def characterize_tet(
    wf: lab.Context,
    design: "Circuit[ATc, SfGFP]",
    inducer: "Material[ATc]",
) -> tuple["Circuit[ATc, SfGFP]", "Material[ATc]"]:
    """Characterize the tetracycline-responsive circuit."""
    return design, inducer


@lab.workflow
def characterize_arabinose(
    wf: lab.Context,
    design: "Circuit[Arabinose_Signal, SfGFP]",
    inducer: "Material[Arabinose_Signal]",
) -> tuple[
    "Circuit[Arabinose_Signal, SfGFP]",
    "Material[Arabinose_Signal]",
]:
    """Characterize the arabinose-responsive circuit."""
    return design, inducer


@lab.workflow
def characterize_iptg(
    wf: lab.Context,
    design: "Circuit[IPTG, SfGFP]",
    inducer: "Material[IPTG]",
) -> tuple["Circuit[IPTG, SfGFP]", "Material[IPTG]"]:
    """Characterize the IPTG-responsive circuit."""
    return design, inducer


@lab.workflow
def summarize(
    wf: lab.Context,
    from_tet: Reading,
    from_ara: Reading,
    from_lac: Reading,
) -> list[Reading]:
    """Return the comparable readings produced by the panel."""
    return [from_tet, from_ara, from_lac]
