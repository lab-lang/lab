"""Lab modules and the artifacts declared in them.

A Lab module is a Python module. Its declarations are ordinary objects bound to
ordinary names, and its properties are keyword arguments:

    from lab.bio.designs import Plasmid
    from lab.prelude import circular, dna

    module = lab.Module("golden_gate.designs.plasmids", doc=__doc__)

    composite_plasmid_1 = Plasmid.build(
        sequence=dna("GCTAGC"),
        backbone=pSB1C3,
        require=[lambda plasmid: plasmid.topology == circular],
    )

A declaration takes its Lab name from the Python name it is bound to, the way
`__set_name__` gives a descriptor its name, so nothing is spelled twice. Its
`use` lines are the Lab modules of the words it refers to, so importing
`Plasmid` from Python is what imports `std.bio.designs` in Lab.
"""

from __future__ import annotations

import sys
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING

from ._expressions import Expression, Fields, expression
from ._source import Origin, SourceMap, SourceWriter

if TYPE_CHECKING:
    from ._vocabulary import ArtifactKind

#: A claim, written as a function of the artifact it is about.
Predicate = Callable[[Fields], Expression]

#: The scope a claim's names are resolved in. One instance serves every claim,
#: because it holds no state: it turns attribute access into a Lab name.
_FIELDS = Fields()


@dataclass(frozen=True)
class Claim:
    """A `require` or `accept` predicate, and the evidence it is believed on.

    Wrap a predicate in one of these to give a single claim its own
    evidentiary standard; passing the predicate alone takes the declaration's.
    """

    predicate: Predicate
    across: int | None = None


@dataclass(frozen=True)
class Property:
    """`name = value` in a declaration body."""

    name: str
    value: Expression


class DeclarationReference(Expression):
    """A reference to a declaration, by the name it ends up with.

    The name is read when the reference is rendered rather than when it is
    built, because a declaration is named by the binding it is assigned to and
    that assignment has not happened yet while its own arguments are evaluated.
    """

    __slots__ = ("declaration",)

    def __init__(self, declaration: Declaration) -> None:
        self.declaration = declaration

    def render(self) -> str:
        return self.declaration.name

    def lab_modules(self) -> Iterator[str]:
        yield self.declaration.module.name


class Declaration:
    """One artifact declaration: a kind, a provenance, and what it states."""

    def __init__(
        self,
        *,
        module: Module,
        kind: ArtifactKind,
        provenance: str,
        properties: Mapping[str, object],
        name: str | None,
        doc: str | None,
        ascribed: str | None,
        requirements: Sequence[Claim],
        acceptance: Sequence[Claim],
        across: int | None,
        origin: Origin,
        scope: str,
    ) -> None:
        self.module = module
        self.kind = kind
        self.provenance = provenance
        self.doc = doc
        self.ascribed = ascribed
        self.across = across
        self.origin = origin
        self.properties = [Property(name, expression(value)) for name, value in properties.items()]
        self.requirements = list(requirements)
        self.acceptance = list(acceptance)
        self._scope = scope
        self._name = name

    @property
    def name(self) -> str:
        """This declaration's Lab name.

        Ordinarily it is the Python name the declaration is bound to, so that
        nothing is spelled twice. A declaration built in a loop has no such
        name and states its own.
        """

        if self._name is None:
            self._name = self._find_name()
        return self._name

    def _find_name(self) -> str:
        for name, value in vars(sys.modules[self._scope]).items():
            if value is self:
                return name
        raise LookupError(
            f"the {self.provenance} {self.kind.word} declared at {self.origin} has no Lab name: "
            f"bind it to a variable in {self._scope}, or pass name= when declaring it"
        )

    @property
    def has_block(self) -> bool:
        return bool(
            self.properties or self.requirements or self.acceptance or self.across is not None
        )

    def claims(self) -> Iterator[tuple[str, Claim, Expression]]:
        """Every claim with the predicate it states, in the order Lab writes them."""

        for word, group in (("require", self.requirements), ("accept", self.acceptance)):
            for claim in group:
                yield word, claim, expression(claim.predicate(_FIELDS))

    def lab_modules(self) -> Iterator[str]:
        """Every Lab module this declaration's words come from."""

        yield from self.kind.uses
        for property_ in self.properties:
            yield from property_.value.lab_modules()
        for _, _, predicate in self.claims():
            yield from predicate.lab_modules()

    def __lab_expression__(self) -> Expression:
        return DeclarationReference(self)

    def __repr__(self) -> str:
        return f"<lab {self.provenance} {self.kind.word} {self._name or 'unbound'}>"


