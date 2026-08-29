"""Lab modules and the artifacts declared in them.

A Lab module is a Python module. Its declarations are ordinary objects bound to
ordinary names, and its properties are keyword arguments:

    from lab.bio.designs import Plasmid
    from lab import circular, dna

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
from typing import TYPE_CHECKING, Any, ClassVar, Generic, TypeVar

from ._expressions import Expression, Fields, expression
from ._source import Origin, SourceMap, SourceWriter

if TYPE_CHECKING:
    from ._vocabulary import ArtifactKind

_ArtifactKindT = TypeVar("_ArtifactKindT", bound="ArtifactKind")

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

    def __init__(self, declaration: Declaration[Any] | Binding) -> None:
        self.declaration = declaration

    def render(self) -> str:
        return self.declaration.name

    def lab_modules(self) -> Iterator[str]:
        yield self.declaration.module.name


class Declaration(Generic[_ArtifactKindT]):
    """One artifact declaration: a kind, a provenance, and what it states."""

    _expected_provenance: ClassVar[str | None] = None

    def __init__(
        self,
        *,
        module: Module,
        kind: type[_ArtifactKindT],
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
        design: object | None = None,
    ) -> None:
        if self._expected_provenance is not None and provenance != self._expected_provenance:
            raise ValueError(
                f"{type(self).__name__} has provenance {self._expected_provenance!r}, "
                f"not {provenance!r}"
            )
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
        self.design = design
        self._design_prepared = design is None
        self._user_property_names = frozenset(properties)
        self._scope = scope
        self._name = name
        if design is not None:
            attach = getattr(design, "__lab_sbol_attach_declaration__", None)
            if callable(attach):
                attach(self)

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

    def prepare_design(self) -> None:
        """Materialize and read an attached SBOL design exactly once.

        A declaration takes its name from the Python binding that receives it,
        so an anonymous design cannot receive its stable identity until this
        point. Module emission calls this before it computes imports or writes
        source.
        """

        if self._design_prepared:
            return
        if self.design is None:
            raise AssertionError("a declaration without a design was marked unprepared")

        from . import _sbol

        read = _sbol.read_design(
            self.design,
            kind=self.kind,
            module=self.module,
            origin=self.origin,
            declaration_name=self.name,
            provenance=self.provenance,
            before=self,
        )
        contributed = [
            Property(name, expression(value))
            for name, value in read.properties.items()
            if name not in self._user_property_names
        ]
        self.properties = [*contributed, *self.properties]
        self.requirements = [*read.requirements, *self.requirements]
        self.doc = self.doc or read.doc
        self._design_prepared = True

    def lab_modules(self) -> Iterator[str]:
        """Every Lab module this declaration's words come from."""

        yield from self.kind.uses
        for property_ in self.properties:
            yield from property_.value.lab_modules()
        for _, _, predicate in self.claims():
            yield from predicate.lab_modules()

    def __lab_expression__(self) -> Expression:
        return DeclarationReference(self)

    def __lab_sbol_design__(self) -> object:
        """The typed SBOL design this declaration explicitly sources."""

        if self.design is None:
            raise TypeError(f"{self!r} has no attached SBOL design")
        return self.design

    def __repr__(self) -> str:
        return f"<lab {self.provenance} {self.kind.word} {self._name or 'unbound'}>"


class BuildDeclaration(Declaration[_ArtifactKindT]):
    """A typed declaration for an artifact this laboratory plans to make."""

    _expected_provenance = "build"


class BuyDeclaration(Declaration[_ArtifactKindT]):
    """A typed declaration for an artifact an external source lists."""

    _expected_provenance = "buy"


@dataclass(frozen=True)
class Case:
    """One tagged variant of a record: a name and the fields it adds."""

    name: str
    fields: Sequence[tuple[str, str]] = ()
    doc: str | None = None


class RecordDeclaration:
    """A `record` declaration: a named type, its fields, and the roles it plays.

    A record with cases is a tagged union: the fields above the cases are
    common to every variant, and each case adds its own.
    """

    def __init__(
        self,
        *,
        module: Module,
        name: str,
        roles: Sequence[str] = (),
        fields: Sequence[tuple[str, str]] = (),
        cases: Sequence[Case] = (),
        doc: str | None = None,
        role_uses: Sequence[str] = (),
        origin: Origin | None = None,
    ) -> None:
        self.module = module
        self.name = name
        self.roles = tuple(roles)
        self.fields = list(fields)
        self.cases = list(cases)
        self.doc = doc
        self.role_uses = tuple(role_uses)
        self.origin = origin

    def lab_modules(self) -> Iterator[str]:
        yield from self.role_uses

    def write(self, writer: SourceWriter) -> None:
        with writer.region(self.origin):
            writer.documentation(self.doc, "/**")
            played = f" is {', '.join(self.roles)}" if self.roles else ""
            header = f"record {self.name}{played}"
            if not self.fields and not self.cases:
                writer.line(header)
                return
            writer.line(f"{header}:")
            with writer.indented():
                for name, annotation in self.fields:
                    writer.line(f"{name}: {annotation}")
                for case in self.cases:
                    writer.line()
                    writer.documentation(case.doc, "/**")
                    if not case.fields:
                        writer.line(f"case {case.name}")
                        continue
                    writer.line(f"case {case.name}:")
                    with writer.indented():
                        for name, annotation in case.fields:
                            writer.line(f"{name}: {annotation}")

    def __repr__(self) -> str:
        return f"<lab record {self.name}>"


