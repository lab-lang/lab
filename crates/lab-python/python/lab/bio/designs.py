"""The kinds of thing a synthetic-biology laboratory works with.

A kind names a type, and its instances are written with that type's name in
snake_case. The word is vocabulary a package supplies; the compiler only
knows the shape. Whether any one thing was built or bought is stated by the
declaration that names it, not by its kind.

Each kind states the ontology terms it stands for, so what it is travels with
it. Any consumer reading a design knows a backbone is DNA and an antibiotic
is a small molecule without being told separately."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

_Both_First_1 = TypeVar("_Both_First_1")
_Both_Second_2 = TypeVar("_Both_Second_2")
_Competence_T1_1 = TypeVar("_Competence_T1_1")
_Cultivation_T1_1 = TypeVar("_Cultivation_T1_1")
_Operon_First_1 = TypeVar("_Operon_First_1")
_Operon_Second_2 = TypeVar("_Operon_Second_2")
_Pouring_T1_1 = TypeVar("_Pouring_T1_1")

LAB_MODULE = "std.bio.designs"
"""The exact Lab module these bindings import."""

class Both(LabType, Generic[_Both_First_1, _Both_Second_2]):
    __lab_name__ = "Both"
    __lab_roles__ = ("Signal",)
    __lab_definition__ = ("std.bio.designs", "Both")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
"""A condition that is two signals at once.

A promoter can integrate several inputs, and what it responds to is then no
single molecule. Nesting states more than two, because a condition of
several signals is itself a signal."""

Competence = Symbol(name="Competence", uses=("std.bio.ontology", "std.bio.designs"), definition=("std.bio.designs", "Competence"))


class naive(LabState, Generic[_Competence_T1_1]):
    __lab_state__ = "naive"
    __lab_definition__ = ("std.bio.designs", "Competence")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class competent(LabState, Generic[_Competence_T1_1]):
    __lab_state__ = "competent"
    __lab_definition__ = ("std.bio.designs", "Competence")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
"""Whether a chassis will take up DNA.

Cells are made competent or bought that way, and the difference matters to
the one operation that needs it: transformation takes cells that are, and
says so, rather than trusting that whatever was fetched will do.

How competent they are is the batch's own number. A preparation is accepted
on a control transformation, so the efficiency belongs to the cells rather
than to the strain they came from or the plasmid they will carry."""

Cultivation = Symbol(name="Cultivation", uses=("std.bio.ontology", "std.bio.designs"), definition=("std.bio.designs", "Cultivation"))


class designed(LabState, Generic[_Cultivation_T1_1]):
    __lab_state__ = "designed"
    __lab_definition__ = ("std.bio.designs", "Cultivation")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class transformed(LabState, Generic[_Cultivation_T1_1]):
    __lab_state__ = "transformed"
    __lab_definition__ = ("std.bio.designs", "Cultivation")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class recovered(LabState, Generic[_Cultivation_T1_1]):
    __lab_state__ = "recovered"
    __lab_definition__ = ("std.bio.designs", "Cultivation")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class diluted(LabState, Generic[_Cultivation_T1_1]):
    __lab_state__ = "diluted"
    __lab_definition__ = ("std.bio.designs", "Cultivation")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class isolated(LabState, Generic[_Cultivation_T1_1]):
    __lab_state__ = "isolated"
    __lab_definition__ = ("std.bio.designs", "Cultivation")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class grown(LabState, Generic[_Cultivation_T1_1]):
    __lab_state__ = "grown"
    __lab_definition__ = ("std.bio.designs", "Cultivation")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
"""Where an organism is in the course of being grown.

A culture was a type of its own, which is why it could never say what was
growing in it. It is an organism in a state, so the organism is the type and
how far along it is travels beside it."""

class Ingredient(LabConstructor):
    __lab_fields__ = (("concentration", "concentration", False), ("substance", "substance", False))
    __lab_name__ = "Ingredient"
    __lab_roles__ = ()
    __lab_definition__ = ("std.bio.designs", "Ingredient")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
"""How much of one substance a medium holds per unit volume."""

class Operon(LabType, Generic[_Operon_First_1, _Operon_Second_2]):
    __lab_name__ = "Operon"
    __lab_roles__ = ("Protein",)
    __lab_definition__ = ("std.bio.designs", "Operon")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
"""Two products expressed from one promoter.

A transcription unit may carry more than one coding sequence, and everything
downstream of the promoter is expressed together. Nesting states more than
two, the way `Both` does for the signals a promoter answers to."""

Pouring = Symbol(name="Pouring", uses=("std.bio.ontology", "std.bio.designs"), definition=("std.bio.designs", "Pouring"))


class prepared(LabState, Generic[_Pouring_T1_1]):
    __lab_state__ = "prepared"
    __lab_definition__ = ("std.bio.designs", "Pouring")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class poured(LabState, Generic[_Pouring_T1_1]):
    __lab_state__ = "poured"
    __lab_definition__ = ("std.bio.designs", "Pouring")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")


class inoculated(LabState, Generic[_Pouring_T1_1]):
    __lab_state__ = "inoculated"
    __lab_definition__ = ("std.bio.designs", "Pouring")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
"""Whether a medium has been poured and what has been put on it.