class Module:
    """One Lab module, and the declarations written into it."""

    def __init__(self, name: str, doc: str | None = None, uses: Iterable[object] = ()) -> None:
        self.name = name
        self.doc = doc
        self.declarations: list[Declaration] = []
        #: Modules to import beyond the ones the declarations refer to by name.
        #: A module that only contributes properties to a schema is never named
        #: by anything, so it cannot be inferred.
        self.uses = [_module_path(item) for item in uses]

    def declare(self, declaration: Declaration) -> None:
        self.declarations.append(declaration)

    def imports(self) -> list[str]:
        """The modules this one imports, in the order they are first needed."""

        ordered: dict[str, None] = {}
        for path in self.uses:
            ordered.setdefault(path, None)
        for declaration in self.declarations:
            for path in declaration.lab_modules():
                ordered.setdefault(path, None)
        ordered.pop(self.name, None)
        return list(ordered)

    def emit(self) -> tuple[str, SourceMap]:
        """This module as Lab source, with the map back to its Python."""

        writer = SourceWriter()
        imports = self.imports()
        writer.documentation(self.doc, "/*!")
        if self.doc and imports:
            writer.line()
        for path in imports:
            writer.line(f"use {path}")

        for group in _provenance_groups(self.declarations):
            if writer.offset:
                writer.line()
            _write_group(writer, group)
        return writer.finish()

    def source(self) -> str:
        return self.emit()[0]

    def __repr__(self) -> str:
        return f"<lab module {self.name}: {len(self.declarations)} declarations>"


def declaring_module(depth: int, given: Module | None = None) -> tuple[Module, str]:
    """The Lab module being written `depth` frames above this call.

    One Python module holds one Lab module, so a declaration ordinarily finds
    the module it belongs to the same way it finds the name it is bound to: in
    the globals of the file that wrote it. Code that builds a module somewhere
    other than at a file's top level names it instead.
    """

    scope = sys._getframe(depth).f_globals
    if given is not None:
        return given, str(scope["__name__"])
    modules = [value for value in scope.values() if isinstance(value, Module)]
    if len(modules) != 1:
        found = "no Lab module" if not modules else f"{len(modules)} Lab modules"
        raise RuntimeError(
            f"{scope.get('__name__', '<unknown>')} holds {found}; a Python module holds "
            'exactly one, declared with lab.Module("package.module")'
        )
    return modules[0], str(scope["__name__"])


def _module_path(target: object) -> str:
    """The Lab module a `uses` entry names.

    It may be a generated mirror module, another Lab module built in Python, or
    the path itself.
    """

    if isinstance(target, Module):
        return target.name
    path = getattr(target, "LAB_MODULE", target)
    if not isinstance(path, str):
        raise TypeError(f"{target!r} does not name a Lab module")
    return path


def _provenance_groups(declarations: Sequence[Declaration]) -> list[list[Declaration]]:
    """Consecutive declarations sharing a provenance verb.

    A run of bought items is written as one `buy:` block, which is what the
    block form means: one origin stated over everything inside it.
    """

    groups: list[list[Declaration]] = []
    for declaration in declarations:
        if groups and groups[-1][0].provenance == declaration.provenance == "buy":
            groups[-1].append(declaration)
        else:
            groups.append([declaration])
    return groups


def _write_group(writer: SourceWriter, group: Sequence[Declaration]) -> None:
    if len(group) == 1:
        _write_declaration(writer, group[0], verb=True)
        return
    writer.line(f"{group[0].provenance}:")
    with writer.indented():
        for index, declaration in enumerate(group):
            # An item with a block stands apart from its neighbours, so a list
            # of bare names stays a list.
            if index and (declaration.has_block or group[index - 1].has_block):
                writer.line()
            _write_declaration(writer, declaration, verb=False)


def _write_declaration(writer: SourceWriter, declaration: Declaration, *, verb: bool) -> None:
    with writer.region(declaration.origin):
        writer.documentation(declaration.doc, "/**")
        opener = f"{declaration.provenance} " if verb else ""
        header = f"{opener}{declaration.kind.word} {declaration.name}"
        if declaration.ascribed:
            header = f"{header}: {declaration.ascribed}"
        if not declaration.has_block:
            writer.line(header)
            return
        writer.line(f"{header}:")
        with writer.indented():
            _write_body(writer, declaration)


def _write_body(writer: SourceWriter, declaration: Declaration) -> None:
    for property_ in declaration.properties:
        writer.line(f"{property_.name} = {property_.value.render()}")

    if declaration.across is not None:
        writer.line()
        writer.line(f"across {_replicates(declaration.across)}")

    previous = None
    for word, claim, predicate in declaration.claims():
        if word != previous:
            writer.line()
            previous = word
        evidence = "" if claim.across is None else f" across {_replicates(claim.across)}"
        writer.line(f"{word} {predicate.render()}{evidence}")


def _replicates(count: int) -> str:
    noun = "biological replicate" if count == 1 else "biological replicates"
    return f"{count} {noun}"
