"""Python type annotations read as Lab types.

A field's type is written the way Python writes types, and means the Lab type
of the same shape: `Material[Plate]` is `Material<Plate>`, `list[X]` is
`List<X>`, and a record's own class stands for its name. Nothing here
evaluates a type; it reads what the annotation already is.

`Material[Plate]` has to be legal Python before it can be read, so a `Symbol`
answers subscripting with a type application rather than raising. That is the
same trick `typing` plays, and it is why a name from the mirror can appear in
an annotation at all.
"""

from __future__ import annotations

import types
import typing
from collections.abc import Iterator, Sequence
from typing import Any

#: Python's own names for types Lab spells differently.
_BUILTIN = {
    "list": "List",
    "str": "String",
    "int": "Integer",
    "float": "Decimal",
    "bool": "Bool",
    "NoneType": "None",
}


class LabType:
    """The base every mirrored Lab type is generated as.

    A type is a class rather than a value so that `Material[Plate]` is
    something a typechecker can read as well as the compiler: an annotation
    written with mirror names is checked by mypy the way any other annotation
    is, and the Lab type is read back off the class.
    """

    #: The Lab modules a program naming this type has to import.
    __lab_uses__: tuple[str, ...] = ()


class LabConstructor(LabType):
    """A type that is also written as a constructor, such as `Accepted`.

    `Accepted<Plasmid>` names a type and `Accepted{material: ...}` builds one,
    so the mirror is a single class: it annotates like a type and calling it
    builds the record literal.
    """

    def __new__(cls, **fields: object) -> Any:
        from ._expressions import Record

        return Record(cls.__name__, **fields)


class LabRole(LabType):
    """A role, which classifies types and may be inherited to play it.

    `class ColoniesReady(Event)` is `record ColoniesReady is Event`.
    """

    #: The Lab name of the role, read back by the record decorator.
    __lab_role__: str = ""


class LabState:
    """One state a facet admits, mirrored as a class so it may be written in an
    annotation.

    `inoculated[Medium]` is `Medium is inoculated`. Lab spells the narrowing
    with `is`, which Python reads as identity, and a type checker will not
    accept a call or a bare variable in an annotation. A generic class is what
    is left, and it is what roles are already mirrored as.
    """

    #: The Lab name of the state, read back where a declaration states it.
    __lab_state__: str = ""
    #: The Lab modules a program naming this state has to import.
    __lab_uses__: tuple[str, ...] = ()


def state_name(annotation: object) -> str | None:
    """The state a class names, if it names one."""

    return (
        annotation.__lab_state__
        if isinstance(annotation, type) and issubclass(annotation, LabState)
        else None
    )


class TypeApplication:
    """A parameterized type, such as `Material[Plate]`."""

    __slots__ = ("arguments", "constructor")

    def __init__(self, constructor: object, arguments: Sequence[object]) -> None:
        self.constructor = constructor
        self.arguments = tuple(arguments)

    def render(self) -> str:
        rendered = ", ".join(lab_type(argument) for argument in self.arguments)
        return f"{lab_type(self.constructor)}<{rendered}>"

    def __repr__(self) -> str:
        return f"<lab type {self.render()}>"


class InState:
    """A type narrowed to one facet state, such as `inoculated(Medium)`.

    Lab writes this `Medium is inoculated`, which is not something Python can
    parse. Calling the state reads the same way round and is an ordinary
    callable: the state is what you know, and the subject is what you know it
    about.
    """

    __slots__ = ("state", "subject")

    def __init__(self, subject: object, state: str) -> None:
        self.subject = subject
        self.state = state

    def render(self) -> str:
        return f"{lab_type(self.subject)} is {self.state}"

    def __repr__(self) -> str:
        return f"<lab type {self.render()}>"


