"""The runtime types the generated standard-library mirror is built from.

A Lab module exports words; the mirror binds each to one of these. A `Symbol`
is a name, an `ArtifactKind` is a word declarations are written with, and a
`Function` is a name that is called. Each carries the Lab modules a program
using it has to import, so `use` lines are derived from the Python imports a
program actually made rather than restated by hand.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence

from ._declarations import Claim, Declaration, Module, Predicate, declaring_module
from ._expressions import Expression
from ._source import caller_origin


class Symbol(Expression):
    """A name a Lab module exports."""

    __slots__ = ("name", "uses")

    def __init__(self, *, name: str, uses: Sequence[str] = ()) -> None:
        self.name = name
        self.uses = tuple(uses)

    def render(self) -> str:
        return self.name

    def lab_modules(self) -> Iterator[str]:
        yield from self.uses

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

    __slots__ = ("produces", "properties", "uses", "word")

    def __init__(
        self,
        *,
        word: str,
        produces: str,
        uses: Sequence[str] = (),
        properties: Sequence[str] = (),
    ) -> None:
        self.word = word
        self.produces = produces
        self.uses = tuple(uses)
        self.properties = tuple(properties)

    def build(
        self,
        design: object | None = None,
        /,
        *,
        module: Module | None = None,
        name: str | None = None,
        doc: str | None = None,
        ascribed: str | None = None,
        across: int | None = None,
        require: Sequence[Predicate | Claim] = (),
        accept: Sequence[Predicate | Claim] = (),
        properties: Mapping[str, object] | None = None,
        **stated: object,
    ) -> Declaration:
        """Declare something this laboratory makes.

        It has a recipe, acceptance criteria, and a place in a build order.
        Properties are keyword arguments; `properties` states the ones whose
        names this signature has already taken, and the ones a caller holds in
        a mapping.

        `design` is a pySBOL3 component stating the design directly: its
        referenced parts become `components` in the order the document's
        `meets` constraints put them, a readable sequence becomes `sequence`,
        and circular topology becomes the requirement it already states.
        What the declaration itself adds is what SBOL has no vocabulary for:
        provenance, acceptance claims, and a place in a build order.
        """

        if design is not None:
            from . import _sbol

            found, _ = declaring_module(depth=2, given=module)
            read = _sbol.read_design(design, kind=self, module=found, origin=caller_origin(2))
            module = found
            stated = {**read.properties, **stated}
            require = [*read.requirements, *require]
            doc = doc or read.doc
        return self._declare(
            "build", module, name, doc, ascribed, across, require, accept, properties, stated
        )

    def buy(
        self,
        *,
        module: Module | None = None,
        name: str | None = None,
        doc: str | None = None,
        ascribed: str | None = None,
        properties: Mapping[str, object] | None = None,
        **stated: object,
    ) -> Declaration:
        """Declare something a supplier lists.

        It has an identity to order against and is never built, so it takes no
        claims and no build order: `require` and `accept` belong to building.
        """

        return self._declare("buy", module, name, doc, ascribed, None, (), (), properties, stated)

    def _declare(
        self,
        provenance: str,
        module: Module | None,
        name: str | None,
        doc: str | None,
        ascribed: str | None,
        across: int | None,
        require: Sequence[Predicate | Claim],
        accept: Sequence[Predicate | Claim],
        properties: Mapping[str, object] | None,
        stated: Mapping[str, object],
    ) -> Declaration:
        found, scope = declaring_module(depth=3, given=module)
        declaration = Declaration(
            module=found,
            kind=self,
            provenance=provenance,
            properties={**stated, **(properties or {})},
            name=name,
            doc=doc,
            ascribed=ascribed,
            requirements=[_claim(item) for item in require],
            acceptance=[_claim(item) for item in accept],
            across=across,
            origin=caller_origin(3),
            scope=scope,
        )
        found.declare(declaration)
        return declaration

    def __repr__(self) -> str:
        return f"<lab artifact kind {self.produces}>"


def _claim(claim: Predicate | Claim) -> Claim:
    return claim if isinstance(claim, Claim) else Claim(claim)
