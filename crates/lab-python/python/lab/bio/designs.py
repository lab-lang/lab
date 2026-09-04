"""The kinds of thing a synthetic-biology laboratory works with.

A kind names a type, and its instances are written with that type's name in
snake_case. The word is vocabulary a package supplies; the compiler only
knows the shape. Whether any one thing was built or bought is stated by the
declaration that names it, not by its kind.

Each kind states the ontology terms it stands for, so what it is travels with
it. Any consumer reading a design knows a backbone is DNA and an antibiotic
is a small molecule without being told separately.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from typing import Generic, TypeVar

from .._types import LabConstructor, LabState, LabType
from .._vocabulary import ArtifactKind, Symbol

_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")

LAB_MODULE = "std.bio.designs"
"""The Lab module these names come from."""


class Both(LabType, Generic[_T1, _T2]):
    """A condition that is two signals at once.

    A promoter can integrate several inputs, and what it responds to is then no
    single molecule. Nesting states more than two, because a condition of
    several signals is itself a signal.
    """

    __lab_uses__ = ("std.bio.designs",)


Competence = Symbol(name="Competence", uses=("std.bio.designs",))
"""Whether a chassis will take up DNA.

Cells are made competent or bought that way, and the difference matters to
the one operation that needs it: transformation takes cells that are, and
says so, rather than trusting that whatever was fetched will do.

How competent they are is the batch's own number. A preparation is accepted
on a control transformation, so the efficiency belongs to the cells rather
than to the strain they came from or the plasmid they will carry.
"""


class naive(LabState, Generic[_T1]):
    __lab_state__ = "naive"
    __lab_uses__ = ("std.bio.designs",)


class competent(LabState, Generic[_T1]):
    __lab_state__ = "competent"
    __lab_uses__ = ("std.bio.designs",)


Cultivation = Symbol(name="Cultivation", uses=("std.bio.designs",))
"""Where an organism is in the course of being grown.

A culture was a type of its own, which is why it could never say what was
growing in it. It is an organism in a state, so the organism is the type and
how far along it is travels beside it.
"""


class designed(LabState, Generic[_T1]):
    __lab_state__ = "designed"
    __lab_uses__ = ("std.bio.designs",)


class transformed(LabState, Generic[_T1]):
    __lab_state__ = "transformed"
    __lab_uses__ = ("std.bio.designs",)


class recovered(LabState, Generic[_T1]):
    __lab_state__ = "recovered"
    __lab_uses__ = ("std.bio.designs",)


class diluted(LabState, Generic[_T1]):
    __lab_state__ = "diluted"
    __lab_uses__ = ("std.bio.designs",)


class isolated(LabState, Generic[_T1]):
    __lab_state__ = "isolated"
    __lab_uses__ = ("std.bio.designs",)


class grown(LabState, Generic[_T1]):
    __lab_state__ = "grown"
    __lab_uses__ = ("std.bio.designs",)


class Ingredient(LabConstructor):
    """How much of one substance a medium holds per unit volume."""

    __lab_uses__ = ("std.bio.designs",)


class Operon(LabType, Generic[_T1, _T2]):
    """Two products expressed from one promoter.

    A transcription unit may carry more than one coding sequence, and everything
    downstream of the promoter is expressed together. Nesting states more than
    two, the way `Both` does for the signals a promoter answers to.
    """

    __lab_uses__ = ("std.bio.designs",)


Pouring = Symbol(name="Pouring", uses=("std.bio.designs",))
"""Whether a medium has been poured and what has been put on it.

