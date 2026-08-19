"""Circuits written as LOICA genetic networks.

A LOICA network is how the field already designs circuits against SBOL, and it
is not one circuit. It is a set of transcription units wired together by the
gene products they express: an operator is a promoter driving a coding
sequence, and its output is the input of whichever operators respond to it.
LOICA's own SBOL export says exactly that, one TU component per operator.

Lab's `Circuit<Trigger, Product>` is one transcription unit, so a network
lowers to one Lab circuit per operator, bound into a list. The wiring is
carried by the types rather than by a separate graph: a regulator is a record
that plays both `Protein` and `Signal`, so it is the product of the unit that
expresses it and the trigger of the units that answer to it, and the compiler
checks the cascade because the second unit's promoter must respond to the
first unit's product.

    @lab.circuit
    def repressilator() -> lab.Layout:
        network = loica.GeneticNetwork()
        network.add_operator([p_lac, p_tet, p_ci])
        network.add_regulator([tetR, cI, lacI])
        return lab.layout(network, rbs=B0034, terminator=B0015)

    ring = repressilator()

Everything LOICA can build lowers: constitutive `Source` units, `Receiver` and
`Hill1` units, the two-input `Hill2` and N-input `Sum` (whose combined trigger
is `Both<A, B>`, nested for more than two), polycistronic units (whose combined
product is `Operon<A, B>`), cascades, feedback cycles, fan-in and fan-out, and
several reporters at once.

Nothing here imports LOICA or pySBOL3. The network is read structurally, so
any object with a LOICA network's shape lowers, and the `lab` package works
without either library installed.
"""

from __future__ import annotations

import sys
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass, field

from . import _naming, _terms
from ._declarations import (
    Binding,
    CircuitDeclaration,
    Declaration,
    Module,
    RecordDeclaration,
)
from ._expressions import ListLiteral, Reference, expression
from ._source import Origin, caller_origin

_DESIGNS = "std.bio.designs"

#: The signal a promoter with no input answers to. A constitutive promoter is
#: always on, which is a condition like any other, so it is named rather than
#: left as a hole in the type.
CONSTITUTIVE = "Constitutive"


@dataclass(frozen=True)
class Layout:
    """A genetic network and the shared parts each of its units is built with.

    A network names its promoters and the products they express; the ribosome
    binding site and terminator that every transcription unit needs are stated
    here, because LOICA has no slot for either.
    """

    network: object
    rbs: object
    terminator: object


def layout(network: object, *, rbs: object, terminator: object) -> Layout:
    """State the parts every transcription unit in a network is built with."""

    return Layout(network=network, rbs=rbs, terminator=terminator)


class CircuitError(TypeError):
    """A network that cannot be lowered to Lab circuits, and why."""


class NetworkBinding(Binding):
    """A whole network: the list of transcription units it is built from.

    The binding's Lab value is the list of unit bindings, which is what a
    plasmid carries as `cargo`. Each unit's own binding is reachable through
    `units`, and the characterization its operator arrived with rides along on
    the unit rather than lowering, because a catalogued promoter's schema has
    no field for Hill parameters.
    """

    def __init__(self, *, units: Sequence[UnitBinding], **rest: object) -> None:
        super().__init__(**rest)  # type: ignore[arg-type]
        self.units = list(units)

    @property
    def characterization(self) -> list[dict[str, object]]:
        """What each unit's operator was characterized with, in order."""

        return [unit.characterization for unit in self.units]


class UnitBinding(Binding):
    """One transcription unit, and the characterization its operator carried."""

    def __init__(self, *, characterization: dict[str, object], **rest: object) -> None:
        super().__init__(**rest)  # type: ignore[arg-type]
        self.characterization = characterization


@dataclass
class _Node:
    """One gene product or supplement, and the Lab record standing for it."""

    part: object
    name: str
    type_name: str = ""
    #: A product is expressed by some unit; a signal induces one. A regulator
    #: does both, which is what wires two units together.
    expressed: bool = False
    induces: bool = False


@dataclass
class _Unit:
    """One operator, resolved into the pieces its Lab circuit needs."""

    operator: object
    trigger: str
    product: str
    promoter: Declaration = field(init=False)
    coding: Declaration = field(init=False)


