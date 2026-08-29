"""Read typed Lab designs or raw pySBOL3 components structurally.

Typed designs can remain anonymous until a ``build`` or ``buy`` declaration is
named.  At module-emission time this reader asks the design to materialize
itself under that declaration name, then reads the same biological facts the
raw SBOL path reads: sequence, ordered components, topology, and prose.

Nothing here imports pySBOL3.  Raw components continue to work through a small
structural protocol, while the public :mod:`lab.sbol` layer keeps biological
kinds and their sequences typed. Typed composite children must already carry
an explicit Lab ``build`` or ``buy`` declaration; only an unannotated raw SBOL
import uses the language's catalogued fallback.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any, cast
from weakref import WeakKeyDictionary

from . import _naming, _terms
from ._declarations import Binding, BuyDeclaration, Claim, Declaration, Module
from ._expressions import Expression, Fields, expression
from ._source import Origin

if TYPE_CHECKING:
    from ._vocabulary import ArtifactKind

_FIELDS = Fields()

# Registry components already represented in a module. A typed occurrence may
# refine a raw occurrence's biological kind, but one IRI cannot become two
# explicitly incompatible kinds.
_CATALOGUED: WeakKeyDictionary[Module, dict[str, tuple[Declaration[Any], bool]]] = (
    WeakKeyDictionary()
)

# A sequence is a module-level DNA value, not text copied into every design
# that references it. The registry also makes repeated use of one SBOL
# sequence emit exactly one Lab binding.
_SEQUENCE_BINDINGS: WeakKeyDictionary[Module, dict[str, tuple[Binding, str]]] = WeakKeyDictionary()


class DesignError(TypeError):
    """A component that cannot be read as the requested biological design."""


class ReadDesign:
    """What one SBOL design contributes to a Lab artifact declaration."""

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


def validate_design_argument(design: object, *, kind: type[ArtifactKind]) -> None:
    """Reject an obviously incompatible design before declaring anything."""

    _check_typed_design_kind(design, kind)
    molecule = getattr(design, "__lab_sbol_molecule_type__", None)
    if callable(molecule):
        stated = cast(str, molecule())
        if stated != "dna":
            raise DesignError(
                f"a design passed to {kind.produces} must be DNA; this typed design is {stated}"
            )
        return

    if not looks_like_component(design):
        raise DesignError(
            f"a design passed to {kind.produces} must be a typed lab.sbol design "
            "or a raw pySBOL3 Component"
        )
    _check_raw_dna(design, kind)


def read_design(
    design: object,
    *,
    kind: type[ArtifactKind],
    module: Module,
    origin: Origin,
    declaration_name: str,
    provenance: str,
    before: Declaration[Any],
) -> ReadDesign:
    """Prepare and read the design attached to one declaration."""

    validate_design_argument(design, kind=kind)
    prepare = getattr(design, "__lab_sbol_prepare__", None)
    if callable(prepare):
        prepare(declaration_name)

    if _is_typed_design(design):
        return _read_typed_design(
            design,
            kind=kind,
            module=module,
            origin=origin,
            declaration_name=declaration_name,
            provenance=provenance,
            before=before,
        )
    return _read_raw_design(
        design,
        kind=kind,
        module=module,
        origin=origin,
        declaration_name=declaration_name,
        provenance=provenance,
        before=before,
    )


def _read_typed_design(
    design: object,
    *,
    kind: type[ArtifactKind],
    module: Module,
    origin: Origin,
    declaration_name: str,
    provenance: str,
    before: Declaration[Any],
) -> ReadDesign:
    properties: dict[str, object] = {}
    children = _typed_components(design)
    parts = [_typed_source(child, module) for child in children]
    if parts:
        properties["components"] = parts

    sequence = _typed_sequence(design)
    if sequence is not None:
        target, elements = sequence
        properties["sequence"] = _sequence_binding(
            module,
            origin,
            before,
            target=target,
            elements=elements,
            fallback_name=f"{declaration_name}_sequence",
        )

    identity = _typed_identity(design)
    properties["sbol_identity"] = identity
    if provenance == "buy":
        _remember_typed_buy(module, identity, before, design)

    requirements: list[Claim] = []
    topology = getattr(design, "topology", None)
    if provenance == "build" and getattr(topology, "value", None) == "circular":
        from ._prelude import circular

        requirements.append(_topology_claim(circular))

    doc = _text(design, "description") or _text(design, "name")
    return ReadDesign(properties=properties, requirements=requirements, doc=doc)


def _read_raw_design(
    design: object,
    *,
    kind: type[ArtifactKind],
    module: Module,
    origin: Origin,
    declaration_name: str,
    provenance: str,
    before: Declaration[Any],
) -> ReadDesign:
    raw = _component(design)
    types = _check_raw_dna(raw, kind)

    properties: dict[str, object] = {}
    parts = [
        _catalogued_part(module, origin, uri, before=before) for uri in _ordered_component_uris(raw)
    ]
    if parts:
        properties["components"] = parts
    sequence = _readable_sequence(raw)
    if sequence is not None:
        target, elements = sequence
        properties["sequence"] = _sequence_binding(
            module,
            origin,
            before,
            target=target,
            elements=elements,
            fallback_name=f"{declaration_name}_sequence",
        )

    identity = getattr(raw, "identity", None)
    if identity is None:
        raise DesignError(f"a design passed to {kind.produces} has no SBOL identity")
    properties["sbol_identity"] = str(identity)

    requirements: list[Claim] = []
    if provenance == "build" and _terms.CIRCULAR in types:
        from ._prelude import circular

        requirements.append(_topology_claim(circular))

    doc = _text(raw, "description") or _text(raw, "name")
    return ReadDesign(properties=properties, requirements=requirements, doc=doc)


def _check_raw_dna(design: object, kind: type[ArtifactKind]) -> set[str]:
    raw = _component(design)
    types = _terms.terms(_attribute(raw, "types"))
    if _terms.NUCLEIC_ACID not in types:
        stated: object = sorted(types) if types else "nothing"
        raise DesignError(
            f"a design passed to {kind.produces} must be DNA "
            f"({_terms.NUCLEIC_ACID}); this component's types state {stated}"
        )
    return types


def _topology_claim(topology: object) -> Claim:
    """``require topology == <topology>``, read from the design itself."""

    stated = expression(topology)
    return Claim(lambda artifact: artifact.topology == stated)


def _ordered_component_uris(design: object) -> list[str]:
    """Referenced parts in the order stated by SBOL ``meets`` constraints."""

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


def _catalogued_part(
    module: Module,
    origin: Origin,
    uri: str,
    typed: object | None = None,
    *,
    before: Declaration[Any],
) -> Declaration[Any]:
    """The catalogued declaration for a registry component, made once."""

    catalogued = _CATALOGUED.setdefault(module, {})
    typed_kind = _typed_kind(typed)
    entry = catalogued.get(uri)
    if entry is not None:
        existing, kind_was_stated = entry
        if typed_kind is None:
            return existing
        if kind_was_stated and existing.kind.word != typed_kind.word:
            raise DesignError(
                f"{uri!r} is used as both {existing.kind.produces} and "
                f"{typed_kind.produces}; one SBOL identity must keep one biological kind"
            )
        if not kind_was_stated:
            existing.kind = typed_kind
            catalogued[uri] = (existing, True)
        return existing

    from .bio import designs

    kind = typed_kind or designs.Part
    name = _naming.free_name(
        _naming.identifier(_registry_display_id(uri)), "part", _naming.taken_names(module)
    )
    declaration = BuyDeclaration(
        module=module,
        kind=kind,
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
    module.declare(declaration, before=before)
    catalogued[uri] = (declaration, typed_kind is not None)
    return declaration


def _typed_source(design: object, module: Module) -> Declaration[Any]:
    """The explicit Lab source declaration attached to a typed child design."""

    read = getattr(design, "__lab_sbol_declaration__", None)
    declaration = read(module) if callable(read) else None
    if isinstance(declaration, Declaration):
        return declaration
    kind = _typed_kind(design)
    factory = kind.produces if kind is not None else "Artifact"
    raise DesignError(
        f"component {_typed_identity(design)!r} has an SBOL design but no Lab provenance; "
        f"declare it with {factory}.buy(design=...) or {factory}.build(design=...), "
        "then pass that declaration in components="
    )


def _remember_typed_buy(
    module: Module,
    identity: str,
    declaration: Declaration[Any],
    design: object,
) -> None:
    """Preserve the one-identity/one-biological-kind invariant for explicit buys."""

    catalogued = _CATALOGUED.setdefault(module, {})
    typed_kind = _typed_kind(design)
    entry = catalogued.get(identity)
    if entry is None:
        catalogued[identity] = (declaration, typed_kind is not None)
        return
    existing, kind_was_stated = entry
    if typed_kind is not None and kind_was_stated and existing.kind.word != typed_kind.word:
        raise DesignError(
            f"{identity!r} is used as both {existing.kind.produces} and "
            f"{typed_kind.produces}; one SBOL identity must keep one biological kind"
        )
    if typed_kind is not None and not kind_was_stated:
        existing.kind = typed_kind
        catalogued[identity] = (existing, True)


def _registry_display_id(uri: str) -> str:
    """The display ID a registry IRI already gives a component."""

    segments = [segment for segment in uri.rstrip("/").split("/") if segment]
    if not segments:
        return "part"
    if segments[-1].isdigit() and len(segments) > 1:
        return segments[-2]
    return segments[-1]


def _readable_sequence(design: object) -> tuple[object, str] | None:
    """The one readable sequence carried by a raw SBOL component."""

    found: tuple[object, str] | None = None
    for entry in _attribute(design, "sequences"):
        target = entry
        lookup = getattr(entry, "lookup", None)
        if callable(lookup):
            try:
                target = lookup() or entry
            except Exception:
                target = entry
        elements = getattr(target, "elements", None)
        if elements:
            if found is not None:
                raise DesignError(
                    "the design carries more than one readable sequence; a DNA artifact "
                    "states one, so the extras have to go"
                )
            found = (target, str(elements))
    return found


def _typed_sequence(design: object) -> tuple[object, str] | None:
    sequence = getattr(design, "sequence", None)
    elements = getattr(sequence, "elements", None)
    return (sequence, str(elements)) if elements is not None else None


def _sequence_binding(
    module: Module,
    origin: Origin,
    before: Declaration[Any],
    *,
    target: object,
    elements: str,
    fallback_name: str,
) -> Binding:
    """Declare one independently named Lab DNA value for one SBOL sequence."""

    identity = getattr(target, "identity", None)
    key = f"identity:{identity}" if identity else f"object:{id(target)}"
    bindings = _SEQUENCE_BINDINGS.setdefault(module, {})
    existing = bindings.get(key)
    if existing is not None:
        binding, prior_elements = existing
        if prior_elements != elements:
            raise DesignError(
                f"SBOL sequence {identity!r} is used with conflicting elements in one module"
            )
        return binding

    from ._prelude import dna

    display_id = _registry_display_id(str(identity)) if identity else fallback_name
    name = _naming.free_name(
        _naming.identifier(display_id), "sequence", _naming.taken_names(module)
    )
    binding = Binding(
        module=module,
        value=expression(dna)(elements),
        name=name,
        annotation="DNA",
        origin=origin,
    )
    anchor = next((item for item in module.declarations if isinstance(item, Declaration)), before)
    module.declare(binding, before=anchor)
    bindings[key] = (binding, elements)
    return binding


def _typed_identity(design: object) -> str:
    identity = getattr(design, "identity", None)
    if not identity:
        raise DesignError("a typed SBOL design was read before its identity was resolved")
    return str(identity)


def _attribute(target: object, name: str) -> Sequence[object]:
    value = getattr(target, name, None)
    return list(value) if value is not None else []


def _text(target: object, name: str) -> str | None:
    value = getattr(target, name, None)
    return str(value) if value else None


def looks_like_component(candidate: object) -> bool:
    """Whether an object is plausibly a raw pySBOL3 Component."""

    if _is_typed_design(candidate):
        return True
    return not isinstance(candidate, Expression | str) and (
        hasattr(candidate, "types") and hasattr(candidate, "features")
    )


def _component(candidate: object) -> object:
    unwrap = getattr(candidate, "__lab_sbol_component__", None)
    return unwrap() if callable(unwrap) else candidate


def _is_typed_design(candidate: object) -> bool:
    return callable(getattr(candidate, "__lab_sbol_molecule_type__", None))


def _typed_components(candidate: object) -> tuple[object, ...]:
    children = getattr(candidate, "__lab_sbol_components__", None)
    if not callable(children):
        return ()
    return tuple(cast(Sequence[object], children()))


def _check_typed_design_kind(candidate: object, expected: type[ArtifactKind]) -> None:
    read = getattr(candidate, "__lab_sbol_kind__", None)
    stated = cast(str | None, read()) if callable(read) else None
    if stated is not None and stated != expected.word:
        raise DesignError(
            f"a typed SBOL {stated} cannot be passed to {expected.produces}; "
            f"pass a typed {expected.word} design"
        )


def _typed_kind(candidate: object | None) -> type[ArtifactKind] | None:
    """The Lab kind preserved by a typed SBOL child."""

    from .bio import designs

    read = getattr(candidate, "__lab_sbol_kind__", None)
    word = cast(str | None, read()) if callable(read) else None
    if word is None:
        return None
    return {
        "backbone": designs.Backbone,
        "cds": designs.CDS,
        "part": designs.Part,
        "plasmid": designs.Plasmid,
        "promoter": designs.Promoter,
    }.get(word)
