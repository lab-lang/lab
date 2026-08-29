"""Typed declaration factories for package-local artifact kinds."""

from __future__ import annotations

import re
import types
import typing
from collections.abc import Iterator
from typing import Any

from ._declarations import (
    ArtifactField,
    ArtifactKindDeclaration,
    RecordDeclaration,
    declaring_module,
)
from ._source import caller_origin
from ._types import LabType, lab_type, type_modules
from ._vocabulary import ArtifactKind


def artifact(name: str, **properties: object) -> type[ArtifactKind]:
    """Declare a package-local artifact kind and return its typed declaration API.

    Keyword values are the kind's property types. `T | None` makes a property
    optional; every other type is required. `name` is the produced Lab type, and
    its snake-case spelling is the artifact declaration word.
    """

    module, _ = declaring_module(depth=2)
    annotations = list(properties.items())
    fields = [_field(field, annotation) for field, annotation in annotations]
    uses = tuple(_field_modules(annotations))
    origin = caller_origin(2)
    module.declare(
        RecordDeclaration(
            module=module,
            name=name,
            origin=origin,
        )
    )
    module.declare(
        ArtifactKindDeclaration(
            module=module,
            name=name,
            fields=fields,
            uses=uses,
            origin=origin,
        )
    )
    return type(
        name,
        (ArtifactKind, LabType),
        {
            "__module__": module.name,
            "word": _snake_case(name),
            "uses": (module.name,),
            "__lab_uses__": (module.name,),
            "properties": tuple(field.name for field in fields),
        },
    )


def _field(name: str, annotation: object) -> ArtifactField:
    origin = typing.get_origin(annotation)
    arguments = typing.get_args(annotation)
    if origin in (typing.Union, types.UnionType) and type(None) in arguments:
        required = tuple(argument for argument in arguments if argument is not type(None))
        if len(required) != 1:
            raise TypeError(
                f"optional artifact property '{name}' must name exactly one non-None type"
            )
        return ArtifactField(name=name, annotation=lab_type(required[0]), optional=True)
    return ArtifactField(name=name, annotation=lab_type(annotation))


def _field_modules(annotations: list[tuple[str, Any]]) -> Iterator[str]:
    for _, annotation in annotations:
        yield from type_modules(annotation)


def _snake_case(name: str) -> str:
    words = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", words).lower()
