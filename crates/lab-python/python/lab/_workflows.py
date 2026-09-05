"""Workflows written as Python functions.

A workflow is the one place Lab's shape and Python's diverge enough that the
object model cannot carry it. A design is a value and can be built by calling
things; a workflow is control flow, and `if`, `match`, and `return` are
statements Python evaluates rather than records. So a workflow is read from
the function's own syntax:

    @lab.workflow
    def build_reporter(wf) -> tuple[Material[Strain], Material[Plate]]:
        product = wf.perform(realize(reporter))
        cells = wf.perform(provision(DH5alpha))
        strain, culture = wf.perform(transform(host, plasmids=[product], cells=cells))
        return strain, plate

Statements are translated from the syntax; expressions are evaluated. That
split is what makes the rest of the SDK reusable here: `30 * minutes` is a
quantity because Python multiplied it, a record constructor builds a record
because it was called, and the `use` lines still fall out of the names an
expression happens to mention.

The first parameter is the workflow context, `wf`, and carries the two things
Lab spells with keywords: `wf.perform` is `<-`, and everything else is `=`.
"""

from __future__ import annotations

import ast
import inspect
import textwrap
from collections.abc import Callable, Iterator, Sequence
from typing import Any

from . import _naming
from ._declarations import Module, WorkflowDeclaration, declaring_module
from ._effects import Effect
from ._expressions import Decimal, Expression, Field, Integer, Quantity, Reference, expression
from ._source import Origin, SourceWriter, caller_origin
from ._types import lab_type, result_types, type_modules

#: The Lab name of the context a workflow body reads its elapsed time from.
_CONTEXT = "workflow"


class WorkflowError(TypeError):
    """A workflow that cannot be translated, and why."""


class Workflow:
    """A declared workflow, callable from another workflow as a durable step."""

    def __init__(self, declaration: WorkflowDeclaration) -> None:
        self.declaration = declaration
        self.name = declaration.name
        self.results = tuple(name for name, _ in declaration.results)

    def __call__(self, *arguments: object) -> WorkflowCall:
        expected = len(self.declaration.inputs)
        if len(arguments) != expected:
            raise TypeError(
                f"{self.name} takes {expected} argument(s), {len(arguments)} were given"
            )
        return WorkflowCall(self, [expression(argument) for argument in arguments])

    def __repr__(self) -> str:
        return f"<lab workflow {self.name}>"


class WorkflowCall:
    """One workflow performed by another, which is a durable step like any other."""

    __slots__ = ("arguments", "workflow")

    def __init__(self, workflow: Workflow, arguments: Sequence[Expression]) -> None:
        self.workflow = workflow
        self.arguments = list(arguments)

    def render(self) -> str:
        words = [self.workflow.name, *(argument.operand() for argument in self.arguments)]
        return " ".join(words)

    def lab_modules(self) -> Iterator[str]:
        yield self.workflow.declaration.module.name
        for argument in self.arguments:
            yield from argument.lab_modules()

    @property
    def results(self) -> tuple[str, ...]:
        return self.workflow.results


class Context:
    """The `wf` a workflow body is written against.

    Its members are recognized where they are written rather than called, so
    this object exists to be referred to rather than to do anything. The one
    exception is `elapsed`, which is a value Lab reads off the context.
    """

    @property
    def elapsed(self) -> Expression:
        return Field(Reference(_CONTEXT), "elapsed")

    def perform(self, step: object) -> Any:  # pragma: no cover - read syntactically
        raise WorkflowError("wf.perform is only meaningful inside a @lab.workflow function")

    def state(self, annotation: object, initial: object) -> Any:  # pragma: no cover
        raise WorkflowError("wf.state is only meaningful inside a @lab.workflow function")

    def emit(self, event: object) -> None:  # pragma: no cover
        raise WorkflowError("wf.emit is only meaningful inside a @lab.workflow function")

    def every(self, period: object) -> Callable[[Callable[[], Any]], None]:  # pragma: no cover
        raise WorkflowError("wf.every is only meaningful inside a @lab.workflow function")

    def after(self, delay: object) -> Callable[[Callable[[], Any]], None]:  # pragma: no cover
        raise WorkflowError("wf.after is only meaningful inside a @lab.workflow function")


def workflow(fn: Callable[..., Any]) -> Workflow:
    """A workflow, read from the body of a Python function."""

    module, _ = declaring_module(depth=2)
    return Workflow(_translate(fn, module, caller_origin(2)))


