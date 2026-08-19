"""Circuits written as LOICA genetic networks.

A LOICA network is how the field already designs circuits against SBOL: an
operator connects the supplement that induces it to the gene product it
expresses, and each carries its SBOL component. `lab.circuit` reads the
network and lowers it to a Lab `circuit` declaration, so the compiler reads
`Circuit<Trigger, Product>` off the network's own SBOL types rather than
asking for them a second time.

    @lab.circuit
    def regulated_expression() -> lab.Layout:
        network = loica.GeneticNetwork()
        network.add_operator(pTet)
        network.add_reporter(sfGFP)
        return lab.layout(network, rbs=B0034, terminator=B0015)

    tet_reporter = regulated_expression()

Nothing here imports LOICA or pySBOL3. The network is read structurally, so
any object with a LOICA network's shape lowers, and the `lab` package works
without either library installed.
"""

from __future__ import annotations

import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass

from . import _naming, _terms
from ._declarations import (
    Binding,
    CircuitDeclaration,
    Declaration,
    Module,
    RecordDeclaration,
)
from ._expressions import Reference, expression
from ._source import Origin, caller_origin

_DESIGNS = "std.bio.designs"


@dataclass(frozen=True)
class Layout:
    """A genetic network and the shared parts that complete its layout.

    A network names its operator and its product; the ribosome binding site
    and terminator between them are stated here, because LOICA has no slot
    for either.
    """

    network: object
    rbs: object
    terminator: object


def layout(network: object, *, rbs: object, terminator: object) -> Layout:
    """State the parts a network's layout shares across its transcription unit."""

    return Layout(network=network, rbs=rbs, terminator=terminator)


class CircuitError(TypeError):
    """A network that cannot be lowered to a Lab circuit, and why."""


class CircuitBinding(Binding):
    """A circuit instance, and the characterization its network arrived with.

    A LOICA receiver is a characterized part: its Hill parameters were
    constructor arguments. A catalogued promoter's schema has no field for
    them yet, so they ride along here rather than lowering into Lab.
    """

    def __init__(self, *, characterization: dict[str, object], **rest: object) -> None:
        super().__init__(**rest)  # type: ignore[arg-type]
        self.characterization = characterization


@dataclass
class _Lowered:
    """What one decorated function lowered to, minted once and then reused."""

    declaration: CircuitDeclaration
    promoter: Declaration
    coding: Declaration
    characterization: dict[str, object]


def circuit(fn: Callable[[], Layout]) -> Callable[..., CircuitBinding]:
    """A circuit written as a function returning a LOICA genetic network.

    Calling the decorated function lowers the network once, then binds one
    circuit instance per call: `tet_reporter = regulated_expression()` is the
    binding `tet_reporter = regulated_expression(...)` in Lab, with the
    trigger and product read off the network's SBOL.
    """

    lowered: list[_Lowered] = []

    def instantiate(*, name: str | None = None, module: Module | None = None) -> CircuitBinding:
        found = module if module is not None else _module_of(fn)
        origin = caller_origin(2)
        scope = str(sys._getframe(1).f_globals.get("__name__", fn.__module__))
        if not lowered:
            lowered.append(_lower(fn, found, origin))
        one = lowered[0]
        binding = CircuitBinding(
            module=found,
            value=expression(one.declaration)(one.promoter, one.coding),
            name=name,
            origin=origin,
            scope=scope,
            characterization=dict(one.characterization),
        )
        found.declare(binding)
        return binding

    instantiate.__name__ = fn.__name__
    instantiate.__qualname__ = fn.__qualname__
    instantiate.__doc__ = fn.__doc__
    instantiate.__module__ = fn.__module__
    return instantiate


def _module_of(fn: Callable[[], Layout]) -> Module:
    """The Lab module of the file the circuit function was written in."""

    modules = [value for value in fn.__globals__.values() if isinstance(value, Module)]
    if len(modules) != 1:
        found = "no Lab module" if not modules else f"{len(modules)} Lab modules"
        raise RuntimeError(
            f"{fn.__module__} holds {found}; a Python module holds exactly one, "
            'declared with lab.Module("package.module"), or pass module= when instantiating'
        )
    return modules[0]