def circuit(fn: Callable[[], Layout]) -> Callable[..., NetworkBinding]:
    """A network written as a function returning a LOICA genetic network.

    Calling the decorated function lowers the network once into one Lab
    circuit per transcription unit, then binds the list of them.
    """

    lowered: list[_Lowered] = []

    def instantiate(*, name: str | None = None, module: Module | None = None) -> NetworkBinding:
        found = module if module is not None else _module_of(fn)
        origin = caller_origin(2)
        scope = str(sys._getframe(1).f_globals.get("__name__", fn.__module__))
        if not lowered:
            lowered.append(_lower(fn, found, origin))
        one = lowered[0]
        units = []
        for index, unit in enumerate(one.units):
            bound = UnitBinding(
                module=found,
                value=expression(one.declaration)(unit.promoter, unit.coding),
                name=_naming.free_name(
                    _naming.identifier(f"{fn.__name__}_{index + 1}"),
                    "unit",
                    _naming.taken_names(found),
                ),
                origin=origin,
                scope=scope,
                characterization=_characterization(unit.operator),
            )
            found.declare(bound)
            units.append(bound)
        binding = NetworkBinding(
            module=found,
            value=ListLiteral([expression(unit) for unit in units]),
            name=name,
            doc=fn.__doc__,
            origin=origin,
            scope=scope,
            units=units,
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


@dataclass
class _Lowered:
    """What one decorated function lowered to, minted once and then reused."""

    declaration: CircuitDeclaration
    units: list[_Unit]


def _lower(fn: Callable[[], Layout], module: Module, origin: Origin) -> _Lowered:
    """Run the network builder once and mint everything its units need."""

    built = fn()
    if not isinstance(built, Layout):
        raise CircuitError(
            f"{fn.__name__} returned {type(built).__name__}; a circuit function returns "
            "lab.layout(network, rbs=..., terminator=...), which states the shared parts "
            "LOICA has no slot for"
        )
    operators = list(getattr(built.network, "operators", ()) or ())
    if not operators:
        raise CircuitError(
            f"{fn.__name__} built a network with no operators; a circuit is a promoter "
            "driving a coding sequence, so a network needs at least one"
        )

    for operator in operators:
        _check_roles(operator, expected=_terms.PROMOTER, what="an operator")
        for output in _outputs(operator):
            _check_roles(output, expected=_terms.CDS, what="an operator's output")

    nodes = _nodes(fn.__name__, operators)
    taken = _naming.taken_names(
        module,
        prospective_uses=(_DESIGNS, *_uses_of(built.rbs), *_uses_of(built.terminator)),
    )
    _name_records(module, nodes, taken, origin)
    constitutive = (
        _mint_record(module, taken, origin, name=CONSTITUTIVE, roles=("Signal",))
        if any(not _inputs(operator) for operator in operators)
        else CONSTITUTIVE
    )

    units = [_resolve(operator, nodes, constitutive) for operator in operators]
    for unit in units:
        unit.promoter = _mint_part(
            module,
            taken,
            origin,
            component=_component(unit.operator),
            fallback=_operator_name(unit.operator),
            word="promoter",
            ascribed=f"Promoter<{unit.trigger}>",
            properties=_regulation(unit.operator),
        )
        outputs = _outputs(unit.operator)
        unit.coding = _mint_part(
            module,
            taken,
            origin,
            # A unit expressing several products has no one registry part
            # standing for all of them, so it is named for what it expresses
            # and carries no identity of its own.
            component=_component(outputs[0]) if len(outputs) == 1 else None,
            fallback="_".join([*(_node_name(output) for output in outputs), "cds"]),
            word="cds",
            ascribed=f"CDS<{unit.product}>",
            properties={},
        )

    declaration = CircuitDeclaration(
        module=module,
        name=_naming.free_name(_naming.identifier(f"{fn.__name__}_unit"), "circuit", taken),
        doc="One transcription unit: a promoter driving a coding sequence\n"
        "through the shared RBS and terminator.",
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
    return _Lowered(declaration=declaration, units=units)


def _nodes(name: str, operators: Sequence[object]) -> dict[int, _Node]:
    """Every supplement and gene product the network mentions, once each.

    Nodes are keyed by identity rather than by name, because two LOICA parts
    with the same name are two parts, and the same object reached from two
    operators is what makes those operators one network.
    """

    nodes: dict[int, _Node] = {}

    def record(part: object, *, expressed: bool = False, induces: bool = False) -> _Node:
        node = nodes.setdefault(id(part), _Node(part=part, name=_node_name(part)))
        node.expressed = node.expressed or expressed
        node.induces = node.induces or induces
        return node

    for operator in operators:
        outputs = _outputs(operator)
        if not outputs:
            raise CircuitError(
                f"an operator in {name} expresses nothing; a transcription unit's "
                "operator names the gene product it produces, the way a loica "
                "Receiver names its output"
            )
        for output in outputs:
            record(output, expressed=True)
        for entry in _inputs(operator):
            record(entry, induces=True)
    return nodes


def _name_records(module: Module, nodes: dict[int, _Node], taken: set[str], origin: Origin) -> None:
    """Declare one Lab record per node, playing the roles its wiring implies.

    A supplement induces without being expressed, so it is a `Signal`. A
    reporter is expressed without inducing, so it is a `Protein`. A regulator
    does both, and a record that plays both roles is exactly what lets one
    unit's product be another unit's trigger.

    An inducer that is not a small molecule is a gene product whichever unit
    expressed it, so it plays `Protein` too even when the network shown here
    only consumes it. That keeps a regulator the same kind of thing in the
    module that makes it and the module that answers to it.
    """

    for node in nodes.values():
        roles = []
        if node.expressed or (node.induces and not _is_chemical(node.part)):
            roles.append("Protein")
        if node.induces:
            roles.append("Signal")
        node.type_name = _mint_record(module, taken, origin, name=node.name, roles=tuple(roles))


def _is_chemical(part: object) -> bool:
    """Whether a part's SBOL component types it as a small molecule.

    A component that says nothing is taken at its word: with no types stated
    there is no ground to call the part a protein.
    """

    component = _component(part)
    if component is None:
        return True
    stated = _terms.terms(getattr(component, "types", ()) or ())
    return not stated or _terms.SIMPLE_CHEMICAL in stated


def _resolve(operator: object, nodes: dict[int, _Node], constitutive: str) -> _Unit:
    """The trigger and product types one operator's circuit is written with."""

    inputs = _inputs(operator)
    outputs = _outputs(operator)
    trigger = (
        _combine([nodes[id(part)].type_name for part in inputs], "Both") if inputs else constitutive
    )
    product = _combine([nodes[id(part)].type_name for part in outputs], "Operon")
    return _Unit(operator=operator, trigger=trigger, product=product)


def _combine(names: Sequence[str], combinator: str) -> str:
    """Several types written as one, nesting to the right.

    A promoter answering two signals answers `Both<A, B>`, and one answering
    three answers `Both<A, Both<B, C>>`, because a condition of several
    signals is itself a signal.
    """

    combined = names[-1]
    for name in reversed(names[:-1]):
        combined = f"{combinator}<{name}, {combined}>"
    return combined


def _inputs(operator: object) -> list[object]:
    """What induces an operator: none for a source, one or several otherwise."""

    found = getattr(operator, "input", None)
    if found is None:
        return []
    return list(found) if isinstance(found, list | tuple) else [found]


def _outputs(operator: object) -> list[object]:
    """What an operator expresses. A polycistronic unit expresses several."""

    found = getattr(operator, "output", None)
    if found is None:
        return []
    return list(found) if isinstance(found, list | tuple) else [found]


def _first_output(operator: object) -> object:
    return _outputs(operator)[0]


def _regulation(operator: object) -> dict[str, object]:
    """Which way a promoter answers its signal, where the network says.

    LOICA states the direction in the Hill parameters rather than in a field:
    an operator whose basal rate exceeds its regulated rate expresses less in
    the presence of its input, which is repression. A source has no input and
    so has no direction to state.
    """

    if not _inputs(operator):
        return {}
    alpha = getattr(operator, "alpha", None)
    if not isinstance(alpha, list | tuple) or len(alpha) < 2:
        return {}
    basal, regulated = alpha[0], max(alpha[1:])
    if not all(isinstance(value, int | float) for value in (basal, regulated)):
        return {}
    return {"regulation": Reference("repressed" if basal > regulated else "induced")}


def _check_roles(part: object, *, expected: str, what: str) -> None:
    """Verify a part's SBOL component agrees with the part the network uses it as.

    A component that states no roles says nothing to disagree with, so only a
    component that names its roles and omits the expected one is an error.
    Which molecule a gene product is comes from the wiring rather than from
    here: a regulator is a protein and a signal because it is expressed by one
    unit and induces another, whatever its component happens to say.
    """

    component = _component(part)
    if component is None:
        return
    stated = _terms.terms(getattr(component, "roles", ()) or ())
    if stated and expected not in stated:
        raise CircuitError(
            f"{what} carries an SBOL component whose roles are {sorted(stated)}, "
            f"not {expected}; a transcription unit is a promoter driving a coding "
            "sequence, so the component has to say what the network uses it as"
        )


def _component(part: object) -> object | None:
    return getattr(part, "sbol_comp", None)


def _node_name(part: object) -> str:
    component = _component(part)
    if component is not None:
        display = getattr(component, "display_id", None)
        if display:
            return str(display)
    name = getattr(part, "name", None)
    return str(name) if name else str(part)


def _operator_name(operator: object) -> str:
    component = _component(operator)
    if component is not None:
        display = getattr(component, "display_id", None)
        if display:
            return str(display)
    name = getattr(operator, "name", None)
    if name:
        return str(name)
    return f"p_{_node_name(_first_output(operator))}"


def _mint_record(
    module: Module, taken: set[str], origin: Origin, *, name: str, roles: tuple[str, ...]
) -> str:
    """Declare the record one node stands for, under a name nothing else holds.

    Nodes are already one per part, so two parts a network happens to give the
    same name are two records: they are two molecules, and the second is named
    for the part it plays rather than merged into the first.
    """

    minted = _naming.free_name(_naming.type_name(name), roles[0] if roles else "Record", taken)
    module.declare(RecordDeclaration(module=module, name=minted, roles=roles, origin=origin))
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
    properties: dict[str, object],
) -> Declaration:
    from .bio import designs

    kind = {"promoter": designs.Promoter, "cds": designs.CDS}[word]
    stated = dict(properties)
    if component is not None:
        identity = getattr(component, "identity", None)
        if identity:
            stated = {"identity": str(identity), **stated}
    name = _naming.free_name(_naming.identifier(fallback), word, taken)
    declaration = Declaration(
        module=module,
        kind=kind,
        provenance="buy",
        properties=stated,
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
    for parameter in ("alpha", "K", "n", "rate"):
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
