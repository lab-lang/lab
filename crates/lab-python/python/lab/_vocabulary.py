"""The runtime types the generated standard-library mirror is built from.

A Lab module exports words; the mirror binds each to one of these. A `Symbol`
is a name, an `ArtifactKind` is a word declarations are written with, and a
`Function` is a name that is called. Each carries the Lab modules a program
using it has to import, so `use` lines are derived from the Python imports a
program actually made rather than restated by hand.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from typing import ClassVar, TypeVar

from ._declarations import (
    BuildDeclaration,
    BuyDeclaration,
    Claim,
    Module,
    Predicate,
    declaring_module,
)
from ._expressions import Expression
from ._source import caller_origin
from ._types import TypeApplication

_ArtifactKindT = TypeVar("_ArtifactKindT", bound="ArtifactKind")


class Symbol(Expression):
    """A name a Lab module exports."""

    __slots__ = ("_role_base", "name", "uses")

    def __init__(self, *, name: str, uses: Sequence[str] = ()) -> None:
        self.name = name
        self.uses = tuple(uses)
        self._role_base: type | None = None

    def render(self) -> str:
        return self.name

    def lab_modules(self) -> Iterator[str]:
        yield from self.uses

    def __getitem__(self, arguments: object) -> TypeApplication:
        """`Material[Plate]`, which is `Material<Plate>` written in Python.

        A type argument has to be legal Python before it can be read back, so
        a name answers subscripting instead of refusing it.
        """

        given = arguments if isinstance(arguments, tuple) else (arguments,)
        return TypeApplication(self, given)

    def __mro_entries__(self, bases: tuple[type, ...]) -> tuple[type, ...]:
        """The base a record inherits to say it plays this role.

        A role is a value rather than a class, so writing it as a base needs
        a stand-in. The class Python builds carries the role's name, and the
        record decorator reads it back from there.
        """

        if self._role_base is None:
            self._role_base = type(
                f"_role_{self.name}",
                (),
                {"__lab_role__": self.name, "__lab_uses__": self.uses},
            )
        return (self._role_base,)

    def __repr__(self) -> str:
        return f"<lab symbol {self.name}>"


class Function(Symbol):
    """A pure function a Lab module exports, such as `dna`."""

    __slots__ = ()

    def __repr__(self) -> str:
        return f"<lab function {self.name}>"


class ArtifactKind:
    """A kind of artifact, and the word its declarations are written with.

    The Python name is the type its instances have, because that is the name a
    reader knows the thing by; `word` is what Lab writes a declaration with.
    Several modules describe one kind, so `uses` carries every module a
    declaration made from this one has to import.
    """

    #: The word Lab writes a declaration of this kind with.
    word: ClassVar[str] = ""
    #: The name of the type its instances have, which is the class's own.
    produces: ClassVar[str] = ""
    #: Every Lab module a declaration made from this kind has to import.
    uses: ClassVar[tuple[str, ...]] = ()
    #: The property names the kind's schema contributes.
    properties: ClassVar[tuple[str, ...]] = ()

    @classmethod
    def build(
        cls: type[_ArtifactKindT],
        *,
        design: object | None = None,
        module: Module | None = None,
        name: str | None = None,
        doc: str | None = None,
        ascribed: str | None = None,
        across: int | None = None,
        require: Sequence[Predicate | Claim] = (),
        accept: Sequence[Predicate | Claim] = (),
        properties: Mapping[str, object] | None = None,
        **stated: object,
    ) -> BuildDeclaration[_ArtifactKindT]:
        """Declare something this laboratory makes.

        It has a recipe, acceptance criteria, and a place in a build order.
        Properties are keyword arguments; `properties` states the ones whose
        names this signature has already taken, and the ones a caller holds in
        a mapping.

        `design` is a typed `lab.sbol` design or a raw pySBOL3 component. A
        typed anonymous design takes this declaration's eventual Python name,
        then materializes and validates during module emission. Its explicitly
        sourced parts become `components`, a readable sequence becomes
        `sequence`, and circular topology becomes the requirement it already
        states. What the declaration itself adds is what SBOL has no vocabulary
        for: provenance, acceptance claims, and a place in a build order.
        """

        found, scope = declaring_module(depth=2, given=module)
        if design is not None:
            from . import _sbol

            _sbol.validate_design_argument(design, kind=cls)
        declaration = BuildDeclaration(
            module=found,
            kind=cls,
            provenance="build",
            properties={**stated, **(properties or {})},
            name=name,
            doc=doc,
            ascribed=ascribed,
            requirements=[_claim(item) for item in require],
            acceptance=[_claim(item) for item in accept],
            across=across,
            origin=caller_origin(2),
            scope=scope,
            design=design,
        )
        found.declare(declaration)
        return declaration

    @classmethod
    def buy(
        cls: type[_ArtifactKindT],
        *,
        design: object | None = None,
        module: Module | None = None,
        name: str | None = None,
        doc: str | None = None,
        ascribed: str | None = None,
        properties: Mapping[str, object] | None = None,
        **stated: object,
    ) -> BuyDeclaration[_ArtifactKindT]:
        """Declare something a supplier lists.

        It has a supplier identity to resolve or order against and is never
        built, so it takes no claims and no build order: `require` and `accept`
        belong to building. A typed `design` contributes its independent SBOL
        Component identity, sequence, and other biological facts without making
        the design factory itself imply procurement.
        """

        found, scope = declaring_module(depth=2, given=module)
        if design is not None:
            from . import _sbol

            _sbol.validate_design_argument(design, kind=cls)
        declaration = BuyDeclaration(
            module=found,
            kind=cls,
            provenance="buy",
            properties={**stated, **(properties or {})},
            name=name,
            doc=doc,
            ascribed=ascribed,
            requirements=(),
            acceptance=(),
            across=None,
            origin=caller_origin(2),
            scope=scope,
            design=design,
        )
        found.declare(declaration)
        return declaration

    def __init_subclass__(cls, **rest: object) -> None:
        """A kind names the type its instances have, which is its own name."""

        super().__init_subclass__(**rest)
        if not cls.produces:
            cls.produces = cls.__name__


def _claim(claim: Predicate | Claim) -> Claim:
    return claim if isinstance(claim, Claim) else Claim(claim)
