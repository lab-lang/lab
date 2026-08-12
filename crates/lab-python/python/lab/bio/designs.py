"""The kinds of thing a synthetic-biology laboratory works with.

A kind names a type, and its instances are written with that type's name in
snake_case. The word is vocabulary a package supplies; the compiler only
knows the shape. Whether any one thing was built or bought is stated by the
declaration that names it, not by its kind.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._vocabulary import ArtifactKind

LAB_MODULE = "std.bio.designs"
"""The Lab module these names come from."""

Antibiotic = ArtifactKind(
    word="antibiotic",
    produces="Antibiotic",
    uses=("std.bio.designs",),
    properties=(),
)
"""A selection agent a transformed culture is plated on."""

Backbone = ArtifactKind(
    word="backbone",
    produces="Backbone",
    uses=("std.bio.designs",),
    properties=(),
)
"""An assembly backbone."""

CDS = ArtifactKind(
    word="cds",
    produces="CDS",
    uses=("std.bio.designs",),
    properties=(),
)
"""A coding sequence for some protein."""

Chassis = ArtifactKind(
    word="chassis",
    produces="Chassis",
    uses=("std.bio.designs",),
    properties=(
        "cold_incubation",
        "heat_shock_temperature",
        "recovery_duration",
        "recovery_temperature",
    ),
)
"""A host organism engineered DNA is carried in.

Competent cells are transformed the way their supplier says, so the heat
shock and recovery belong to the chassis rather than to each strain built in
it.

Properties: cold_incubation?: Quantity<min>, heat_shock_temperature?: Quantity<C>,
recovery_duration?: Quantity<min>, recovery_temperature?: Quantity<C>.
"""

Part = ArtifactKind(
    word="part",
    produces="Part",
    uses=("std.bio.designs",),
    properties=(),
)
"""A part a supplier lists, ordered rather than built."""

Plasmid = ArtifactKind(
    word="plasmid",
    produces="Plasmid",
    uses=("std.bio.designs",),
    properties=(
        "backbone",
        "cargo",
        "components",
        "sequence",
    ),
)
"""A DNA design a laboratory can build.

A plasmid states its sequence directly, or states the backbone together with
what goes into it: the parts an assembly joins, or the circuit a sequence can
be derived from.

Properties: backbone?: Backbone, cargo?: Circuit<any Signal, any Protein>, components?:
List<Part | Plasmid>, sequence?: DNA.

Complete when it states either either sequence, or backbone and components, or backbone
and cargo.
"""

Promoter = ArtifactKind(
    word="promoter",
    produces="Promoter",
    uses=("std.bio.designs",),
    properties=(),
)
"""A promoter for some signal."""

RestrictionEnzyme = ArtifactKind(
    word="restriction_enzyme",
    produces="RestrictionEnzyme",
    uses=("std.bio.designs",),
    properties=(
        "digest_duration",
        "digest_temperature",
    ),
)
"""A type IIS enzyme that opens a backbone.

The temperature and time a digest runs at are the enzyme's, not the design's:
every plasmid cut with the same enzyme cuts the same way. A design may still
state its own where a protocol departs from the datasheet.

Properties: digest_duration?: Quantity<min>, digest_temperature?: Quantity<C>.
"""

Strain = ArtifactKind(
    word="strain",
    produces="Strain",
    uses=("std.bio.designs",),
    properties=(
        "chassis",
        "plasmids",
        "selection",
    ),
)
"""An engineered organism: a chassis carrying named plasmid designs.

The same plasmid in two hosts is two artifacts, each with its own acceptance
criteria and its own place in a build order.

Properties: chassis: Chassis, plasmids: List<Plasmid>, selection?: Antibiotic.
"""
