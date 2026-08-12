"""Named biological parts and the roles they play.

A catalogued name says only that a supplier lists the item. It is not a claim
that a suitable lot is on the shelf; that remains an inventory resolution and
a runtime evidence question.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._vocabulary import Symbol

LAB_MODULE = "std.bio.parts"
"""The Lab module these names come from."""

Arabinose = Symbol(name="Arabinose", uses=("std.bio.designs", "std.bio.parts"))
"""The inducer an arabinose-responsive promoter answers to."""

B0015 = Symbol(name="B0015", uses=("std.bio.designs", "std.bio.parts"))
"""A value of type Part."""

B0034 = Symbol(name="B0034", uses=("std.bio.designs", "std.bio.parts"))
"""A value of type Part."""

BsaI = Symbol(name="BsaI", uses=("std.bio.designs", "std.bio.parts"))
"""A value of type RestrictionEnzyme."""

GreenFluorescentProtein = Symbol(
    name="GreenFluorescentProtein",
    uses=("std.bio.designs", "std.bio.parts"),
)
"""A reporter protein read as green fluorescence."""

Tetracycline = Symbol(name="Tetracycline", uses=("std.bio.designs", "std.bio.parts"))
"""The inducer a tetracycline-responsive promoter answers to."""

pBAD = Symbol(name="pBAD", uses=("std.bio.designs", "std.bio.parts"))
"""A value of type Promoter<Arabinose>."""

pTet = Symbol(name="pTet", uses=("std.bio.designs", "std.bio.parts"))
"""A value of type Promoter<Tetracycline>."""

sfGFP = Symbol(name="sfGFP", uses=("std.bio.designs", "std.bio.parts"))
"""A value of type CDS<GreenFluorescentProtein>."""
