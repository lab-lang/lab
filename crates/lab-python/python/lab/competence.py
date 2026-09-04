"""Making a chassis competent.

Competent cells are grown up, chilled, spun into a pellet, and washed into a
cold buffer until they will take up DNA. None of these steps is assembly or
transformation, so none had a verb until a package could declare one. Each
verb here is an `action`: it names the material it takes, the material it
yields and the state that yielding leaves it in, and the capability a bench
or a robot must offer to run it. The compiler derives a manual method for
each, so the sequence plans without a line of Rust.

A buffer and a medium are both solutions a laboratory pours, and both are
washed or grown into cells, so they share the `Solution` role rather than
repeating what a solution can do on each kind.
"""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from typing import Generic, TypeVar

from ._effects import Action
from ._types import LabState, LabType
from ._vocabulary import ArtifactKind, Symbol

_T1 = TypeVar("_T1")

LAB_MODULE = "std.lab.competence"
"""The Lab module these names come from."""


Growth = Symbol(name="Growth", uses=("std.bio.designs", "std.lab.competence"))
"""How far a batch of cells is along the way to being competent.

Competence itself is a separate fact, declared where a chassis is: cells are
competent or they are not, and transformation is the one operation that
cares. These are the physical states a preparation passes through before it
gets there, so a verb that spins cells down can say it takes ones that are
growing and leaves ones that are pelleted.
"""


class dormant(LabState, Generic[_T1]):
    __lab_state__ = "dormant"
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")


class growing(LabState, Generic[_T1]):
    __lab_state__ = "growing"
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")


class pelleted(LabState, Generic[_T1]):
    __lab_state__ = "pelleted"
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")


class Buffer(ArtifactKind, LabType):
    """A salt solution cells are washed and resuspended in.

    A buffer and a medium are both solutions a laboratory pours, so both play the
    `Solution` role and a verb that resuspends cells asks for either. The
    concentration is the batch's own: a competent-cell protocol is written for a
    molarity of calcium chloride, and what to weigh out is that times the volume.

    Properties: concentration?: Quantity<any Molarity>.
    """

    word = "buffer"
    uses = ("std.bio.designs", "std.lab.competence")
    __lab_uses__ = ("std.bio.designs", "std.lab.competence")
    properties = ("concentration",)


centrifuge = Action(
    name="centrifuge",
    phrase=("centrifuge", "<cells>", "at", "<force>", "for", "<duration>"),
    results=("pellet",),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Spin a chilled culture into a pellet at a stated relative force.

Performed as `centrifuge <cells> at <force> for <duration>`.

Binds pellet.
"""

chill = Action(
    name="chill",
    phrase=("chill", "<cells>", "for", "<duration>"),
    results=("chilled",),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Chill a growing culture on ice before it is spun down.

Performed as `chill <cells> for <duration>`.

Binds chilled.
"""

grow = Action(
    name="grow",
    phrase=("grow", "<cells>", "at", "<temperature>", "to", "<target>"),
    results=("culture",),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Grow cells up to a target optical density.

The target is read at 600 or 700 nanometres, and the two do not convert: an
OD600 of 0.4 is not an OD700 of 0.4, so the unit says which meter the number
came off. Either reaches the same growing culture, so the operand admits
either and the protocol writes whichever its plate reader reports.

Performed as `grow <cells> at <temperature> to <target>`.

Binds culture.
"""

resuspend = Action(
    name="resuspend",
    phrase=("resuspend", "<cells>", "in", "<buffer>"),
    results=("competent",),
    uses=("std.bio.designs", "std.lab.competence"),
)
"""Resuspend a pellet in cold buffer, which is the wash that makes it competent.

The buffer is a solution, so the same verb pours a calcium-chloride wash or
any other a protocol calls for. What comes out is competent: ready for the
one operation that takes cells that are.

Performed as `resuspend <cells> in <buffer>`.

Binds competent.
"""
