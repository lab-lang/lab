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

# fmt: off

from __future__ import annotations

from typing import Any, Final, Generic, Protocol, TypeVar, overload

from lab._effects import Effect
from lab._expressions import Decimal, Quantity
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import WorkflowCall

import lab._prelude as _module_0
import lab.bio.ontology as _module_1

_Both_First_1 = TypeVar("_Both_First_1", bound=_module_0.Signal)
_Both_Second_2 = TypeVar("_Both_Second_2", bound=_module_0.Signal)
_Competence_T1_1 = TypeVar("_Competence_T1_1", covariant=True)
_Cultivation_T1_1 = TypeVar("_Cultivation_T1_1", covariant=True)
_Operon_First_1 = TypeVar("_Operon_First_1", bound=_module_0.Protein)
_Operon_Second_2 = TypeVar("_Operon_Second_2", bound=_module_0.Protein)
_Pouring_T1_1 = TypeVar("_Pouring_T1_1", covariant=True)

LAB_MODULE: Final[str]

class Both(_module_0.Signal, Generic[_Both_First_1, _Both_Second_2]): ...
"""A condition that is two signals at once.

A promoter can integrate several inputs, and what it responds to is then no
single molecule. Nesting states more than two, because a condition of
several signals is itself a signal."""

Competence: Final[Symbol]


class naive(LabState, Generic[_Competence_T1_1]): ...


class competent(LabState, Generic[_Competence_T1_1]): ...
"""Whether a chassis will take up DNA.

Cells are made competent or bought that way, and the difference matters to
the one operation that needs it: transformation takes cells that are, and
says so, rather than trusting that whatever was fetched will do.

How competent they are is the batch's own number. A preparation is accepted
on a control transformation, so the efficiency belongs to the cells rather
than to the strain they came from or the plasmid they will carry."""

Cultivation: Final[Symbol]


class designed(LabState, Generic[_Cultivation_T1_1]): ...


class transformed(LabState, Generic[_Cultivation_T1_1]): ...


class recovered(LabState, Generic[_Cultivation_T1_1]): ...


class diluted(LabState, Generic[_Cultivation_T1_1]): ...


class isolated(LabState, Generic[_Cultivation_T1_1]): ...


class grown(LabState, Generic[_Cultivation_T1_1]): ...
"""Where an organism is in the course of being grown.

A culture was a type of its own, which is why it could never say what was
growing in it. It is an organism in a state, so the organism is the type and
how far along it is travels beside it."""

class Ingredient(LabConstructor):
    def __new__(cls, *, concentration: Quantity, substance: str) -> Ingredient: ...
"""How much of one substance a medium holds per unit volume."""

class Operon(_module_0.Protein, Generic[_Operon_First_1, _Operon_Second_2]): ...
"""Two products expressed from one promoter.

A transcription unit may carry more than one coding sequence, and everything
downstream of the promoter is expressed together. Nesting states more than
two, the way `Both` does for the signals a promoter answers to."""

Pouring: Final[Symbol]


class prepared(LabState, Generic[_Pouring_T1_1]): ...


class poured(LabState, Generic[_Pouring_T1_1]): ...


class inoculated(LabState, Generic[_Pouring_T1_1]): ...
"""Whether a medium has been poured and what has been put on it.

A plate was a type of its own and could not say what it was poured from, so
plating on the wrong medium was not something the compiler could see."""

class Antibiotic(ArtifactKind, _module_0.Antibiotic, _module_1.SimpleChemical): ...
"""A selection agent a transformed culture is plated on."""

class Backbone(ArtifactKind, _module_0.Backbone, _module_1.NucleicAcid, _module_1.EngineeredRegion): ...
"""An assembly backbone."""

class CDS(ArtifactKind, _module_0.CDS[Any], _module_1.NucleicAcid, _module_1.CodingSequence): ...
"""A coding sequence for some protein."""

class Chassis(ArtifactKind, _module_0.Chassis, _module_1.FunctionalEntity): ...
"""A host organism engineered DNA is carried in.

Competent cells are transformed the way their supplier says, so the heat
shock and recovery belong to the chassis rather than to each strain built in
it."""

class Medium(ArtifactKind, _module_0.Medium, _module_1.FunctionalEntity): ...
"""What an organism is grown in or on.

A medium is a recipe: what goes in it, and how much of each per unit volume.
Concentrations rather than masses, because a recipe is the same whether a
laboratory makes half a litre or five, and what to weigh out is the recipe
times the batch.

A solid medium is a liquid one with a gelling agent, which is why agar is a
component rather than a second kind."""

class Part(ArtifactKind, _module_0.Part, _module_1.NucleicAcid): ...
"""A part a supplier lists, ordered rather than built.

A part is made of DNA, so it may state the DNA it is made of. A catalogue
that lists a part usually publishes its sequence, and a design that names the
part is entitled to read it."""

class Plasmid(ArtifactKind, _module_0.Plasmid, _module_1.NucleicAcid, _module_1.EngineeredRegion): ...
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

class Promoter(ArtifactKind, _module_0.Promoter[Any], _module_1.NucleicAcid, _module_1.PromoterRegion): ...
"""A promoter for some signal.

The signal is what the promoter answers to; `regulation` is which way it
answers. A promoter that expresses more in the presence of its signal is
induced by it, and one that expresses less is repressed by it. The
difference is the difference between a buffer and an inverter, so a
catalogue that knows it says it."""

class RestrictionEnzyme(ArtifactKind, _module_0.RestrictionEnzyme, _module_1.Macromolecule): ...
"""A type IIS enzyme that opens a backbone.

The temperature and time a digest runs at are the enzyme's, not the design's:
every plasmid cut with the same enzyme cuts the same way. A design may still
state its own where a protocol departs from the datasheet."""

class Strain(ArtifactKind, _module_0.Strain, _module_1.FunctionalEntity): ...
"""An engineered organism: a chassis carrying named plasmid designs.

The same plasmid in two hosts is two artifacts, each with its own acceptance
criteria and its own place in a build order."""