@dataclass(frozen=True)
class ArtifactField:
    """One property contributed by a package-local artifact kind."""

    name: str
    annotation: str
    optional: bool = False


class ArtifactKindDeclaration:
    """An `artifact` declaration connecting an instance word to a typed schema."""

    def __init__(
        self,
        *,
        module: Module,
        name: str,
        fields: Sequence[ArtifactField] = (),
        uses: Sequence[str] = (),
        origin: Origin | None = None,
    ) -> None:
        self.module = module
        self.name = name
        self.fields = list(fields)
        self.uses = tuple(uses)
        self.origin = origin

    def lab_modules(self) -> Iterator[str]:
        yield from self.uses

    def write(self, writer: SourceWriter) -> None:
        with writer.region(self.origin):
            if not self.fields:
                writer.line(f"artifact {self.name}")
                return
            writer.line(f"artifact {self.name}:")
            with writer.indented():
                for field in self.fields:
                    optional = "?" if field.optional else ""
                    writer.line(f"{field.name}{optional}: {field.annotation}")

    def __repr__(self) -> str:
        return f"<lab artifact kind {self.name}>"


class CircuitDeclaration:
    """A `circuit` declaration: typed inputs, an output type, and a layout.

    The body is the `layout:` section, whose entries are the parts the circuit
    composes in physical order.
    """

    def __init__(
        self,
        *,
        module: Module,
        name: str,
        inputs: Sequence[tuple[str, str]],
        output: str,
        layout: Sequence[object],
        doc: str | None = None,
        uses: Sequence[str] = (),
        origin: Origin | None = None,
    ) -> None:
        self.module = module
        self.name = name
        self.inputs = list(inputs)
        self.output = output
        self.layout = [expression(entry) for entry in layout]
        self.doc = doc
        self.uses = tuple(uses)
        self.origin = origin

    def lab_modules(self) -> Iterator[str]:
        yield from self.uses
        for entry in self.layout:
            yield from entry.lab_modules()

    def write(self, writer: SourceWriter) -> None:
        with writer.region(self.origin):
            writer.documentation(self.doc, "/**")
            writer.line(f"circuit {self.name}(")
            with writer.indented():
                for parameter, annotation in self.inputs:
                    writer.line(f"{parameter}: {annotation},")
            writer.line(f") -> {self.output}:")
            with writer.indented():
                writer.line("layout:")
                with writer.indented():
                    for entry in self.layout:
                        writer.line(entry.render())

    def __lab_expression__(self) -> Expression:
        return _ItemReference(self)

    def __repr__(self) -> str:
        return f"<lab circuit {self.name}>"


class WorkflowDeclaration:
    """A `workflow` declaration: typed parameters, results, and a body.

    The body arrives already translated into lines of Lab, because what a
    workflow does is control flow rather than a value, and the translation
    reads the Python function's own syntax.
    """

    def __init__(
        self,
        *,
        module: Module,
        name: str,
        inputs: Sequence[tuple[str, str]],
        results: Sequence[tuple[str, str]],
        body: Sequence[str],
        doc: str | None = None,
        uses: Sequence[str] = (),
        origin: Origin | None = None,
    ) -> None:
        self.module = module
        self.name = name
        self.inputs = list(inputs)
        self.results = list(results)
        self.body = list(body)
        self.doc = doc
        self.uses = tuple(uses)
        self.origin = origin

    def lab_modules(self) -> Iterator[str]:
        yield from self.uses

    def write(self, writer: SourceWriter) -> None:
        with writer.region(self.origin):
            writer.documentation(self.doc, "/**")
            if self.inputs:
                writer.line(f"workflow {self.name}(")
                with writer.indented():
                    for name, annotation in self.inputs:
                        writer.line(f"{name}: {annotation},")
                header = ")"
            else:
                header = f"workflow {self.name}()"
            # One result needs no name, because nothing binds it by one. Several
            # do, so a caller can say which it wants.
            if len(self.results) == 1:
                writer.line(f"{header} -> {self.results[0][1]}:")
            else:
                writer.line(f"{header} -> (")
                with writer.indented():
                    for name, annotation in self.results:
                        writer.line(f"{name}: {annotation},")
                writer.line("):")
            with writer.indented():
                for line in self.body:
                    writer.line(line)

    def __repr__(self) -> str:
        return f"<lab workflow {self.name}>"


