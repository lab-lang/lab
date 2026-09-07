"""Making a chassis competent.

Competent cells are grown up, chilled, spun into a pellet, and washed into a
cold buffer until they will take up DNA. None of these steps is assembly or
transformation, so none had a verb until a package could declare one. Each
verb here is an `action`: it names the material it takes, the material it
yields and the state that yielding leaves it in, and exactly which input
lineage the result continues. Methods separately state how a facility can
perform each action.

A buffer and a medium are both solutions a laboratory pours, and both are
washed or grown into cells, so they share the `Solution` role rather than
repeating what a solution can do on each kind."""

# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.

# ruff: noqa

# fmt: off

from typing import Generic, TypeVar

from lab._effects import Action
from lab._types import DesignReference, LabConstructor, LabRole, LabState, LabType
from lab._vocabulary import ArtifactKind, Function, Symbol
from lab._workflows import ImportedWorkflow

_Growth_T1_1 = TypeVar("_Growth_T1_1", covariant=True)

LAB_MODULE = "std.lab.competence"
"""The exact Lab module these bindings import."""

Growth = Symbol(name="Growth", uses=("std.bio.designs", "std.lab.competence"), definition=("std.lab.competence", "Growth"))


class dormant(LabState, Generic[_Growth_T1_1]):
    __lab_state__ = "dormant"
    __lab_definition__ = ("std.lab.competence", "Growth")
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")


class growing(LabState, Generic[_Growth_T1_1]):
    __lab_state__ = "growing"
    __lab_definition__ = ("std.lab.competence", "Growth")
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")


class pelleted(LabState, Generic[_Growth_T1_1]):
    __lab_state__ = "pelleted"
    __lab_definition__ = ("std.lab.competence", "Growth")
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")
"""How far a batch of cells is along the way to being competent.

Competence itself is a separate fact, declared where a chassis is: cells are
competent or they are not, and transformation is the one operation that
cares. These are the physical states a preparation passes through before it
gets there, so a verb that spins cells down can say it takes ones that are
growing and leaves ones that are pelleted."""

class Buffer(ArtifactKind, LabType):
    word = "buffer"
    produces = "Buffer"
    definition = ("std.lab.competence", "buffer")
    uses = ("std.bio.designs", "std.lab.competence")
    __lab_name__ = "Buffer"
    __lab_roles__ = ("Solution",)
    __lab_definition__ = ("std.lab.competence", "buffer")
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")
    properties = ("concentration",)
"""A salt solution cells are washed and resuspended in.

A buffer and a medium are both solutions a laboratory pours, so both play the
`Solution` role and a verb that resuspends cells asks for either. The
concentration is the batch's own: a competent-cell protocol is written for a
molarity of calcium chloride, and what to weigh out is that times the volume."""

centrifuge = Action(
    name="centrifuge",
    definition=("std.lab.competence", "centrifuge"),
    operation="std.lab.competence.centrifuge",
    phrase=("centrifuge", "<cells>", "at", "<force>", "for", "<duration>"),
    python_slots=("cells", "force", "duration"),
    results=("pellet",),
    optional=(),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Spin a chilled culture into a pellet at a stated relative force."""

chill = Action(
    name="chill",
    definition=("std.lab.competence", "chill"),
    operation="std.lab.competence.chill",
    phrase=("chill", "<cells>", "for", "<duration>"),
    python_slots=("cells", "duration"),
    results=("chilled",),
    optional=(),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Chill a growing culture on ice before it is spun down."""

grow = Action(
    name="grow",
    definition=("std.lab.competence", "grow"),
    operation="std.lab.competence.grow",
    phrase=("grow", "<cells>", "at", "<temperature>", "to", "<target>"),
    python_slots=("cells", "temperature", "target"),
    results=("culture",),
    optional=(),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Grow cells up to a target optical density.

The target is read at 600 or 700 nanometres, and the two do not convert: an
OD600 of 0.4 is not an OD700 of 0.4, so the unit says which meter the number
came off. Either reaches the same growing culture, so the operand admits
either and the protocol writes whichever its plate reader reports."""

resuspend = Action(
    name="resuspend",
    definition=("std.lab.competence", "resuspend"),
    operation="std.lab.competence.resuspend",
    phrase=("resuspend", "<cells>", "in", "<buffer>"),
    python_slots=("cells", "buffer"),
    results=("competent",),
    optional=(),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Resuspend a pellet in cold buffer, which is the wash that makes it competent.

The buffer is a solution, so the same verb pours a calcium-chloride wash or
any other a protocol calls for. What comes out is competent: ready for the
one operation that takes cells that are."""
