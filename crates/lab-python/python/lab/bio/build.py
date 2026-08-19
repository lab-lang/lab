"""Part of the Python mirror of the Lab standard library."""

# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.

from .._effects import Action

LAB_MODULE = "std.bio.build"
"""The Lab module these names come from."""

realize = Action(
    name="realize",
    phrase=("realize", "<design>", "from", "<dependencies>"),
    results=("product",),
    optional=(("from", "<dependencies>"),),
    uses=("std.bio.build",),
)
"""Performed as `realize <design> from <dependencies>`.

Binds product.
"""
