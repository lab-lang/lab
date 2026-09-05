"""Golden Gate assembly and heat-shock transformation.

The scale a reaction runs at is neither the design's identity nor the
laboratory's equipment: it is what this method needs to make one. A package
that builds by another method describes its own, and a design that builds by
this one imports it.

Every property here is optional, because a method's standard values stand
behind a design that states nothing."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

LAB_MODULE = "std.bio.golden_gate"
"""The exact Lab module these bindings import."""

class Plasmid(ArtifactKind, LabType):
    word = "plasmid"
    produces = "Plasmid"
    definition = ("std.bio.golden_gate", "plasmid")
    uses = ("std.bio.designs", "std.bio.golden_gate")
    __lab_name__ = "Plasmid"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.golden_gate", "plasmid")
    __lab_uses__ = ("std.bio.designs", "std.bio.golden_gate")
    properties = ("assembly_cycles", "assembly_replicates", "buffer_volume", "digest_duration", "digest_temperature", "enzyme_volume", "final_digest_duration", "final_digest_temperature", "heat_inactivation_duration", "heat_inactivation_temperature", "hold_temperature", "lid_temperature", "ligase_volume", "ligate_duration", "ligate_temperature", "part_volume", "reaction_volume", "restriction_enzyme")
"""What Golden Gate assembly needs to build a plasmid."""

class Strain(ArtifactKind, LabType):
    word = "strain"
    produces = "Strain"
    definition = ("std.bio.golden_gate", "strain")
    uses = ("std.bio.designs", "std.bio.golden_gate")
    __lab_name__ = "Strain"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.golden_gate", "strain")
    __lab_uses__ = ("std.bio.designs", "std.bio.golden_gate")
    properties = ("cell_aliquot_volume", "cell_volume", "cold_incubation", "colony_volume", "culture_volume", "dna_volume", "heat_shock_duration", "heat_shock_temperature", "medium_volume", "plating_replicates", "recovery_aliquot_volume", "recovery_duration", "recovery_temperature", "recovery_volume", "serial_dilutions", "transformation_replicates")
"""What heat-shock transformation and plating need to build a strain."""
