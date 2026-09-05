"""Lab expressions, built from Python operators and rendered as Lab source.

A claim about an artifact is a predicate the compiler checks, not a boolean
Python can evaluate, so comparison operators build expressions here rather than
comparing. This is the same shape a query builder takes: `plasmid.topology ==
circular` describes a comparison instead of performing one.
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from typing import Protocol, runtime_checkable

_STRING_ESCAPES = {'"': '\\"', "\\": "\\\\", "\n": "\\n", "\t": "\\t"}


@runtime_checkable
class Expressible(Protocol):
    """Anything that stands for a Lab expression.

    A declaration is expressible as its own name, which is what lets a strain
    name the plasmid it carries by referring to the Python object that declared
    it.
    """

    def __lab_expression__(self) -> Expression: ...


class Expression:
    """One Lab expression."""

    __slots__ = ()

    __hash__ = object.__hash__

    def render(self) -> str:
        """This expression as Lab source."""

        raise NotImplementedError

    def children(self) -> Sequence[Expression]:
        """The expressions this one is built from."""

        return ()

    def lab_modules(self) -> Iterator[str]:
        """Every Lab module the names in this expression come from.

        A `use` line is the consequence of referring to a word, so the imports
        a module needs are read back from what its declarations say rather than
        listed again beside them.
        """

        for child in self.children():
            yield from child.lab_modules()

    def operand(self) -> str:
        """This expression where a larger expression's operand belongs.

        Compound expressions parenthesize themselves, so the precedence of the
        emitted source never depends on how the Python that built it was
        written.
        """

        return self.render()

    def __lab_expression__(self) -> Expression:
        return self

    def __repr__(self) -> str:
        return f"<lab expression: {self.render()}>"

    def __getattr__(self, name: str) -> Expression:
        if name.startswith("_"):
            raise AttributeError(name)
        return Field(self, name)

    def __call__(self, *arguments: object, **named: object) -> Expression:
        return Call(
            self,
            [Argument(None, expression(value)) for value in arguments]
            + [Argument(name, expression(value)) for name, value in named.items()],
        )

    def __eq__(self, other: object) -> Expression:  # type: ignore[override]
        return Binary("==", self, expression(other))

    def __ne__(self, other: object) -> Expression:  # type: ignore[override]
        return Binary("!=", self, expression(other))

    def __lt__(self, other: object) -> Expression:
        return Binary("<", self, expression(other))

    def __le__(self, other: object) -> Expression:
        return Binary("<=", self, expression(other))

    def __gt__(self, other: object) -> Expression:
        return Binary(">", self, expression(other))

    def __ge__(self, other: object) -> Expression:
        return Binary(">=", self, expression(other))

    def __add__(self, other: object) -> Expression:
        return Binary("+", self, expression(other))

    def __sub__(self, other: object) -> Expression:
        return Binary("-", self, expression(other))

    def __mul__(self, other: object) -> Expression:
        return Binary("*", self, expression(other))

    def __truediv__(self, other: object) -> Expression:
        return Binary("/", self, expression(other))

    # Python cannot overload `and`, `or`, and `not`, so the bitwise operators
    # carry Lab's boolean connectives, as they do in every Python library that
    # builds predicates. `and_`, `or_`, and `not_` spell the same thing for a
    # reader who would rather see the word.
    def __and__(self, other: object) -> Expression:
        return Binary("and", self, expression(other))

    def __or__(self, other: object) -> Expression:
        return Binary("or", self, expression(other))

    def __invert__(self) -> Expression:
        return Unary("not", self)


class Reference(Expression):
    """A name, resolved by the Lab compiler rather than by Python."""

    __slots__ = ("name",)

    def __init__(self, name: str) -> None:
        self.name = name

    def render(self) -> str:
        return self.name


class Fields:
    """The names in scope inside a declaration's claims.

    A claim is written as a function of the artifact it is about, so the
    artifact's properties are reached through that function's parameter rather
    than appearing from nowhere:

        require=[lambda plasmid: plasmid.topology == designs.circular]

    Attribute access here yields the bare Lab name, which is what the same
    property is called in Lab's own claim syntax.
    """

    __slots__ = ()

    def __getattr__(self, name: str) -> Expression:
        if name.startswith("_"):
            raise AttributeError(name)
        return Reference(name)


class Integer(Expression):
    __slots__ = ("value",)

    def __init__(self, value: int) -> None:
        self.value = value

    def render(self) -> str:
        return str(self.value)


class Decimal(Expression):
    __slots__ = ("text",)

    def __init__(self, text: str) -> None:
        self.text = text

    def render(self) -> str:
        return self.text


class String(Expression):
    __slots__ = ("value",)

    def __init__(self, value: str) -> None:
        self.value = value

    def render(self) -> str:
        escaped = "".join(_STRING_ESCAPES.get(character, character) for character in self.value)
        return f'"{escaped}"'


class Quantity(Expression):
    """A measurement in a stated unit, such as `20 uL`."""

    __slots__ = ("magnitude", "unit")

    def __init__(self, magnitude: object, unit: str) -> None:
        self.magnitude = expression(magnitude)
        self.unit = unit

    def children(self) -> Sequence[Expression]:
        return (self.magnitude,)

    def render(self) -> str:
        return f"{self.magnitude.operand()} {self.unit}"

    def __truediv__(self, other: object) -> Expression:
        if isinstance(other, Unit):
            return Quantity(self.magnitude, f"{self.unit}/{other.name}")
        return Binary("/", self, expression(other))


class Unit:
    """A unit of measure, which becomes a quantity when a magnitude meets it.

    Units are not expressions. `uL` alone says nothing; `20 * uL` does. This is
    the arrangement every Python units library settles on, and it keeps the
    magnitude and the unit in the order Lab writes them.
    """

    __slots__ = ("name",)

    def __init__(self, name: str) -> None:
        self.name = name

    def __repr__(self) -> str:
        return f"<lab unit: {self.name}>"

    def __rmul__(self, magnitude: object) -> Quantity:
        return Quantity(magnitude, self.name)

    def __truediv__(self, other: Unit) -> Unit:
        return Unit(f"{self.name}/{other.name}")


class ListLiteral(Expression):
    __slots__ = ("elements",)

    def __init__(self, elements: Sequence[Expression]) -> None:
        self.elements = list(elements)

    def children(self) -> Sequence[Expression]:
        return self.elements

    def render(self) -> str:
        return f"[{', '.join(element.render() for element in self.elements)}]"


class Argument:
    """One argument in a call, named where the callee's parameter is named."""

    __slots__ = ("name", "value")

    def __init__(self, name: str | None, value: Expression) -> None:
        self.name = name
        self.value = value

    def render(self) -> str:
        return self.value.render() if self.name is None else f"{self.name}: {self.value.render()}"


