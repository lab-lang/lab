"""Typed, lazy SBOL designs for Lab programs.

The public factories in this module describe biology rather than exposing the
mutable pySBOL3 graph.  A promoter stays a :class:`Promoter`, a coding
sequence stays a :class:`CodingSequence`, and a plasmid stays a
:class:`Plasmid`; an associated sequence is carried by that typed object rather
than returned beside it in an undifferentiated tuple.

Designs and laboratory provenance are deliberately separate.  ``plasmid``
describes what a design is, while ``Plasmid.build(design=...)`` and
``Plasmid.buy(design=...)`` state how a laboratory obtains it.  Consequently a
local design does not need to repeat the Python declaration's name.  Anonymous
designs are materialized lazily under the declaration name when its Lab module
is emitted, and the generated SBOL document is validated at that boundary.

``sbol3`` remains an optional dependency.  Importing :mod:`lab` does not import
it; constructing a document reports the ``bio`` extra if pySBOL3 is absent.

A typed composite requires the source of each child to be explicit. Pass the
``BuildDeclaration`` or ``BuyDeclaration`` returned by an artifact kind into
``components=``; the document unwraps its typed design while the emitted Lab
program retains the chosen provenance.
"""

from __future__ import annotations

import importlib
from collections.abc import Sequence
from enum import Enum
from itertools import pairwise
from typing import Any, ClassVar, Protocol, TypeAlias, TypeVar, cast, runtime_checkable
from urllib.parse import urlsplit


class SbolDependencyError(ImportError):
    """The typed design layer was used without pySBOL3 installed."""


class SbolIdentityError(ValueError):
    """A lazy design could not be assigned one unambiguous SBOL identity."""


class SbolValidationError(ValueError):
    """A typed document whose generated SBOL does not validate."""


class Topology(Enum):
    """The topology stated on a composite DNA component."""

    CIRCULAR = "circular"
    LINEAR = "linear"


class DnaSequence:
    """One IUPAC DNA sequence and, once materialized, its pySBOL3 object."""

    __slots__ = ("_elements", "_owner", "_sbol3_sequence")

    def __init__(self, elements: str, owner: DnaComponent) -> None:
        self._elements = elements
        self._owner = owner
        self._sbol3_sequence: object | None = None

    @property
    def elements(self) -> str:
        return self._elements

    @property
    def sbol3_sequence(self) -> object:
        """The raw pySBOL3 sequence, materializing its design if necessary."""

        self._owner._materialize()
        if self._sbol3_sequence is None:
            raise AssertionError("a DNA sequence was not materialized with its component")
        return self._sbol3_sequence


class ProteinSequence:
    """One IUPAC protein sequence and, once materialized, its pySBOL3 object."""

    __slots__ = ("_elements", "_owner", "_sbol3_sequence")

    def __init__(self, elements: str, owner: ProteinComponent) -> None:
        self._elements = elements
        self._owner = owner
        self._sbol3_sequence: object | None = None

    @property
    def elements(self) -> str:
        return self._elements

    @property
    def sbol3_sequence(self) -> object:
        """The raw pySBOL3 sequence, materializing its design if necessary."""

        self._owner._materialize()
        if self._sbol3_sequence is None:
            raise AssertionError("a protein sequence was not materialized with its component")
        return self._sbol3_sequence