def lab_type(annotation: object) -> str:
    """The Lab type an annotation states."""

    if annotation is None or annotation is type(None):
        return "None"
    if isinstance(annotation, str):
        return _from_text(annotation)
    if isinstance(annotation, (TypeApplication, InState)):
        return annotation.render()
    origin = typing.get_origin(annotation)
    if origin is not None:
        arguments = typing.get_args(annotation)
        if (state := state_name(origin)) is not None:
            return f"{lab_type(arguments[0])} is {state}"
        if origin in (types.UnionType, typing.Union):
            return " | ".join(lab_type(argument) for argument in arguments)
        rendered = ", ".join(lab_type(argument) for argument in arguments)
        return f"{lab_type(origin)}<{rendered}>"
    name = _name_of(annotation)
    if name is None:
        raise TypeError(f"{annotation!r} does not name a Lab type")
    return _BUILTIN.get(name, name)


def _name_of(annotation: object) -> str | None:
    """The name a value goes by where a type belongs.

    An expression answers any attribute with a field of itself, so it is asked
    by its type rather than probed: a mirror name is its own `name`, an
    artifact kind is the type it produces, and a Python class is its
    `__name__`.
    """

    from ._expressions import Expression

    if isinstance(annotation, Expression):
        return str(getattr(annotation, "name", None) or "")
    for attribute in ("produces", "name", "__name__"):
        found = getattr(annotation, attribute, None)
        if isinstance(found, str):
            return found
    return None


def type_modules(annotation: object) -> Iterator[str]:
    """The Lab modules the names in an annotation come from."""

    if isinstance(annotation, InState):
        yield from type_modules(annotation.subject)
        return
    if isinstance(annotation, TypeApplication):
        yield from type_modules(annotation.constructor)
        for argument in annotation.arguments:
            yield from type_modules(argument)
        return
    origin = typing.get_origin(annotation)
    if origin is not None:
        if state_name(origin) is not None:
            yield from getattr(origin, "__lab_uses__", ())
        for argument in typing.get_args(annotation):
            yield from type_modules(argument)
        return
    if isinstance(annotation, type) and issubclass(annotation, LabType):
        yield from annotation.__lab_uses__
        return
    found = getattr(annotation, "lab_modules", None)
    if callable(found):
        yield from found()
        return
    yield from getattr(annotation, "uses", ())


def _from_text(annotation: str) -> str:
    """A forward reference, translated by its shape.

    A quoted annotation is a name the module could not evaluate yet, so it is
    read as text: the brackets Python writes a type argument with become the
    angle brackets Lab writes one with.
    """

    text = annotation.strip()
    for python, lab in (("[", "<"), ("]", ">")):
        text = text.replace(python, lab)
    for python, lab in _BUILTIN.items():
        if text.startswith(f"{python}<"):
            text = f"{lab}<{text[len(python) + 1 :]}"
    return text


def result_types(annotation: object) -> list[tuple[str | None, str]]:
    """A workflow's results: one per value it returns, named where Lab names them.

    Lab names the results of a workflow that returns several, because a
    caller binds them by name. A tuple annotation is that list; anything else
    is the single result, which needs no name.
    """

    origin = typing.get_origin(annotation)
    if origin is tuple:
        return [(None, lab_type(argument)) for argument in typing.get_args(annotation)]
    if isinstance(annotation, str) and annotation.strip().startswith("tuple["):
        inner = annotation.strip()[len("tuple[") : -1]
        return [(None, _from_text(part.strip())) for part in _split(inner)]
    return [(None, lab_type(annotation))]


def _split(text: str) -> list[str]:
    """Split a type argument list on the commas that are not inside brackets."""

    parts: list[str] = []
    depth = 0
    current = ""
    for character in text:
        if character in "[<":
            depth += 1
        elif character in "]>":
            depth -= 1
        if character == "," and depth == 0:
            parts.append(current)
            current = ""
            continue
        current += character
    if current.strip():
        parts.append(current)
    return parts


def annotation_of(value: Any) -> str:
    """The Lab type of an already-evaluated annotation value."""

    return lab_type(value)
