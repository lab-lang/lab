"""Assemble each reporter plasmid into a physical material."""

import lab
from lab import Material
from lab.bio.golden_gate import Plasmid

from ..designs.plasmids import composite_plasmid_1, composite_plasmid_2

module = lab.Module("golden_gate.workflows.assemble", doc=__doc__)


@lab.workflow
def assemble_composite_plasmid_1(wf: lab.Context) -> Material[Plasmid]:
    """Assemble the GFP reporter plasmid."""
    product = wf.perform(lab.realize(composite_plasmid_1))
    return product


@lab.workflow
def assemble_composite_plasmid_2(wf: lab.Context) -> Material[Plasmid]:
    """Assemble the RFP reporter plasmid."""
    product = wf.perform(lab.realize(composite_plasmid_2))
    return product
