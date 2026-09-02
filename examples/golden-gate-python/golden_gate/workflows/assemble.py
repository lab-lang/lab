"""Assemble each plasmid into a physical material."""

import lab
from lab import Material
from lab.bio.golden_gate import Plasmid

from ..designs.plasmids import GVD0011, GVD0013, GVD0015

module = lab.Module("golden_gate.workflows.assemble", doc=__doc__)


@lab.workflow
def assemble_GVD0011(wf: lab.Context) -> Material[Plasmid]:
    plasmid = wf.perform(lab.realize(GVD0011))
    return plasmid


@lab.workflow
def assemble_GVD0013(wf: lab.Context) -> Material[Plasmid]:
    plasmid = wf.perform(lab.realize(GVD0013))
    return plasmid


@lab.workflow
def assemble_GVD0015(wf: lab.Context) -> Material[Plasmid]:
    plasmid = wf.perform(lab.realize(GVD0015))
    return plasmid