def _translate(fn: Callable[..., Any], module: Module, origin: Origin) -> WorkflowDeclaration:
    tree = _parse(fn)
    parameters = _parameters(fn, tree)
    results = _results(fn, tree)
    body = _Body(fn, module, {name for name, _ in parameters})
    lines = body.block(tree.body, skip_doc=True)
    # A signature names types the body may never mention: a workflow that only
    # performs other workflows still returns their materials. The modules its
    # annotations come from import after the ones the body needed.
    for hint in _annotations(fn).values():
        body.uses.update(dict.fromkeys(type_modules(hint)))
    declaration = WorkflowDeclaration(
        module=module,
        name=fn.__name__,
        inputs=parameters,
        results=results,
        body=lines,
        doc=ast.get_docstring(tree),
        uses=tuple(body.uses),
        origin=origin,
    )
    module.declare(declaration)
    return declaration


def _parse(fn: Callable[..., Any]) -> ast.FunctionDef:
    """The function's own syntax, which is what a workflow is written in."""

    try:
        source = textwrap.dedent(inspect.getsource(fn))
    except (OSError, TypeError) as error:  # pragma: no cover - depends on how fn was made
        raise WorkflowError(
            f"the source of {fn.__name__} cannot be read, so its body cannot be "
            "translated; a workflow has to be written in a file rather than built "
            "at runtime"
        ) from error
    parsed = ast.parse(source).body[0]
    if not isinstance(parsed, ast.FunctionDef):
        raise WorkflowError(f"{fn.__name__} is not a plain function")
    return parsed


def _parameters(fn: Callable[..., Any], tree: ast.FunctionDef) -> list[tuple[str, str]]:
    """Every parameter but the context, with the Lab type each is annotated with."""

    arguments = tree.args.args
    if not arguments:
        raise WorkflowError(
            f"{fn.__name__} takes no parameters; a workflow's first parameter is the "
            "context it performs steps through, conventionally written `wf`"
        )
    hints = _annotations(fn)
    parameters = []
    for argument in arguments[1:]:
        if argument.arg not in hints:
            raise WorkflowError(
                f"parameter '{argument.arg}' of {fn.__name__} has no type; Lab states "
                "the type of every workflow parameter"
            )
        parameters.append((argument.arg, lab_type(hints[argument.arg])))
    return parameters


def _results(fn: Callable[..., Any], tree: ast.FunctionDef) -> list[tuple[str, str]]:
    """The workflow's results, named the way the body returns them.

    Lab names each result of a workflow that returns several, because a caller
    binds them by name. Python's tuple annotation has no names in it, so they
    are read from the `return` that produces them.
    """

    hints = _annotations(fn)
    if "return" not in hints:
        raise WorkflowError(
            f"{fn.__name__} has no return type; a workflow states what it produces, "
            "or `None` if it produces nothing"
        )
    types = result_types(hints["return"])
    if len(types) == 1:
        return [("outcome", types[0][1])]
    names = _returned_names(tree, len(types))
    return [(name, stated) for name, (_, stated) in zip(names, types, strict=True)]


def _returned_names(tree: ast.FunctionDef, count: int) -> list[str]:
    for node in ast.walk(tree):
        if isinstance(node, ast.Return) and isinstance(node.value, ast.Tuple):
            elements = node.value.elts
            if len(elements) == count and all(isinstance(e, ast.Name) for e in elements):
                return [element.id for element in elements]  # type: ignore[attr-defined]
    return [f"result_{index + 1}" for index in range(count)]


def _annotations(fn: Callable[..., Any]) -> dict[str, Any]:
    """The function's annotations, left exactly as they were written.

    They are not resolved through `typing.get_type_hints`, because a Lab type
    is a mirror value rather than a Python type and resolution would try to
    make a class of it.
    """

    return dict(getattr(fn, "__annotations__", {}))


