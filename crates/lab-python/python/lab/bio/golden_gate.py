"""Golden Gate assembly and heat-shock transformation.

The scale a reaction runs at is neither the design's identity nor the
laboratory's equipment: it is what this method needs to make one. A package
that builds by another method describes its own, and a design that builds by
this one imports it.

Every property here is optional, because a method's standard values stand
behind a design that states nothing.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._types import LabType
from .._vocabulary import ArtifactKind

LAB_MODULE = "std.bio.golden_gate"
"""The Lab module these names come from."""


class Plasmid(ArtifactKind, LabType):
    """What Golden Gate assembly needs to build a plasmid.

    Properties: assembly_cycles?: Integer, assembly_replicates?: Integer, buffer_volume?:
    Quantity<uL>, digest_duration?: Quantity<min>, digest_temperature?: Quantity<C>,
    enzyme_volume?: Quantity<uL>, final_digest_duration?: Quantity<min>,
    final_digest_temperature?: Quantity<C>, heat_inactivation_duration?: Quantity<min>,
    heat_inactivation_temperature?: Quantity<C>, hold_temperature?: Quantity<C>,
    lid_temperature?: Quantity<C>, ligase_volume?: Quantity<uL>, ligate_duration?:
    Quantity<min>, ligate_temperature?: Quantity<C>, part_volume?: Quantity<uL>,
    reaction_volume?: Quantity<uL>, restriction_enzyme?: RestrictionEnzyme.
    """

    word = "plasmid"
    uses = ("std.bio.designs", "std.bio.golden_gate")
    __lab_uses__ = ("std.bio.designs", "std.bio.golden_gate")
    properties = (
        "assembly_cycles",
        "assembly_replicates",
        "buffer_volume",
        "digest_duration",
        "digest_temperature",
        "enzyme_volume",
        "final_digest_duration",
        "final_digest_temperature",
        "heat_inactivation_duration",
        "heat_inactivation_temperature",
        "hold_temperature",
        "lid_temperature",
        "ligase_volume",
        "ligate_duration",
        "ligate_temperature",
        "part_volume",
        "reaction_volume",
        "restriction_enzyme",
    )


class Strain(ArtifactKind, LabType):
    """What heat-shock transformation and plating need to build a strain.

    Properties: cell_aliquot_volume?: Quantity<uL>, cell_volume?: Quantity<uL>,
    cold_incubation?: Quantity<min>, colony_volume?: Quantity<uL>, culture_volume?:
    Quantity<uL>, dna_volume?: Quantity<uL>, heat_shock_duration?: Quantity<min>,
    heat_shock_temperature?: Quantity<C>, medium_volume?: Quantity<uL>, plating_replicates?:
    Integer, recovery_aliquot_volume?: Quantity<uL>, recovery_duration?: Quantity<min>,
    recovery_temperature?: Quantity<C>, recovery_volume?: Quantity<uL>, serial_dilutions?:
    Integer, transformation_replicates?: Integer.
    """

    word = "strain"
    uses = ("std.bio.designs", "std.bio.golden_gate")
    __lab_uses__ = ("std.bio.designs", "std.bio.golden_gate")
    properties = (
        "cell_aliquot_volume",
        "cell_volume",
        "cold_incubation",
        "colony_volume",
        "culture_volume",
        "dna_volume",
        "heat_shock_duration",
        "heat_shock_temperature",
        "medium_volume",
        "plating_replicates",
        "recovery_aliquot_volume",
        "recovery_duration",
        "recovery_temperature",
        "recovery_volume",
        "serial_dilutions",
        "transformation_replicates",
    )
