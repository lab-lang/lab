"""Designs written as pySBOL3 components.

A design here is an ordinary `sbol3.Component`: parts referenced at the
identity the registry already gave them, ordered by SBOL's own `meets`
constraints, with topology in the component's own types. Where SBOL can
already state something, the compiler reads it rather than asking for it
twice, so the component contributes `components`, `sequence`, and the
circular-topology requirement to the declaration built from it.

Each referenced part becomes a catalogued declaration whose identity is the
registry IRI, because an imported component is something a supplier lists,
not something this laboratory built.

Nothing here imports pySBOL3; the component is read structurally.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING
from weakref import WeakKeyDictionary

from . import _naming, _terms
from ._declarations import Claim, Declaration, Module
from ._expressions import Expression, Fields, expression
from ._source import Origin

if TYPE_CHECKING:
    from ._vocabulary import ArtifactKind

_FIELDS = Fields()

#: Parts already declared for a registry identity, one set per module, so a
#: design mentioning a part twice and two designs sharing one part both name
#: one declaration.
_CATALOGUED: WeakKeyDictionary[Module, dict[str, Declaration]] = WeakKeyDictionary()


class DesignError(TypeError):
    """A component that cannot be read as a design, and why."""


class ReadDesign:
    """What one SBOL component contributes to the declaration built from it."""

    def __init__(
        self,
        *,
        properties: dict[str, object],
        requirements: Sequence[Claim],
        doc: str | None,
    ) -> None:
        self.properties = properties
        self.requirements = list(requirements)
        self.doc = doc


def read_design(
    design: object, *, kind: type[ArtifactKind], module: Module, origin: Origin
) -> ReadDesign:
    """Read a pySBOL3 component into what a `build` declaration states."""

    types = _terms.terms(_attribute(design, "types"))
    if _terms.NUCLEIC_ACID not in types:
        stated = sorted(types) if types else "nothing"
        raise DesignError(
            f"a design passed to {kind.produces}.build must be DNA "
            f"({_terms.NUCLEIC_ACID}); this component's types state {stated}"
        )

    properties: dict[str, object] = {}
    parts = [_catalogued_part(module, origin, uri) for uri in _ordered_component_uris(design)]
    if parts:
        properties["components"] = parts
    elements = _sequence_elements(design)
    if elements is not None:
        from ._prelude import dna

        properties["sequence"] = expression(dna)(elements)

    requirements: list[Claim] = []
    if _terms.CIRCULAR in types:
        from ._prelude import circular

        requirements.append(_topology_claim(circular))

    doc = _text(design, "description") or _text(design, "name")
    return ReadDesign(properties=properties, requirements=requirements, doc=doc)


def _topology_claim(topology: object) -> Claim:
    """`require topology == <topology>`, read from the component's own types."""

    stated = expression(topology)
    return Claim(lambda artifact: artifact.topology == stated)


def _ordered_component_uris(design: object) -> list[str]:
    """The referenced parts, in the order the `meets` constraints state.

    SBOL's own order is a set of constraints, not a list. `subject meets
    object` says the subject's end abuts the object's start, so the walk
    starts at the feature nothing precedes and follows the chain. A design
    with no constraints keeps its feature order.
    """

    features = list(_attribute(design, "features"))
    subcomponents: dict[str, str] = {}
    for feature in features:
        instance_of = getattr(feature, "instance_of", None)
        if instance_of is None:
            continue
        identity = getattr(feature, "identity", None) or instance_of
        subcomponents[str(identity)] = str(instance_of)

    follows: dict[str, str] = {}
    preceded: set[str] = set()
    for constraint in _attribute(design, "constraints"):
        if _terms.term(getattr(constraint, "restriction", "")) != _terms.term(_terms.MEETS):
            continue
        subject = str(getattr(constraint, "subject", ""))
        object_ = str(getattr(constraint, "object", ""))
        if subject in subcomponents and object_ in subcomponents:
            follows[subject] = object_
            preceded.add(object_)

    if not follows:
        return list(subcomponents.values())

    starts = [identity for identity in subcomponents if identity not in preceded]
    if len(starts) != 1:
        raise DesignError(
            "the design's meets constraints do not order its parts into one chain; "
            f"found {len(starts)} possible first parts, and a layout needs exactly one"
        )
    ordered = []
    cursor: str | None = starts[0]
    while cursor is not None and cursor not in ordered:
        ordered.append(cursor)
        cursor = follows.get(cursor)
    if len(ordered) != len(subcomponents):
        raise DesignError(
            "the design's meets constraints order only some of its parts; every part "
            "in the chain has to meet the next for the layout to be one sequence"
        )
    return [subcomponents[identity] for identity in ordered]


def _catalogued_part(module: Module, origin: Origin, uri: str) -> Declaration:
    """The catalogued declaration for a registry part, made once per module."""

    catalogued = _CATALOGUED.setdefault(module, {})
    existing = catalogued.get(uri)
    if existing is not None:
        return existing

    from .bio import designs

    name = _naming.free_name(
        _naming.identifier(_registry_display_id(uri)), "part", _naming.taken_names(module)
    )
    declaration = Declaration(
        module=module,
        kind=designs.Part,
        provenance="buy",
        properties={"identity": uri},
        name=name,
        doc=None,
        ascribed=None,
        requirements=(),
        acceptance=(),
        across=None,
        origin=origin,
        scope=module.name,
    )
    module.declare(declaration)
    catalogued[uri] = declaration
    return declaration


def _registry_display_id(uri: str) -> str:
    """The name a registry IRI knows its part by.

    SynBioHub writes `<collection>/<display_id>/<version>`, so a trailing
    bare number is a version rather than a name.
    """

    segments = [segment for segment in uri.rstrip("/").split("/") if segment]
    if not segments:
        return "part"
    if segments[-1].isdigit() and len(segments) > 1:
        return segments[-2]
    return segments[-1]


def _sequence_elements(design: object) -> str | None:
    """The design's sequence, when the component can produce it.

    A component detached from a document holds sequence references it cannot
    resolve; only an entry that is itself a sequence object with elements is
    readable here.
    """

    found = None
    for entry in _attribute(design, "sequences"):
        target = entry
        lookup = getattr(entry, "lookup", None)
        if callable(lookup):
            try:
                target = lookup() or entry
            except Exception:
                # A reference detached from any document cannot resolve; the
                # design still stands on its components.
                target = entry
        elements = getattr(target, "elements", None)
        if elements:
            if found is not None:
                raise DesignError(
                    "the design carries more than one readable sequence; a plasmid "
                    "states one, so the extras have to go"
                )
            found = str(elements)
    return found


def _attribute(target: object, name: str) -> Sequence[object]:
    value = getattr(target, name, None)
    return list(value) if value is not None else []


def _text(target: object, name: str) -> str | None:
    value = getattr(target, name, None)
    return str(value) if value else None


def looks_like_component(candidate: object) -> bool:
    """Whether a positional argument is plausibly an SBOL component."""

    return not isinstance(candidate, Expression | str) and (
        hasattr(candidate, "types") and hasattr(candidate, "features")
    )