A plate was a type of its own and could not say what it was poured from, so
plating on the wrong medium was not something the compiler could see.
"""


class prepared(LabState, Generic[_T1]):
    __lab_state__ = "prepared"
    __lab_uses__ = ("std.bio.designs",)


class poured(LabState, Generic[_T1]):
    __lab_state__ = "poured"
    __lab_uses__ = ("std.bio.designs",)


class inoculated(LabState, Generic[_T1]):
    __lab_state__ = "inoculated"
    __lab_uses__ = ("std.bio.designs",)


class Antibiotic(ArtifactKind, LabType):
    """A selection agent a transformed culture is plated on."""

    word = "antibiotic"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ()


class Backbone(ArtifactKind, LabType):
    """An assembly backbone.

    Properties: sequence?: DNA.
    """

    word = "backbone"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("sequence",)


class CDS(ArtifactKind, LabType):
    """A coding sequence for some protein.

    Properties: sequence?: DNA.
    """

    word = "cds"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("sequence",)


class Chassis(ArtifactKind, LabType):
    """A host organism engineered DNA is carried in.

    Competent cells are transformed the way their supplier says, so the heat
    shock and recovery belong to the chassis rather than to each strain built in
    it.

    Properties: cold_incubation?: Quantity<min>, heat_shock_temperature?: Quantity<C>,
    recovery_duration?: Quantity<min>, recovery_temperature?: Quantity<C>.
    """

    word = "chassis"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = (
        "cold_incubation",
        "heat_shock_temperature",
        "recovery_duration",
        "recovery_temperature",
    )


class Medium(ArtifactKind, LabType):
    """What an organism is grown in or on.

    A medium is a recipe: what goes in it, and how much of each per unit volume.
    Concentrations rather than masses, because a recipe is the same whether a
    laboratory makes half a litre or five, and what to weigh out is the recipe
    times the batch.

    A solid medium is a liquid one with a gelling agent, which is why agar is a
    component rather than a second kind.

    Properties: components?: List<Ingredient>, ph?: Decimal, selection?: Antibiotic.
    """

    word = "medium"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("components", "ph", "selection")


class Part(ArtifactKind, LabType):
    """A part a supplier lists, ordered rather than built.

    A part is made of DNA, so it may state the DNA it is made of. A catalogue
    that lists a part usually publishes its sequence, and a design that names the
    part is entitled to read it.

    Properties: sequence?: DNA.
    """

    word = "part"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("sequence",)


class Plasmid(ArtifactKind, LabType):
    """A DNA design a laboratory can build.

    A plasmid states its sequence directly, or states the backbone together with
    what goes into it: the parts an assembly joins, or the circuits a sequence
    can be derived from.

    What an assembly joins is anything made of DNA, which is what `any
    NucleicAcid` says. Naming the admissible kinds instead would be a list that
    every new kind of part has to be added to, and a promoter or a coding
    sequence is no less assemblable than a bare part.

    Cargo is a list because a circuit is one transcription unit, and a network
    worth carrying is usually several of them wired together by the proteins
    they express. The triggers and products are forgotten because units with
    different triggers have no trigger in common; what each one responds to
    stays on the unit itself.

    Properties: backbone?: Backbone, cargo?: List<Circuit<any Signal, any Protein>>,
    components?: List<any NucleicAcid>, sequence?: DNA.

    Complete when it states either either sequence, or backbone and components, or backbone
    and cargo.
    """

    word = "plasmid"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("backbone", "cargo", "components", "sequence")


class Promoter(ArtifactKind, LabType):
    """A promoter for some signal.

    The signal is what the promoter answers to; `regulation` is which way it
    answers. A promoter that expresses more in the presence of its signal is
    induced by it, and one that expresses less is repressed by it. The
    difference is the difference between a buffer and an inverter, so a
    catalogue that knows it says it.

    Properties: regulation?: Regulation, sequence?: DNA.
    """

    word = "promoter"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("regulation", "sequence")


class RestrictionEnzyme(ArtifactKind, LabType):
    """A type IIS enzyme that opens a backbone.

    The temperature and time a digest runs at are the enzyme's, not the design's:
    every plasmid cut with the same enzyme cuts the same way. A design may still
    state its own where a protocol departs from the datasheet.

    Properties: digest_duration?: Quantity<min>, digest_temperature?: Quantity<C>.
    """

    word = "restriction_enzyme"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("digest_duration", "digest_temperature")


class Strain(ArtifactKind, LabType):
    """An engineered organism: a chassis carrying named plasmid designs.

    The same plasmid in two hosts is two artifacts, each with its own acceptance
    criteria and its own place in a build order.

    Properties: chassis: Chassis, plasmids: List<Plasmid>, selection?: Antibiotic.
    """

    word = "strain"
    uses = ("std.bio.designs",)
    __lab_uses__ = ("std.bio.designs",)
    properties = ("chassis", "plasmids", "selection")
