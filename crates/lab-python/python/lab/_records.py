"""Records and their cases, written as Python classes.

A Lab record is a named product of fields, and a record with cases is a
tagged union whose common fields sit above the cases. Both are classes here,
because a class is how Python already writes a named thing with typed fields:

    @lab.record
    class ColonyGrowth:
        plate: Material[Plate]
        observations: list[PlateObservation]

        @lab.case
        class Ready:
            colonies: ColonyMap

        @lab.case
        class TimedOut:
            pass

The roles a record plays are its base classes, so `class ColoniesReady(Event)`
is `record ColoniesReady is Event`. A role is a value rather than a class, so
it supplies a base through `__mro_entries__` the way `typing` generics do, and
the class Python actually builds inherits from nothing.

The decorated class is not a class afterwards. It is a `RecordType`, which
constructs values (`ColoniesReady(colonies=...)` is `ColoniesReady{colonies:
...}`) and names its cases (`ColonyGrowth.Ready(...)` is `Ready{...}`).
"""

from __future__ import annotations

import inspect
from collections.abc import Iterator, Sequence
from typing import Any

from ._declarations import Case, Module, RecordDeclaration, declaring_module
from ._expressions import Expression, Record, expression
from ._source import caller_origin
from ._types import lab_type, type_modules


class CaseType:
    """One case of a record, and the fields it adds to the common ones."""

    __slots__ = ("doc", "fields", "name", "owner")

    def __init__(self, name: str, fields: Sequence[tuple[str, Any]], doc: str | None) -> None:
        self.name = name
        self.fields = list(fields)
        self.doc = doc
        self.owner: RecordType | None = None

    def __call__(self, **fields: object) -> Expression:
        return Record(self.name, **fields)

    def render(self) -> str:
        return self.name

    def __repr__(self) -> str:
        return f"<lab case {self.name}>"


class RecordType:
    """A declared record: the type, its constructor, and its cases."""

    def __init__(
        self,
        *,
        name: str,
        roles: Sequence[str],
        fields: Sequence[tuple[str, Any]],
        cases: Sequence[CaseType],
        doc: str | None,
        role_uses: Sequence[str],
        module: Module,
    ) -> None:
        self.name = name
        self.roles = tuple(roles)
        self.fields = list(fields)
        self.cases = list(cases)
        self.doc = doc
        self.module = module
        for case in self.cases:
            case.owner = self
            setattr(self, case.name, case)
        self.declaration = RecordDeclaration(
            module=module,
            name=name,
            roles=roles,
            fields=[(field, lab_type(annotation)) for field, annotation in fields],
            cases=[
                Case(
                    name=case.name,
                    fields=[(f, lab_type(a)) for f, a in case.fields],
                    doc=case.doc,
                )
                for case in cases
            ],
            doc=doc,
            role_uses=tuple(role_uses) + tuple(self._field_modules()),
            origin=caller_origin(3),
        )
        module.declare(self.declaration)

    def _field_modules(self) -> Iterator[str]:
        for _, annotation in self.fields:
            yield from type_modules(annotation)
        for case in self.cases:
            for _, annotation in case.fields:
                yield from type_modules(annotation)

    def __call__(self, **fields: object) -> Expression:
        return Record(self.name, **fields)

    def render(self) -> str:
        return self.name

    def lab_modules(self) -> Iterator[str]:
        yield self.module.name

    def __lab_expression__(self) -> Expression:
        return expression(_Named(self.name, self.module.name))

    def __repr__(self) -> str:
        return f"<lab record {self.name}>"


class _Named(Expression):
    """A record's own name, where an expression belongs."""

    __slots__ = ("module", "name")

    def __init__(self, name: str, module: str) -> None:
        self.name = name
        self.module = module

    def render(self) -> str:
        return self.name

    def lab_modules(self) -> Iterator[str]:
        yield self.module


def case(cls: type) -> CaseType:
    """One case of the record whose body declares it."""

    return CaseType(cls.__name__, _annotations(cls), _docstring(cls))


def record(cls: type) -> RecordType:
    """A record declaration, taking its name and fields from the class."""

    module, _ = declaring_module(depth=2)
    cases = [value for value in vars(cls).values() if isinstance(value, CaseType)]
    roles = []
    uses: list[str] = []
    for base in cls.__mro__[1:]:
        # `LabRole` itself names no role; only the classes generated from one do.
        role = getattr(base, "__lab_role__", "")
        if role:
            roles.append(role)
            uses.extend(getattr(base, "__lab_uses__", ()))
    return RecordType(
        name=cls.__name__,
        roles=roles,
        fields=_annotations(cls),
        cases=cases,
        doc=_docstring(cls),
        role_uses=uses,
        module=module,
    )


def _annotations(cls: type) -> list[tuple[str, Any]]:
    """The fields a class body declares, in the order it declares them.

    Only the class's own annotations: a role supplies a base class, and
    inheriting its fields would give every record the same ones.
    """

    return list(inspect.get_annotations(cls).items())


def _docstring(cls: type) -> str | None:
    """The class's own docstring, never one inherited from a base."""

    return cls.__dict__.get("__doc__")