class Binding:
    """A module-level binding, `name = value`.

    Like an artifact declaration, it takes its Lab name from the Python name it
    is bound to, so `tet_reporter = regulated_expression()` binds the same name
    in both languages.
    """

    def __init__(
        self,
        *,
        module: Module,
        value: object,
        name: str | None = None,
        annotation: str | None = None,
        doc: str | None = None,
        origin: Origin | None = None,
        scope: str | None = None,
    ) -> None:
        self.module = module
        self.value = expression(value)
        self.annotation = annotation
        self.doc = doc
        self.origin = origin
        self._scope = scope
        self._name = name

    @property
    def name(self) -> str:
        """This binding's Lab name, read from the Python name it is bound to."""

        if self._name is None:
            self._name = self._find_name()
        return self._name

    def _find_name(self) -> str:
        if self._scope is not None:
            for name, value in vars(sys.modules[self._scope]).items():
                if value is self:
                    return name
        where = self._scope or "the module that created it"
        raise LookupError(
            f"the binding declared at {self.origin} has no Lab name: "
            f"bind it to a variable in {where}, or pass name= when declaring it"
        )

    def lab_modules(self) -> Iterator[str]:
        yield from self.value.lab_modules()

    def write(self, writer: SourceWriter) -> None:
        with writer.region(self.origin):
            writer.documentation(self.doc, "/**")
            annotated = f"{self.name}: {self.annotation}" if self.annotation else self.name
            writer.line(f"{annotated} = {self.value.render()}")

    def __lab_expression__(self) -> Expression:
        return DeclarationReference(self)

    def __repr__(self) -> str:
        return f"<lab binding {self._name or 'unbound'}>"


class _ItemReference(Expression):
    """A reference to a named module item, such as a circuit being called."""

    __slots__ = ("item",)

    def __init__(self, item: CircuitDeclaration) -> None:
        self.item = item

    def render(self) -> str:
        return self.item.name

    def lab_modules(self) -> Iterator[str]:
        yield self.item.module.name


#: Everything a module can hold, in the order it is written.
ModuleItem = (
    Declaration[Any]
    | RecordDeclaration
    | ArtifactKindDeclaration
    | CircuitDeclaration
    | WorkflowDeclaration
    | Binding
)


class Module:
    """One Lab module, and the declarations written into it."""

    def __init__(self, name: str, doc: str | None = None, uses: Iterable[object] = ()) -> None:
        self.name = name
        self.doc = doc
        self.declarations: list[ModuleItem] = []
        self._preparing_designs = False
        #: Modules to import beyond the ones the declarations refer to by name.
        #: A module that only contributes properties to a schema is never named
        #: by anything, so it cannot be inferred.
        self.uses = [_module_path(item) for item in uses]

    def declare(self, declaration: ModuleItem, *, before: ModuleItem | None = None) -> None:
        """Add an item, optionally before the declaration that discovered it."""

        if before is None:
            self.declarations.append(declaration)
        else:
            self.declarations.insert(self.declarations.index(before), declaration)

    def _prepare_designs(self) -> None:
        if self._preparing_designs:
            return
        self._preparing_designs = True
        try:
            for declaration in list(self.declarations):
                if isinstance(declaration, Declaration):
                    declaration.prepare_design()
        finally:
            self._preparing_designs = False

    def imports(self) -> list[str]:
        """The modules this one imports, in the order they are first needed."""

        self._prepare_designs()
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

        self._prepare_designs()
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
            if isinstance(group, list):
                _write_group(writer, group)
            else:
                group.write(writer)
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


#: What `emit`-style grouping produces: a run of bought declarations written as
#: one block, or a single item that writes itself.
Group = (
    list[Declaration[Any]]
    | RecordDeclaration
    | ArtifactKindDeclaration
    | CircuitDeclaration
    | WorkflowDeclaration
    | Binding
)


def _provenance_groups(items: Sequence[ModuleItem]) -> list[Group]:
    """Consecutive artifact declarations sharing a provenance verb.

    A run of bought items is written as one `buy:` block, which is what the
    block form means: one origin stated over everything inside it. Every other
    kind of item stands alone and writes itself.
    """

    groups: list[Group] = []
    for item in items:
        if not isinstance(item, Declaration):
            groups.append(item)
        elif (
            groups
            and isinstance(groups[-1], list)
            and groups[-1][0].provenance == item.provenance == "buy"
        ):
            groups[-1].append(item)
        else:
            groups.append([item])
    return groups


def _write_group(writer: SourceWriter, group: Sequence[Declaration[Any]]) -> None:
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


def _write_declaration(writer: SourceWriter, declaration: Declaration[Any], *, verb: bool) -> None:
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


def _write_body(writer: SourceWriter, declaration: Declaration[Any]) -> None:
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