class Component:
    """A typed, potentially anonymous SBOL component design."""

    __slots__ = (
        "_declaration_name",
        "_declarations",
        "_description",
        "_document",
        "_identity",
        "_name",
        "_requested_identity",
        "_sbol3_component",
    )
    _lab_kind: ClassVar[str | None] = None
    _display_token: ClassVar[str] = "component"

    def __init__(
        self,
        document: Document,
        identity: str | None,
        *,
        name: str | None,
        description: str | None,
    ) -> None:
        self._document = document
        self._requested_identity = identity
        self._identity: str | None = None
        self._sbol3_component: object | None = None
        self._name = name
        self._description = description
        self._declaration_name: str | None = None
        self._declarations: list[object] = []

    @property
    def identity(self) -> str | None:
        """The stable SBOL identity, or ``None`` until an anonymous design is bound."""

        return self._identity

    @property
    def name(self) -> str | None:
        """The optional human-readable SBOL name."""

        return self._name

    @property
    def description(self) -> str | None:
        """The optional human-readable SBOL description."""

        return self._description

    @property
    def sbol3_component(self) -> object | None:
        """The raw pySBOL3 component, or ``None`` for an external reference."""

        self._materialize()
        return self._sbol3_component

    @property
    def is_reference(self) -> bool:
        """Whether this design only names a component outside its document."""

        return self._document._is_external_reference(self)

    def _materialize(self) -> None:
        self._document._materialize(self)

    def __lab_sbol_component__(self) -> object:
        """Supply the underlying component to Lab's raw structural reader."""

        self._materialize()
        if self._sbol3_component is None:
            raise TypeError(
                f"{self.identity!r} is an external component reference, not a complete SBOL design"
            )
        return self._sbol3_component

    def __lab_sbol_kind__(self) -> str | None:
        """The Lab artifact kind this design preserves, when one exists."""

        return self._lab_kind

    def __lab_sbol_molecule_type__(self) -> str:
        """The molecule family used by Lab's structural design reader."""

        return "component"

    def __lab_sbol_prepare__(self, declaration_name: str) -> None:
        """Bind, materialize, and validate this design for one Lab declaration."""

        self._document._prepare_root(self, declaration_name)

    def __lab_sbol_attach_declaration__(self, declaration: object) -> None:
        """Remember an explicit Lab ``build`` or ``buy`` source for this design."""

        self._declarations.append(declaration)

    def __lab_sbol_declaration__(self, module: object) -> object | None:
        """The explicit source declaration to use when composing this design."""

        local = [
            declaration
            for declaration in self._declarations
            if getattr(declaration, "module", None) is module
        ]
        candidates = local or self._declarations
        if len(candidates) > 1:
            names = ", ".join(
                repr(getattr(declaration, "_name", None) or "<unbound>")
                for declaration in candidates
            )
            raise ValueError(
                f"the SBOL design {self.identity!r} has several possible Lab sources "
                f"({names}); use a distinct design identity for each artifact"
            )
        return candidates[0] if candidates else None

    def __repr__(self) -> str:
        identity = self.identity or "<anonymous>"
        return f"<lab.sbol {type(self).__name__} {identity}>"


class DnaComponent(Component):
    """A component made of DNA."""

    __slots__ = ("_sequence",)

    def __init__(
        self,
        document: Document,
        identity: str | None,
        sequence: str | None,
        *,
        name: str | None,
        description: str | None,
    ) -> None:
        super().__init__(document, identity, name=name, description=description)
        self._sequence = None if sequence is None else DnaSequence(sequence, self)

    @property
    def sequence(self) -> DnaSequence | None:
        """The component's DNA sequence, when one was supplied."""

        return self._sequence

    def __lab_sbol_molecule_type__(self) -> str:
        return "dna"


class DnaPart(DnaComponent):
    """A DNA part with no more specific role stated."""

    _lab_kind = "part"
    _display_token = "part"
    _sbol_role: ClassVar[str | None] = None


class Promoter(DnaPart):
    """A DNA part carrying the Sequence Ontology promoter role."""

    _lab_kind = "promoter"
    _display_token = "promoter"
    _sbol_role = "promoter"


class RibosomeBindingSite(DnaPart):
    """A DNA part carrying the ribosome entry site role."""

    _display_token = "rbs"
    _sbol_role = "rbs"


class CodingSequence(DnaPart):
    """A DNA part carrying the coding-sequence role."""

    _lab_kind = "cds"
    _display_token = "cds"
    _sbol_role = "cds"


class Terminator(DnaPart):
    """A DNA part carrying the terminator role."""

    _display_token = "terminator"
    _sbol_role = "terminator"


class EngineeredRegion(DnaComponent):
    """An ordered composite of DNA components."""

    __slots__ = ("_components", "_topology")
    _display_token = "engineered_region"

    def __init__(
        self,
        document: Document,
        identity: str | None,
        components: Sequence[DnaComponent],
        sequence: str | None,
        topology: Topology,
        *,
        name: str | None,
        description: str | None,
    ) -> None:
        super().__init__(document, identity, sequence, name=name, description=description)
        self._components = tuple(components)
        self._topology = topology

    @property
    def components(self) -> tuple[DnaComponent, ...]:
        return self._components

    @property
    def topology(self) -> Topology:
        return self._topology

    def __lab_sbol_components__(self) -> tuple[DnaComponent, ...]:
        """The typed parts in this design's physical order."""

        return self._components


