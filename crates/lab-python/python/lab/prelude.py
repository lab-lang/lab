"""Foundational types and operations available to every Lab module."""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from ._vocabulary import Function, Symbol

LAB_MODULE = "std.prelude"
"""The Lab module these names come from."""

Accepted = Symbol(name="Accepted", uses=())

Antibiotic = Symbol(name="Antibiotic", uses=())

Backbone = Symbol(name="Backbone", uses=())

CDS = Symbol(name="CDS", uses=())

Chassis = Symbol(name="Chassis", uses=())
"""A host organism that carries engineered DNA."""

Circuit = Symbol(name="Circuit", uses=())

Clone = Symbol(name="Clone", uses=())

CloneSet = Symbol(name="CloneSet", uses=())

Colonies = Symbol(name="Colonies", uses=())

ColonyMap = Symbol(name="ColonyMap", uses=())

Culture = Symbol(name="Culture", uses=())

DNA = Symbol(name="DNA", uses=())

Duration = Symbol(name="Duration", uses=())

Evidence = Symbol(name="Evidence", uses=())

Evidential = Symbol(name="Evidential", uses=())
"""Information that may be offered in support of a claim."""

Event = Symbol(name="Event", uses=())
"""An occurrence the durable workflow journal records."""

Fragment = Symbol(name="Fragment", uses=())

Image = Symbol(name="Image", uses=())

List = Symbol(name="List", uses=())

Material = Symbol(name="Material", uses=())

Part = Symbol(name="Part", uses=())

Plate = Symbol(name="Plate", uses=())

Plasmid = Symbol(name="Plasmid", uses=())
"""A backend-neutral plasmid design."""

Promoter = Symbol(name="Promoter", uses=())

Protein = Symbol(name="Protein", uses=())
"""A gene product a coding sequence expresses."""

Reason = Symbol(name="Reason", uses=())

Rejected = Symbol(name="Rejected", uses=())

RestrictionEnzyme = Symbol(name="RestrictionEnzyme", uses=())

Screening = Symbol(name="Screening", uses=())

Signal = Symbol(name="Signal", uses=())
"""A molecule or condition a circuit responds to."""

Strain = Symbol(name="Strain", uses=())
"""A chassis carrying a defined set of plasmid designs."""

Topology = Symbol(name="Topology", uses=())

WorkflowContext = Symbol(name="WorkflowContext", uses=())

circular = Symbol(name="circular", uses=())
"""A value of type Topology."""

no_colonies = Symbol(name="no_colonies", uses=())
"""A value of type Reason."""

sequence_mismatch = Symbol(name="sequence_mismatch", uses=())
"""A value of type Reason."""

inconclusive_sequence = Symbol(name="inconclusive_sequence", uses=())
"""A value of type Reason."""

acceptance_failed = Symbol(name="acceptance_failed", uses=())
"""A value of type Reason."""

dna = Function(name="dna", uses=())
"""Construct a DNA value from a nucleotide sequence.

Called as (String) -> DNA.
"""

detect_colonies = Function(name="detect_colonies", uses=())
"""Called as (Image) -> ColonyMap."""

sites = Function(name="sites", uses=())
"""Called as (RestrictionEnzyme) -> Integer."""

accepts = Function(name="accepts", uses=())
"""Whether a design's acceptance criteria are met by this evidence.

Called as (Plasmid, List<Evidence>) -> Bool.
"""

Accepted = Symbol(name="Accepted", uses=())
"""An accepted material paired with its supporting evidence."""

Rejected = Symbol(name="Rejected", uses=())
"""A rejected material with evidence and a machine-readable reason."""
