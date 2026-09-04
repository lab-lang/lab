"""Part of the Python mirror of the Lab standard library."""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from ._effects import Action

LAB_MODULE = "std.lab.plasmid"
"""The Lab module these names come from."""

capture = Action(
    name="capture",
    phrase=("capture", "image", "of", "<plate>"),
    results=("image",),
    uses=("std.lab.plasmid",),
)
"""Performed as `capture image of <plate>`.

Binds image.
"""

synthesize = Action(
    name="synthesize",
    phrase=("synthesize", "<design>"),
    results=("fragments",),
    uses=("std.lab.plasmid",),
)
"""Performed as `synthesize <design>`.

Binds fragments.
"""

assemble = Action(
    name="assemble",
    phrase=("assemble", "<fragments>"),
    results=("construct",),
    uses=("std.lab.plasmid",),
)
"""Performed as `assemble <fragments>`.

Binds construct.
"""

provision = Action(
    name="provision",
    phrase=("provision", "<item>"),
    results=("material",),
    uses=("std.lab.plasmid",),
)
"""Performed as `provision <item>`.

Binds material.
"""

transform = Action(
    name="transform",
    phrase=("transform", "<design>", "from", "<plasmids>", "into", "<cells>"),
    results=("strain", "culture"),
    uses=("std.lab.plasmid",),
)
"""Performed as `transform <design> from <plasmids> into <cells>`.

Binds strain, culture.
"""

recover = Action(
    name="recover",
    phrase=("recover", "<culture>", "for", "<duration>"),
    results=("culture",),
    uses=("std.lab.plasmid",),
)
"""Performed as `recover <culture> for <duration>`.

Binds culture.
"""

dilute = Action(
    name="dilute",
    phrase=("dilute", "<culture>"),
    results=("culture",),
    uses=("std.lab.plasmid",),
)
"""Performed as `dilute <culture>`.

Binds culture.
"""

plate = Action(
    name="plate",
    phrase=("plate", "<culture>", "on", "<medium>"),
    results=("plate",),
    uses=("std.lab.plasmid",),
)
"""Performed as `plate <culture> on <medium>`.

Binds plate.
"""

pick = Action(
    name="pick",
    phrase=("pick", "<count>", "isolated", "colonies", "from", "<plate>"),
    results=("candidates",),
    uses=("std.lab.plasmid",),
)
"""Performed as `pick <count> isolated colonies from <plate>`.

Binds candidates.
"""

screen = Action(
    name="screen",
    phrase=("screen", "<candidates>", "against", "<design>"),
    results=("screening",),
    uses=("std.lab.plasmid",),
)
"""Performed as `screen <candidates> against <design>`.

Binds screening.
"""

grow = Action(
    name="grow",
    phrase=("grow", "<clone>", "at", "<temperature>", "for", "<duration>"),
    results=("culture",),
    uses=("std.lab.plasmid",),
)
"""Performed as `grow <clone> at <temperature> for <duration>`.

Binds culture.
"""

purify = Action(
    name="purify",
    phrase=("purify", "<culture>"),
    results=("plasmid",),
    uses=("std.lab.plasmid",),
)
"""Performed as `purify <culture>`.

Binds plasmid.
"""

split = Action(
    name="split",
    phrase=("split", "<material>"),
    results=("retained", "aliquot"),
    uses=("std.lab.plasmid",),
)
"""Performed as `split <material>`.

Binds retained, aliquot.
"""

sequence = Action(
    name="sequence",
    phrase=("sequence", "<aliquot>"),
    results=("result",),
    uses=("std.lab.plasmid",),
)
"""Performed as `sequence <aliquot>`.

Binds result.
"""

quantify = Action(
    name="quantify",
    phrase=("quantify", "<material>"),
    results=("evidence",),
    uses=("std.lab.plasmid",),
)
"""Performed as `quantify <material>`.

Binds evidence.
"""

store = Action(
    name="store",
    phrase=("store", "<material>", "at", "<temperature>"),
    results=("material",),
    uses=("std.lab.plasmid",),
)
"""Performed as `store <material> at <temperature>`.

Binds material.
"""

dispose = Action(
    name="dispose",
    phrase=("dispose", "<material>"),
    results=(),
    uses=("std.lab.plasmid",),
)
"""Performed as `dispose <material>`."""
