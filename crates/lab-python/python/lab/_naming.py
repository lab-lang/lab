"""Lab names for things that arrive named by someone else.

A part read from an SBOL document or a LOICA network already has a name, but
that name may not be a Lab identifier, and it may collide with a name an
imported module exports: a module cannot shadow its imports. These helpers
turn a foreign name into an identifier and pick a free spelling of it.
"""

from __future__ import annotations

import importlib
import re
from collections.abc import Iterable

from ._declarations import (
    CircuitDeclaration,
    Module,
    RecordDeclaration,
    WorkflowDeclaration,
)

_ROOT = "std"
_MIRROR_ROOT = "lab"


def identifier(name: str) -> str:
    """`name` as a Lab identifier: word characters only, not starting a digit."""

    cleaned = re.sub(r"\W", "_", name).strip("_") or "unnamed"
    return f"_{cleaned}" if cleaned[0].isdigit() else cleaned


def type_name(name: str) -> str:
    """`name` as a Lab type name, which is an identifier with a capital start."""

    cleaned = identifier(name)
    return cleaned[0].upper() + cleaned[1:]


def free_name(base: str, suffix: str, taken: set[str]) -> str:
    """A spelling of `base` no import or existing item already claims.

    The foreign name is kept whenever it is free. When it is not, the name says
    what the thing is (`pTet_promoter`), because a reader meeting the renamed
    form should still recognize both halves.
    """

    if base not in taken:
        return base
    candidate = f"{base}_{suffix}"
    counter = 2
    while candidate in taken:
        candidate = f"{base}_{suffix}{counter}"
        counter += 1
    return candidate


def taken_names(module: Module, prospective_uses: Iterable[str] = ()) -> set[str]:
    """Every name the module cannot claim for a new declaration.

    That is each name an imported standard module exports, read from the
    generated mirror, plus each name an item of the module already holds.
    """

    taken: set[str] = set()
    for path in (*module.imports(), *prospective_uses):
        taken.update(_mirror_exports(path))
    for item in module.declarations:
        if isinstance(item, RecordDeclaration | CircuitDeclaration | WorkflowDeclaration):
            taken.add(item.name)
        elif item._name is not None:
            # Artifact declarations and bindings are named by a Python
            # assignment that may not have happened yet; an unresolved name
            # cannot be known, so it cannot be reserved.
            taken.add(item._name)
    return taken


def _mirror_exports(path: str) -> set[str]:
    """The names the mirror of the Lab module at `path` binds."""

    segments = path.split(".")
    if segments[0] != _ROOT:
        return set()
    rest = segments[1:]
    if rest == ["prelude"]:
        rest = ["_prelude"]
    elif len(rest) > 1 and rest[0] == _MIRROR_ROOT:
        rest = rest[1:]
    mirror = ".".join([_MIRROR_ROOT, *rest])
    try:
        imported = importlib.import_module(mirror)
    except ImportError:
        return set()
    return {name for name in vars(imported) if not name.startswith("_") and name != "LAB_MODULE"}