class Plasmid(EngineeredRegion):
    """A circular DNA design assembled from ordered DNA components."""

    _lab_kind = "plasmid"
    _display_token = "plasmid"


class Backbone(EngineeredRegion):
    """An assembly backbone, retained as a distinct type from a plasmid."""

    _lab_kind = "backbone"
    _display_token = "backbone"


class ProteinComponent(Component):
    """A protein component, which cannot be placed in a DNA layout."""

    __slots__ = ("_sequence",)
    _display_token = "protein"

    def __init__(
        self,
        document: Document,
        identity: str | None,
        sequence: str | None,
        *,
        name: str | None,
        description: str | None,
    ) -> None:
        super().__init__(document, identity, name=name, description=description)
        self._sequence = None if sequence is None else ProteinSequence(sequence, self)

    @property
    def sequence(self) -> ProteinSequence | None:
        return self._sequence

    def __lab_sbol_molecule_type__(self) -> str:
        return "protein"


_DnaPartT = TypeVar("_DnaPartT", bound=DnaPart)
_EngineeredRegionT = TypeVar("_EngineeredRegionT", bound=EngineeredRegion)


@runtime_checkable
class DesignSource(Protocol):
    """A Lab declaration carrying a typed SBOL design."""

    def __lab_sbol_design__(self) -> object: ...


DnaComponentInput: TypeAlias = DnaComponent | DesignSource