class _Body:
    """Translates the statements of one workflow into lines of Lab."""

    def __init__(self, fn: Callable[..., Any], module: Module, bound: set[str]) -> None:
        self.fn = fn
        self.module = module
        self.globals = fn.__globals__
        self.context = _context_name(fn)
        self.bound = set(bound)
        self.state: set[str] = set()
        # Keyed by module path in the order the body first needs each one. The
        # emitted `use` lines read from this, and emitted source must be
        # byte-stable across runs, so iteration order cannot come from a set.
        self.uses: dict[str, None] = {}

    def block(self, statements: Sequence[ast.stmt], *, skip_doc: bool = False) -> list[str]:
        """The statements of one body, spaced the way Lab is written by hand.

        A block that stands apart from its neighbours reads as one thing, so
        the memory a workflow keeps, each reaction it registers, and each
        branch it takes are separated from the run of steps around them.
        """

        written = [
            statement
            for index, statement in enumerate(statements)
            if not (skip_doc and index == 0 and _is_docstring(statement))
        ]
        writer = SourceWriter()
        self.write(written, writer)
        return writer.finish()[0].splitlines()

    def write(self, statements: Sequence[ast.stmt], writer: SourceWriter) -> None:
        """Write a run of statements, spacing the blocks among them apart."""

        previous: ast.stmt | None = None
        for statement in statements:
            if _is_docstring(statement) and previous is None:
                continue
            if previous is not None and (
                _stands_apart(statement)
                or _stands_apart(previous)
                or (self._is_state(previous) and not self._is_state(statement))
            ):
                writer.line()
            self.statement(statement, writer)
            previous = statement

    def _is_state(self, node: ast.stmt) -> bool:
        """Whether a statement declares the memory a workflow keeps."""

        value = getattr(node, "value", None)
        return value is not None and _call_on(value, self.context) == "state"

    def statement(self, node: ast.stmt, writer: SourceWriter) -> None:
        if isinstance(node, ast.Pass):
            return
        if isinstance(node, ast.Assign):
            self.assign(node, writer)
        elif isinstance(node, ast.AnnAssign):
            self.annotated(node, writer)
        elif isinstance(node, ast.Expr):
            self.effect_statement(node, writer)
        elif isinstance(node, ast.Return):
            self.returns(node, writer)
        elif isinstance(node, ast.If):
            self.branch(node, writer)
        elif isinstance(node, ast.Match):
            self.match(node, writer)
        elif isinstance(node, ast.For):
            self.loop(node, writer)
        elif isinstance(node, ast.FunctionDef):
            self.handler(node, writer)
        else:
            raise WorkflowError(
                f"{type(node).__name__.lower()} has no Lab form in a workflow, at "
                f"{self._where(node)}"
            )

    def assign(self, node: ast.Assign, writer: SourceWriter) -> None:
        if len(node.targets) != 1:
            raise WorkflowError(f"chained assignment has no Lab form, at {self._where(node)}")
        target = node.targets[0]
        call = _call_on(node.value, self.context)
        if call == "state":
            self.declare_state(target, node.value, writer)
            return
        if call == "perform":
            self.perform(target, node.value, writer)
            return
        names = _targets(target, self._where(node))
        if len(names) != 1:
            raise WorkflowError(
                f"only a durable step binds several names at once, at {self._where(node)}"
            )
        rendered = self.expression(node.value)
        writer.line(f"{names[0]} = {rendered.render()}")
        self.bound.add(names[0])

    def annotated(self, node: ast.AnnAssign, writer: SourceWriter) -> None:
        if node.value is None:
            raise WorkflowError(f"a binding needs a value, at {self._where(node)}")
        if _call_on(node.value, self.context) == "state":
            self.declare_state(node.target, node.value, writer)
            return
        name = _targets(node.target, self._where(node))[0]
        stated = lab_type(self.evaluate(node.annotation))
        writer.line(f"{name}: {stated} = {self.expression(node.value).render()}")
        self.bound.add(name)

    def declare_state(self, target: ast.expr, value: ast.expr, writer: SourceWriter) -> None:
        name = _targets(target, self._where(value))[0]
        arguments = value.args  # type: ignore[attr-defined]
        if len(arguments) != 2:
            raise WorkflowError(
                "wf.state states the type it remembers and what it starts as, "
                f"as wf.state(list[Observation], []), at {self._where(value)}"
            )
        stated = lab_type(self.evaluate(arguments[0]))
        self.uses.update(dict.fromkeys(type_modules(self.evaluate(arguments[0]))))
        writer.line(f"state {name}: {stated} = {self.expression(arguments[1]).render()}")
        self.bound.add(name)
        self.state.add(name)

    def perform(self, target: ast.expr | None, value: ast.expr, writer: SourceWriter) -> None:
        arguments = value.args  # type: ignore[attr-defined]
        if len(arguments) != 1:
            raise WorkflowError(f"wf.perform performs one step, at {self._where(value)}")
        step = self.evaluate(arguments[0])
        if not isinstance(step, Effect | WorkflowCall):
            raise WorkflowError(
                f"wf.perform takes a durable action or another workflow, not "
                f"{type(step).__name__}, at {self._where(value)}"
            )
        self.uses.update(dict.fromkeys(step.lab_modules()))
        names = [] if target is None else _targets(target, self._where(value))
        expected = len(step.results)
        if names and len(names) != expected:
            raise WorkflowError(
                f"this step binds {expected} name(s), {', '.join(step.results) or 'none'}; "
                f"{len(names)} were given, at {self._where(value)}"
            )
        phrase = self.hoisted(step, writer)
        writer.line(f"{', '.join(names)} <- {phrase}" if names else f"<- {phrase}")
        self.bound.update(names)

    def hoisted(self, step: Effect | WorkflowCall, writer: SourceWriter) -> str:
        """The step's phrase, with every operand reduced to a single word.

        An action's operand is one word in Lab, so anything built in place,
        a list of materials most often, is bound above the step and named
        there. That is what the hand-written form does too.
        """

        if isinstance(step, WorkflowCall):
            arguments = [
                self._word(argument, f"argument_{index + 1}", writer)
                for index, argument in enumerate(step.arguments)
            ]
            return " ".join([step.workflow.name, *arguments])
        operands = {
            slot: self._word(operand, slot, writer) for slot, operand in step.operands.items()
        }
        skipped = step.action.skipped(set(step.operands))
        words = []
        for index, word in enumerate(step.action.phrase):
            if index in skipped:
                continue
            slot = word[1:-1] if word.startswith("<") else None
            words.append(operands[slot] if slot else word)
        return " ".join(words)

    def _word(self, operand: Expression, slot: str, writer: SourceWriter) -> str:
        """One operand, bound above the step first if it is not already a word."""

        if isinstance(operand, Reference | Field | Quantity | Integer | Decimal):
            return operand.render()
        rendered = operand.render()
        if rendered.replace(".", "").replace("_", "").isalnum():
            return rendered
        name = _naming.free_name(_naming.identifier(slot), "value", self.bound)
        writer.line(f"{name} = {rendered}")
        self.bound.add(name)
        return name

    def effect_statement(self, node: ast.Expr, writer: SourceWriter) -> None:
        call = _call_on(node.value, self.context)
        if call == "perform":
            self.perform(None, node.value, writer)
            return
        if call == "emit":
            arguments = node.value.args  # type: ignore[attr-defined]
            emitted = self.expression(arguments[0])
            writer.line(f"emit {emitted.render()}")
            return
        appended = self._appended(node.value)
        if appended is not None:
            name, value = appended
            writer.line(f"{name} = {name} + [{self.expression(value).render()}]")
            return
        raise WorkflowError(
            f"an expression on its own does nothing in Lab; a step goes through "
            f"wf.perform, at {self._where(node)}"
        )

    def _appended(self, node: ast.expr) -> tuple[str, ast.expr] | None:
        """`observations.append(x)` on state, which Lab rebinds rather than mutates."""

        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute):
            return None
        if node.func.attr != "append" or not isinstance(node.func.value, ast.Name):
            return None
        name = node.func.value.id
        if name not in self.state or len(node.args) != 1:
            return None
        return name, node.args[0]

    def returns(self, node: ast.Return, writer: SourceWriter) -> None:
        if node.value is None:
            writer.line("return None")
            return
        if isinstance(node.value, ast.Tuple):
            rendered = ", ".join(self.expression(element).render() for element in node.value.elts)
            writer.line(f"return {rendered}")
            return
        if _call_on(node.value, self.context) == "perform":
            raise WorkflowError(
                f"a step is performed on its own line and then returned, at {self._where(node)}"
            )
        writer.line(f"return {self.expression(node.value).render()}")

    def branch(self, node: ast.If, writer: SourceWriter) -> None:
        writer.line(f"if {self.expression(node.test).render()}:")
        with writer.indented():
            self.write(node.body, writer)
        if node.orelse:
            writer.line("else:")
            with writer.indented():
                self.write(node.orelse, writer)

    def match(self, node: ast.Match, writer: SourceWriter) -> None:
        writer.line(f"match {self.expression(node.subject).render()}:")
        with writer.indented():
            for index, arm in enumerate(node.cases):
                if index:
                    writer.line()
                writer.line(f"case {self._pattern(arm.pattern)}:")
                with writer.indented():
                    self.write(arm.body, writer)

    def _pattern(self, pattern: ast.pattern) -> str:
        """A case name, which Lab writes bare even where Python qualifies it."""

        if isinstance(pattern, ast.MatchClass) and not pattern.patterns:
            target = pattern.cls
            if isinstance(target, ast.Attribute):
                return target.attr
            if isinstance(target, ast.Name):
                return target.id
        if isinstance(pattern, ast.MatchAs) and pattern.pattern is None:
            return pattern.name or "_"
        raise WorkflowError("a case matches a name, as `case ColonyGrowth.Ready():`")

    def loop(self, node: ast.For, writer: SourceWriter) -> None:
        names = _targets(node.target, self._where(node))
        if len(names) != 1:
            raise WorkflowError(f"a loop binds one name, at {self._where(node)}")
        writer.line(f"for {names[0]} in {self.expression(node.iter).render()}:")
        self.bound.add(names[0])
        with writer.indented():
            self.write(node.body, writer)

    def handler(self, node: ast.FunctionDef, writer: SourceWriter) -> None:
        """A nested function decorated with a timer, which is a `when` block."""

        if len(node.decorator_list) != 1:
            raise WorkflowError(
                f"{node.name} is nested in a workflow, so it states when it runs, "
                "as @wf.every(30 * minutes) or @wf.after(18 * h)"
            )
        decorator = node.decorator_list[0]
        trigger = _call_on(decorator, self.context)
        if trigger not in ("every", "after") or not isinstance(decorator, ast.Call):
            raise WorkflowError(
                f"{node.name} is nested in a workflow, so it states when it runs, "
                "as @wf.every(30 * minutes) or @wf.after(18 * h)"
            )
        period = self.expression(decorator.args[0])
        writer.line(f"when {trigger} {period.render()}:")
        with writer.indented():
            self.write(node.body, writer)

    def expression(self, node: ast.expr) -> Expression:
        """One Python expression, as the Lab expression it evaluates to."""

        value = self.evaluate(node)
        try:
            rendered = expression(value)
        except TypeError as error:
            raise WorkflowError(f"{ast.unparse(node)} is not a Lab expression: {error}") from error
        self.uses.update(dict.fromkeys(rendered.lab_modules()))
        return rendered

    def evaluate(self, node: ast.expr) -> Any:
        """Evaluate a Python expression with the workflow's names in scope.

        Names the body bound stand for themselves, because Lab resolves them;
        everything else is the module's own, so a record constructor, a unit,
        or an imported design is the object it always was.
        """

        scope: dict[str, Any] = {name: Reference(name) for name in self.bound}
        scope[self.context] = Context()
        try:
            return eval(
                compile(ast.Expression(body=node), "<workflow>", "eval"),
                self.globals,
                scope,
            )
        except WorkflowError:
            raise
        except Exception as error:
            raise WorkflowError(
                f"{ast.unparse(node)} could not be read as a Lab expression: {error}"
            ) from error

    def _where(self, node: ast.AST) -> str:
        line = getattr(node, "lineno", 0)
        return f"{self.fn.__name__} line {line}"


def _context_name(fn: Callable[..., Any]) -> str:
    parameters = list(inspect.signature(fn).parameters)
    return parameters[0] if parameters else "wf"


def _call_on(node: ast.expr, context: str) -> str | None:
    """The context method a node calls, if it calls one at all."""

    if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute):
        return None
    target = node.func.value
    if isinstance(target, ast.Name) and target.id == context:
        return node.func.attr
    return None


def _targets(target: ast.expr, where: str) -> list[str]:
    if isinstance(target, ast.Name):
        return [target.id]
    if isinstance(target, ast.Tuple) and all(isinstance(e, ast.Name) for e in target.elts):
        return [element.id for element in target.elts]  # type: ignore[attr-defined]
    raise WorkflowError(f"a binding names values, at {where}")


def _is_docstring(node: ast.stmt) -> bool:
    return isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant)


def _stands_apart(node: ast.stmt) -> bool:
    """Whether a statement is a block rather than a step in a run of them."""

    return isinstance(node, ast.FunctionDef | ast.Match | ast.If | ast.For)