class Call(Expression):
    __slots__ = ("arguments", "callee")

    def __init__(self, callee: Expression, arguments: Sequence[Argument]) -> None:
        self.callee = callee
        self.arguments = list(arguments)

    def children(self) -> Sequence[Expression]:
        return [self.callee, *(argument.value for argument in self.arguments)]

    def render(self) -> str:
        rendered = ", ".join(argument.render() for argument in self.arguments)
        return f"{self.callee.operand()}({rendered})"


class Record(Expression):
    """Typed data construction, written `Constructor{ field: value }`."""

    __slots__ = ("constructor", "fields")

    def __init__(self, constructor: str, **fields: object) -> None:
        self.constructor = constructor
        self.fields = {name: expression(value) for name, value in fields.items()}

    def children(self) -> Sequence[Expression]:
        return list(self.fields.values())

    def render(self) -> str:
        rendered = ", ".join(f"{name}: {value.render()}" for name, value in self.fields.items())
        return f"{self.constructor}{{{rendered}}}"


class Field(Expression):
    __slots__ = ("field", "subject")

    def __init__(self, subject: Expression, field: str) -> None:
        self.subject = subject
        self.field = field

    def children(self) -> Sequence[Expression]:
        return (self.subject,)

    def render(self) -> str:
        return f"{self.subject.operand()}.{self.field}"


class Unary(Expression):
    __slots__ = ("inner", "operator")

    def __init__(self, operator: str, inner: Expression) -> None:
        self.operator = operator
        self.inner = inner

    def children(self) -> Sequence[Expression]:
        return (self.inner,)

    def render(self) -> str:
        return f"{self.operator} {self.inner.operand()}"

    def operand(self) -> str:
        return f"({self.render()})"


class Binary(Expression):
    __slots__ = ("left", "operator", "right")

    def __init__(self, operator: str, left: Expression, right: Expression) -> None:
        self.operator = operator
        self.left = left
        self.right = right

    def children(self) -> Sequence[Expression]:
        return (self.left, self.right)

    def render(self) -> str:
        return f"{self.left.operand()} {self.operator} {self.right.operand()}"

    def operand(self) -> str:
        return f"({self.render()})"


def expression(value: object) -> Expression:
    """The Lab expression a Python value stands for."""

    from ._types import state_name

    if (state := state_name(value)) is not None:
        return Reference(state)
    if isinstance(value, Expression):
        return value
    if isinstance(value, Expressible):
        return value.__lab_expression__()
    if isinstance(value, bool):
        raise TypeError("Lab has no boolean literal; state the claim the value came from")
    if isinstance(value, int):
        return Integer(value)
    if isinstance(value, float):
        return Decimal(repr(value))
    if isinstance(value, str):
        return String(value)
    if isinstance(value, list | tuple):
        return ListLiteral([expression(element) for element in value])
    raise TypeError(f"{type(value).__name__} is not a Lab expression")


def and_(*operands: object) -> Expression:
    return _connective("and", operands)


def or_(*operands: object) -> Expression:
    return _connective("or", operands)


def not_(operand: object) -> Expression:
    return Unary("not", expression(operand))


def _connective(operator: str, operands: Sequence[object]) -> Expression:
    if len(operands) < 2:
        raise TypeError(f"'{operator}' joins at least two expressions")
    result = expression(operands[0])
    for operand in operands[1:]:
        result = Binary(operator, result, expression(operand))
    return result
