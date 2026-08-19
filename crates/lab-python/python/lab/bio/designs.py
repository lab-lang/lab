"""The kinds of thing a synthetic-biology laboratory works with.

A kind names a type, and its instances are written with that type's name in
snake_case. The word is vocabulary a package supplies; the compiler only
knows the shape. Whether any one thing was built or bought is stated by the
declaration that names it, not by its kind.

Each kind states the ontology terms it stands for, so what it is travels with
it. A target reading a design knows a backbone is DNA and an antibiotic is a
small molecule without being told separately.
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
    properties=("sequence",),
)
"""An assembly backbone.

Properties: sequence?: DNA.
"""

CDS = ArtifactKind(
    word="cds",
    produces="CDS",
    uses=("std.bio.designs",),
    properties=("sequence",),
)
"""A coding sequence for some protein.

Properties: sequence?: DNA.
"""

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
    properties=("sequence",),
)
"""A part a supplier lists, ordered rather than built.

A part is made of DNA, so it may state the DNA it is made of. A catalogue
that lists a part usually publishes its sequence, and a design that names the
part is entitled to read it.

Properties: sequence?: DNA.
"""

Plasmid = ArtifactKind(
    word="plasmid",
    produces="Plasmid",
    uses=("std.bio.designs",),
    properties=("backbone", "cargo", "components", "sequence"),
)
"""A DNA design a laboratory can build.

A plasmid states its sequence directly, or states the backbone together with
what goes into it: the parts an assembly joins, or the circuit a sequence can
be derived from.

What an assembly joins is anything made of DNA, which is what `any
NucleicAcid` says. Naming the admissible kinds instead would be a list that
every new kind of part has to be added to, and a promoter or a coding
sequence is no less assemblable than a bare part.

Properties: backbone?: Backbone, cargo?: Circuit<any Signal, any Protein>, components?:
List<any NucleicAcid>, sequence?: DNA.

Complete when it states either either sequence, or backbone and components, or backbone
and cargo.
"""

Promoter = ArtifactKind(
    word="promoter",
    produces="Promoter",
    uses=("std.bio.designs",),
    properties=("sequence",),
)
"""A promoter for some signal.

Properties: sequence?: DNA.
"""

RestrictionEnzyme = ArtifactKind(
    word="restriction_enzyme",
    produces="RestrictionEnzyme",
    uses=("std.bio.designs",),
    properties=("digest_duration", "digest_temperature"),
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
    properties=("chassis", "plasmids", "selection"),
)
"""An engineered organism: a chassis carrying named plasmid designs.

The same plasmid in two hosts is two artifacts, each with its own acceptance
criteria and its own place in a build order.

Properties: chassis: Chassis, plasmids: List<Plasmid>, selection?: Antibiotic.
"""