class Document:
    """A namespace and the typed SBOL designs declared within it.

    Every public argument is keyword-only.  ``identity`` is optional: an
    absolute value preserves a registry identity, a relative value is resolved
    under ``namespace``, and an omitted value is assigned from the Lab
    declaration that builds or buys the design.
    """

    def __init__(self, *, namespace: str) -> None:
        if not _absolute(namespace):
            raise ValueError(f"an SBOL namespace must be an absolute IRI, got {namespace!r}")
        self.namespace = namespace.rstrip("/")
        self._sbol3 = _load_sbol3()
        self._document: Any = self._sbol3.Document()
        self._components: list[Component] = []
        self._identities: dict[str, object] = {}
        self._materializing: set[Component] = set()

    @property
    def components(self) -> tuple[Component, ...]:
        """The typed designs in factory-call order."""

        return tuple(self._components)

    @property
    def sbol3_document(self) -> object:
        """The complete raw pySBOL3 document.

        Anonymous designs must first be attached to a Lab ``build`` or ``buy``
        declaration, because no stable identity exists before that point.
        """

        for component in self._components:
            if component.identity is not None:
                self._assign_descendant_identities(component)
        unresolved = [component for component in self._components if component.identity is None]
        if unresolved:
            raise SbolIdentityError(_unresolved_message(unresolved))
        for component in self._components:
            self._materialize(component)
        return cast(object, self._document)

    def validate(self) -> None:
        """Materialize and validate every design with a resolved identity."""

        _ = self.sbol3_document
        self._validate_materialized()

    def part(
        self,
        *,
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> DnaPart:
        """Describe a DNA part whose more specific role is not known."""

        return self._part(
            DnaPart,
            identity=identity,
            sequence=sequence,
            name=name,
            description=description,
        )

    def promoter(
        self,
        *,
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> Promoter:
        """Describe a promoter, optionally with its IUPAC DNA sequence."""

        return self._part(
            Promoter,
            identity=identity,
            sequence=sequence,
            name=name,
            description=description,
        )

    def rbs(
        self,
        *,
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> RibosomeBindingSite:
        """Describe a ribosome binding site."""

        return self._part(
            RibosomeBindingSite,
            identity=identity,
            sequence=sequence,
            name=name,
            description=description,
        )

    def cds(
        self,
        *,
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> CodingSequence:
        """Describe a coding sequence."""

        return self._part(
            CodingSequence,
            identity=identity,
            sequence=sequence,
            name=name,
            description=description,
        )

    def terminator(
        self,
        *,
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> Terminator:
        """Describe a transcription terminator."""

        return self._part(
            Terminator,
            identity=identity,
            sequence=sequence,
            name=name,
            description=description,
        )

    def engineered_region(
        self,
        *,
        components: Sequence[DnaComponentInput] = (),
        identity: str | None = None,
        sequence: str | None = None,
        topology: Topology = Topology.LINEAR,
        name: str | None = None,
        description: str | None = None,
    ) -> EngineeredRegion:
        """Describe DNA components in one unambiguous head-to-tail layout."""

        return self._composite(
            EngineeredRegion,
            identity=identity,
            components=components,
            sequence=sequence,
            topology=topology,
            name=name,
            description=description,
        )

    def plasmid(
        self,
        *,
        components: Sequence[DnaComponentInput] = (),
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> Plasmid:
        """Describe a circular plasmid from DNA components in physical order."""

        return self._composite(
            Plasmid,
            identity=identity,
            components=components,
            sequence=sequence,
            topology=Topology.CIRCULAR,
            name=name,
            description=description,
        )

    def backbone(
        self,
        *,
        components: Sequence[DnaComponentInput] = (),
        identity: str | None = None,
        sequence: str | None = None,
        topology: Topology = Topology.CIRCULAR,
        name: str | None = None,
        description: str | None = None,
    ) -> Backbone:
        """Describe an assembly backbone without conflating it with a plasmid."""

        return self._composite(
            Backbone,
            identity=identity,
            components=components,
            sequence=sequence,
            topology=topology,
            name=name,
            description=description,
        )

    def protein(
        self,
        *,
        identity: str | None = None,
        sequence: str | None = None,
        name: str | None = None,
        description: str | None = None,
    ) -> ProteinComponent:
        """Describe a protein component and retain its non-DNA type."""

        component = ProteinComponent(
            self,
            identity,
            sequence,
            name=name,
            description=description,
        )
        self._register_design(component)
        return component

    def _part(
        self,
        result: type[_DnaPartT],
        *,
        identity: str | None,
        sequence: str | None,
        name: str | None,
        description: str | None,
    ) -> _DnaPartT:
        component = result(
            self,
            identity,
            sequence,
            name=name,
            description=description,
        )
        self._register_design(component)
        return component

    def _composite(
        self,
        result: type[_EngineeredRegionT],
        *,
        identity: str | None,
        components: Sequence[DnaComponentInput],
        sequence: str | None,
        topology: Topology,
        name: str | None,
        description: str | None,
    ) -> _EngineeredRegionT:
        if not isinstance(topology, Topology):
            raise TypeError(f"topology must be a lab.sbol.Topology, got {topology!r}")
        parts: list[DnaComponent] = []
        for index, given in enumerate(components):
            part: object = given
            if not isinstance(part, DnaComponent):
                unwrap = getattr(part, "__lab_sbol_design__", None)
                part = unwrap() if callable(unwrap) else part
            if not isinstance(part, DnaComponent):
                raise TypeError(
                    f"component {index} is {type(given).__name__}, not a DNA design "
                    "or a build/buy declaration carrying one"
                )
            if part._document is not self:
                raise ValueError(
                    f"component {index} belongs to another SBOL document; "
                    "one composite must belong to one document"
                )
            parts.append(part)

        component = result(
            self,
            identity,
            tuple(parts),
            sequence,
            topology,
            name=name,
            description=description,
        )
        self._register_design(component)
        return component

    def _register_design(self, component: Component) -> None:
        if component._requested_identity is not None:
            self._assign_identity(component, self._identity(component._requested_identity))
        self._components.append(component)

    def _prepare_root(self, component: Component, declaration_name: str) -> None:
        if component._requested_identity is None:
            if (
                component._declaration_name is not None
                and component._declaration_name != declaration_name
            ):
                raise SbolIdentityError(
                    "one anonymous SBOL design was attached to declarations named "
                    f"{component._declaration_name!r} and {declaration_name!r}; give the design "
                    "an explicit identity before reusing it"
                )
            component._declaration_name = declaration_name
            expected = self._identity(declaration_name)
            if component.identity is not None and component.identity != expected:
                raise SbolIdentityError(
                    f"the anonymous design was already nested as {component.identity!r} before "
                    f"it was attached to declaration {declaration_name!r}; give a reused design "
                    "an explicit identity"
                )
            self._assign_identity(component, expected)
        self._assign_descendant_identities(component)
        self._materialize(component)
        self._validate_materialized()

    def _materialize(self, component: Component, suggested_identity: str | None = None) -> None:
        if component._document is not self:
            raise ValueError("an SBOL document cannot materialize another document's design")
        if component.identity is None:
            if suggested_identity is None:
                raise SbolIdentityError(
                    "this anonymous SBOL design has no identity yet; attach it with "
                    "Artifact.build(design=...) or Artifact.buy(design=...), or pass identity="
                )
            self._assign_identity(component, suggested_identity)
        self._assign_descendant_identities(component)
        if component._sbol3_component is not None or self._is_external_reference(component):
            return
        if component in self._materializing:
            raise ValueError("an SBOL design cannot contain itself")

        self._materializing.add(component)
        try:
            if isinstance(component, EngineeredRegion):
                self._materialize_composite(component)
            elif isinstance(component, DnaPart):
                self._materialize_dna_part(component)
            elif isinstance(component, ProteinComponent):
                self._materialize_protein(component)
            else:
                raise TypeError(f"unsupported typed SBOL design {type(component).__name__}")
        finally:
            self._materializing.remove(component)

    def _assign_descendant_identities(self, component: Component) -> None:
        """Reserve a composite's complete identity tree before mutating pySBOL3."""

        if not isinstance(component, EngineeredRegion):
            return
        identity = _required_identity(component)
        for index, part in enumerate(component.components):
            if part.identity is None:
                self._assign_identity(part, f"{identity}/{part._display_token}_{index + 1}")
            self._assign_descendant_identities(part)

    def _materialize_dna_part(self, component: DnaPart) -> None:
        identity = _required_identity(component)
        raw_sequence = self._dna_sequence(component)
        role = {
            None: None,
            "promoter": self._sbol3.SO_PROMOTER,
            "rbs": self._sbol3.SO_RBS,
            "cds": self._sbol3.SO_CDS,
            "terminator": self._sbol3.SO_TERMINATOR,
        }[component._sbol_role]
        raw = self._sbol3.Component(
            identity,
            self._sbol3.SBO_DNA,
            roles=[] if role is None else [role],
            sequences=[] if raw_sequence is None else [raw_sequence],
            name=component.name,
            description=component.description,
        )
        self._add_materialized(component, raw, raw_sequence)

    def _materialize_composite(self, component: EngineeredRegion) -> None:
        identity = _required_identity(component)
        for index, part in enumerate(component.components):
            child = f"{identity}/{part._display_token}_{index + 1}"
            self._materialize(part, child)

        raw_sequence = self._dna_sequence(component)
        features = [
            self._sbol3.SubComponent(_required_identity(part)) for part in component.components
        ]
        topology_type = {
            Topology.CIRCULAR: self._sbol3.SO_CIRCULAR,
            Topology.LINEAR: self._sbol3.SO_LINEAR,
        }[component.topology]
        raw = self._sbol3.Component(
            identity,
            [self._sbol3.SBO_DNA, topology_type],
            roles=[self._sbol3.SO_ENGINEERED_REGION],
            sequences=[] if raw_sequence is None else [raw_sequence],
            features=features,
            name=component.name,
            description=component.description,
        )
        raw.constraints = [
            self._sbol3.Constraint(self._sbol3.SBOL_MEETS, first, second)
            for first, second in pairwise(features)
        ]
        self._add_materialized(component, raw, raw_sequence)

    def _materialize_protein(self, component: ProteinComponent) -> None:
        identity = _required_identity(component)
        raw_sequence = None
        if component.sequence is not None:
            sequence_identity = f"{identity}_sequence"
            raw_sequence = self._sbol3.Sequence(
                sequence_identity,
                elements=component.sequence.elements,
                encoding=self._sbol3.IUPAC_PROTEIN_ENCODING,
            )
            component.sequence._sbol3_sequence = raw_sequence
        raw = self._sbol3.Component(
            identity,
            self._sbol3.SBO_PROTEIN,
            sequences=[] if raw_sequence is None else [raw_sequence],
            name=component.name,
            description=component.description,
        )
        self._add_materialized(component, raw, raw_sequence)

    def _dna_sequence(self, component: DnaComponent) -> object | None:
        if component.sequence is None:
            return None
        sequence_identity = f"{_required_identity(component)}_sequence"
        raw_sequence = self._sbol3.Sequence(
            sequence_identity,
            elements=component.sequence.elements,
            encoding=self._sbol3.IUPAC_DNA_ENCODING,
        )
        component.sequence._sbol3_sequence = raw_sequence
        return cast(object, raw_sequence)

    def _add_materialized(
        self, component: Component, raw_component: object, raw_sequence: object | None
    ) -> None:
        objects = [raw_component]
        if raw_sequence is not None:
            objects.append(raw_sequence)
        self._document.add(objects)
        component._sbol3_component = raw_component

    def _assign_identity(self, component: Component, identity: str) -> None:
        sequence = getattr(component, "sequence", None)
        candidates: list[tuple[str, object]] = [(identity, component)]
        if sequence is not None:
            candidates.append((f"{identity}_sequence", sequence))
        for candidate, owner in candidates:
            existing = self._identities.get(candidate)
            if existing is not None and existing is not owner:
                raise ValueError(f"the SBOL identity {candidate!r} is already in this document")
        component._identity = identity
        self._identities.update(candidates)

    def _identity(self, identity: str) -> str:
        if not identity:
            raise ValueError("an SBOL identity cannot be empty")
        return identity if _absolute(identity) else f"{self.namespace}/{identity}"

    def _is_external_reference(self, component: Component) -> bool:
        identity = component.identity
        if identity is None or identity.startswith(f"{self.namespace}/"):
            return False
        sequence = getattr(component, "sequence", None)
        parts = component.components if isinstance(component, EngineeredRegion) else ()
        return (
            sequence is None
            and component.name is None
            and component.description is None
            and not parts
        )

    def _validate_materialized(self) -> None:
        report: Any = self._document.validate()
        errors = tuple(str(error) for error in report.errors)
        if errors:
            details = "\n".join(f"- {error}" for error in errors)
            raise SbolValidationError(f"generated SBOL is invalid:\n{details}")


def _required_identity(component: Component) -> str:
    if component.identity is None:
        raise AssertionError("an SBOL component was materialized without an identity")
    return component.identity


def _unresolved_message(components: Sequence[Component]) -> str:
    kinds = ", ".join(type(component).__name__ for component in components)
    return (
        f"the document still has anonymous design(s): {kinds}; attach each one with "
        "Artifact.build(design=...) or Artifact.buy(design=...), or pass identity="
    )


def _load_sbol3() -> Any:
    try:
        return importlib.import_module("sbol3")
    except ModuleNotFoundError as error:
        if error.name != "sbol3":
            raise
        raise SbolDependencyError(
            'typed SBOL designs require pySBOL3; install "lab-compiler[bio]"'
        ) from error


def _absolute(identity: str) -> bool:
    return bool(urlsplit(identity).scheme)


# Explicit aliases make the design/artifact distinction readable in annotations
# without breaking the concise biological names already exposed by lab.sbol.
DnaComponentDesign: TypeAlias = DnaComponent
DnaPartDesign: TypeAlias = DnaPart
PromoterDesign: TypeAlias = Promoter
RibosomeBindingSiteDesign: TypeAlias = RibosomeBindingSite
CodingSequenceDesign: TypeAlias = CodingSequence
TerminatorDesign: TypeAlias = Terminator
EngineeredRegionDesign: TypeAlias = EngineeredRegion
PlasmidDesign: TypeAlias = Plasmid
BackboneDesign: TypeAlias = Backbone
ProteinDesign: TypeAlias = ProteinComponent


__all__ = [
    "Backbone",
    "BackboneDesign",
    "CodingSequence",
    "CodingSequenceDesign",
    "Component",
    "DesignSource",
    "DnaComponent",
    "DnaComponentDesign",
    "DnaComponentInput",
    "DnaPart",
    "DnaPartDesign",
    "DnaSequence",
    "Document",
    "EngineeredRegion",
    "EngineeredRegionDesign",
    "Plasmid",
    "PlasmidDesign",
    "Promoter",
    "PromoterDesign",
    "ProteinComponent",
    "ProteinDesign",
    "ProteinSequence",
    "RibosomeBindingSite",
    "RibosomeBindingSiteDesign",
    "SbolDependencyError",
    "SbolIdentityError",
    "SbolValidationError",
    "Terminator",
    "TerminatorDesign",
    "Topology",
]