def _lower(fn: Callable[[], Layout], module: Module, origin: Origin) -> _Lowered:
    """Run the network builder once and mint everything the circuit needs."""

    built = fn()
    if not isinstance(built, Layout):
        raise CircuitError(
            f"{fn.__name__} returned {type(built).__name__}; a circuit function returns "
            "lab.layout(network, rbs=..., terminator=...), which states the shared parts "
            "LOICA has no slot for"
        )
    operator = _single_operator(fn.__name__, built.network)
    trigger = _gene_end(fn.__name__, operator, "input")
    product = _gene_end(fn.__name__, operator, "output")
    _check_component(
        trigger, types=frozenset((_terms.SIMPLE_CHEMICAL,)), what="the operator's input"
    )
    _check_component(operator, roles=frozenset((_terms.PROMOTER,)), what="the operator")
    _check_component(product, roles=frozenset((_terms.CDS,)), what="the operator's output")

    taken = _naming.taken_names(
        module,
        prospective_uses=(_DESIGNS, *_uses_of(built.rbs), *_uses_of(built.terminator)),
    )
    trigger_type = _mint_record(module, taken, origin, name=_name_of(trigger), role="Signal")
    product_type = _mint_record(module, taken, origin, name=_name_of(product), role="Protein")
    promoter = _mint_part(
        module,
        taken,
        origin,
        component=_component(operator),
        fallback=_name_of(operator),
        word="promoter",
        ascribed=f"Promoter<{trigger_type}>",
    )
    coding = _mint_part(
        module,
        taken,
        origin,
        component=_component(product),
        fallback=_name_of(product),
        word="cds",
        ascribed=f"CDS<{product_type}>",
    )
    declaration = CircuitDeclaration(
        module=module,
        name=fn.__name__,
        doc=fn.__doc__,
        inputs=[
            ("promoter", "Promoter<Trigger: Signal>"),
            ("coding", "CDS<Product: Protein>"),
        ],
        output="Circuit<Trigger, Product>",
        layout=[Reference("promoter"), built.rbs, Reference("coding"), built.terminator],
        uses=(_DESIGNS,),
        origin=origin,
    )
    module.declare(declaration)
    return _Lowered(
        declaration=declaration,
        promoter=promoter,
        coding=coding,
        characterization=_characterization(operator),
    )


def _single_operator(name: str, network: object) -> object:
    operators = list(getattr(network, "operators", ()) or ())
    if len(operators) != 1:
        count = "no operators" if not operators else f"{len(operators)} operators"
        raise CircuitError(
            f"{name} built a network with {count}; a Lab circuit lowers from exactly one "
            "operator today, the receiver connecting one inducer to one product"
        )
    return operators[0]


def _gene_end(name: str, operator: object, end: str) -> object:
    found = getattr(operator, end, None)
    if found is None:
        what = "an inducer" if end == "input" else "a gene product"
        raise CircuitError(
            f"the operator in {name} has no {end}; a circuit's operator connects {what} "
            "to the circuit, the way a loica.Receiver does"
        )
    return found


def _component(part: object) -> object | None:
    return getattr(part, "sbol_comp", None)


def _name_of(part: object) -> str:
    component = _component(part)
    if component is not None:
        display = getattr(component, "display_id", None)
        if display:
            return str(display)
    name = getattr(part, "name", None)
    return str(name) if name else str(part)


def _check_component(
    part: object,
    *,
    what: str,
    types: frozenset[str] = frozenset(),
    roles: frozenset[str] = frozenset(),
) -> None:
    """Verify a part's SBOL component says what the lowering is about to assume."""

    component = _component(part)
    if component is None:
        return
    checks = (
        (types, _terms.terms(getattr(component, "types", ()) or ()), "types"),
        (roles, _terms.terms(getattr(component, "roles", ()) or ()), "roles"),
    )
    for expected, stated, vocabulary in checks:
        missing = expected - stated
        if expected and stated and missing:
            raise CircuitError(
                f"{what} carries an SBOL component whose {vocabulary} are "
                f"{sorted(stated)}, not {sorted(missing)[0]}; the trigger and product of "
                "a circuit are read off the network's own SBOL, so the component has to "
                "say what the network uses it as"
            )


def _mint_record(module: Module, taken: set[str], origin: Origin, *, name: str, role: str) -> str:
    base = _naming.type_name(name)
    for item in module.declarations:
        if isinstance(item, RecordDeclaration) and item.name == base and role in item.roles:
            return base
    minted = _naming.free_name(base, role, taken)
    module.declare(RecordDeclaration(module=module, name=minted, roles=(role,), origin=origin))
    taken.add(minted)
    return minted


def _mint_part(
    module: Module,
    taken: set[str],
    origin: Origin,
    *,
    component: object | None,
    fallback: str,
    word: str,
    ascribed: str,
) -> Declaration:
    from .bio import designs

    kind = {"promoter": designs.Promoter, "cds": designs.CDS}[word]
    properties: dict[str, object] = {}
    if component is not None:
        identity = getattr(component, "identity", None)
        if identity:
            properties["identity"] = str(identity)
    name = _naming.free_name(_naming.identifier(fallback), word, taken)
    declaration = Declaration(
        module=module,
        kind=kind,
        provenance="buy",
        properties=properties,
        name=name,
        doc=None,
        ascribed=ascribed,
        requirements=(),
        acceptance=(),
        across=None,
        origin=origin,
        scope=module.name,
    )
    module.declare(declaration)
    taken.add(name)
    return declaration


def _characterization(operator: object) -> dict[str, object]:
    """The numbers the operator arrived carrying: its Hill parameters."""

    found: dict[str, object] = {}
    for parameter in ("alpha", "K", "n"):
        value = getattr(operator, parameter, None)
        if value is not None:
            found[parameter] = value
    return found


def _uses_of(part: object) -> Iterator[str]:
    """The Lab modules a layout part's name comes from, if it has any."""

    try:
        yield from expression(part).lab_modules()
    except TypeError:
        return