A plate was a type of its own and could not say what it was poured from, so
plating on the wrong medium was not something the compiler could see."""

class Antibiotic(ArtifactKind, LabType):
    word = "antibiotic"
    produces = "Antibiotic"
    definition = ("std.bio.designs", "antibiotic")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Antibiotic"
    __lab_roles__ = ("SimpleChemical",)
    __lab_definition__ = ("std.bio.designs", "antibiotic")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ()
"""A selection agent a transformed culture is plated on."""

class Backbone(ArtifactKind, LabType):
    word = "backbone"
    produces = "Backbone"
    definition = ("std.bio.designs", "backbone")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Backbone"
    __lab_roles__ = ("NucleicAcid", "EngineeredRegion")
    __lab_definition__ = ("std.bio.designs", "backbone")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("sequence",)
"""An assembly backbone."""

class CDS(ArtifactKind, LabType):
    word = "cds"
    produces = "CDS"
    definition = ("std.bio.designs", "cds")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "CDS"
    __lab_roles__ = ("NucleicAcid", "CodingSequence")
    __lab_definition__ = ("std.bio.designs", "cds")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("sequence",)
"""A coding sequence for some protein."""

class Chassis(ArtifactKind, LabType):
    word = "chassis"
    produces = "Chassis"
    definition = ("std.bio.designs", "chassis")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Chassis"
    __lab_roles__ = ("FunctionalEntity",)
    __lab_definition__ = ("std.bio.designs", "chassis")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("cold_incubation", "heat_shock_temperature", "recovery_duration", "recovery_temperature")
"""A host organism engineered DNA is carried in.

Competent cells are transformed the way their supplier says, so the heat
shock and recovery belong to the chassis rather than to each strain built in
it."""

class Medium(ArtifactKind, LabType):
    word = "medium"
    produces = "Medium"
    definition = ("std.bio.designs", "medium")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Medium"
    __lab_roles__ = ("FunctionalEntity",)
    __lab_definition__ = ("std.bio.designs", "medium")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("components", "ph", "selection")
"""What an organism is grown in or on.

A medium is a recipe: what goes in it, and how much of each per unit volume.
Concentrations rather than masses, because a recipe is the same whether a
laboratory makes half a litre or five, and what to weigh out is the recipe
times the batch.

A solid medium is a liquid one with a gelling agent, which is why agar is a
component rather than a second kind."""

class Part(ArtifactKind, LabType):
    word = "part"
    produces = "Part"
    definition = ("std.bio.designs", "part")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Part"
    __lab_roles__ = ("NucleicAcid",)
    __lab_definition__ = ("std.bio.designs", "part")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("sequence",)
"""A part a supplier lists, ordered rather than built.

A part is made of DNA, so it may state the DNA it is made of. A catalogue
that lists a part usually publishes its sequence, and a design that names the
part is entitled to read it."""

class Plasmid(ArtifactKind, LabType):
    word = "plasmid"
    produces = "Plasmid"
    definition = ("std.bio.designs", "plasmid")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Plasmid"
    __lab_roles__ = ("NucleicAcid", "EngineeredRegion")
    __lab_definition__ = ("std.bio.designs", "plasmid")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("backbone", "cargo", "components", "sequence")
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
stays on the unit itself."""

class Promoter(ArtifactKind, LabType):
    word = "promoter"
    produces = "Promoter"
    definition = ("std.bio.designs", "promoter")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Promoter"
    __lab_roles__ = ("NucleicAcid", "PromoterRegion")
    __lab_definition__ = ("std.bio.designs", "promoter")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("regulation", "sequence")
"""A promoter for some signal.

The signal is what the promoter answers to; `regulation` is which way it
answers. A promoter that expresses more in the presence of its signal is
induced by it, and one that expresses less is repressed by it. The
difference is the difference between a buffer and an inverter, so a
catalogue that knows it says it."""

class RestrictionEnzyme(ArtifactKind, LabType):
    word = "restriction_enzyme"
    produces = "RestrictionEnzyme"
    definition = ("std.bio.designs", "restriction_enzyme")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "RestrictionEnzyme"
    __lab_roles__ = ("Macromolecule",)
    __lab_definition__ = ("std.bio.designs", "restriction_enzyme")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("digest_duration", "digest_temperature")
"""A type IIS enzyme that opens a backbone.

The temperature and time a digest runs at are the enzyme's, not the design's:
every plasmid cut with the same enzyme cuts the same way. A design may still
state its own where a protocol departs from the datasheet."""

class Strain(ArtifactKind, LabType):
    word = "strain"
    produces = "Strain"
    definition = ("std.bio.designs", "strain")
    uses = ("std.bio.ontology", "std.bio.designs")
    __lab_name__ = "Strain"
    __lab_roles__ = ("FunctionalEntity",)
    __lab_definition__ = ("std.bio.designs", "strain")
    __lab_uses__ = ("std.bio.ontology", "std.bio.designs")
    properties = ("chassis", "plasmids", "selection")
"""An engineered organism: a chassis carrying named plasmid designs.

The same plasmid in two hosts is two artifacts, each with its own acceptance
criteria and its own place in a build order."""
