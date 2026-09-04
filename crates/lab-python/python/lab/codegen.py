"""Generate the Python mirror of the Lab standard library.

`lab.bio.designs` holds the same words `std.bio.designs` does, as real Python
names an editor completes and a typechecker resolves. The modules are generated
from the compiler's own catalog rather than written by hand, so the mirror
cannot drift from what a Lab program sees.

Regenerate after changing the standard library:

    python -m lab.codegen

The result is checked in, and `tests/test_codegen.py` fails if it is stale.
"""

from __future__ import annotations

import json
import keyword
import sys
import textwrap
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from ._native import lab_standard_library

#: The Lab package the mirror is rooted at. `std.bio.designs` becomes
#: `lab.bio.designs`, because the Python package name already says `lab`.
_ROOT = "std"

#: The Python package the mirror is written into, which a Lab path repeats:
#: `std.lab.plasmid` is `lab.plasmid` rather than `lab.lab.plasmid`.
_MIRROR_ROOT = "lab"

#: The module Lab imports into every other one, whose Python home is the
#: package namespace rather than a module anybody writes an import for.
_PRELUDE = "prelude"

_HEADER = "# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit."

#: Documentation is wrapped well inside the line limit, because it is prose
#: rather than code and the limit is not what makes it readable.
_WIDTH = 88


@dataclass(frozen=True)
class GeneratedModule:
    """One Python module of the mirror, and where it belongs."""

    path: Path
    source: str


def standard_library() -> dict[str, Any]:
    """The compiler's description of every bundled standard module."""

    return cast(dict[str, Any], json.loads(lab_standard_library()))


def generate(library: dict[str, Any] | None = None) -> list[GeneratedModule]:
    """Render the mirror, one Python module per Lab module."""

    described = library if library is not None else standard_library()
    grounding = {
        module["path"]
        for module in described["modules"]
        if module["exports"] and all(export["kind"] == "role" for export in module["exports"])
    }
    modules = [module for module in described["modules"] if _is_mirrored(module)]
    generated = [_module(module, grounding) for module in modules]
    return generated + _packages(generated)


def write(root: Path, generated: list[GeneratedModule]) -> list[Path]:
    """Write the mirror under `root`, returning the files that changed."""

    changed = []
    for module in generated:
        target = root / module.path
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists() or target.read_text() != module.source:
            target.write_text(module.source)
            changed.append(target)
    return changed


def _is_mirrored(module: dict[str, Any]) -> bool:
    """Whether a standard module has anything the object model can use."""

    return any(_binds(export) for export in module["exports"])


def _binds(export: dict[str, Any]) -> bool:
    """Whether an export becomes a Python name.

    `None` is a Lab value and a Python keyword. Lab's own spelling of it is
    what an author writes, so there is nothing useful to bind here.
    """

    return not keyword.iskeyword(export["name"])


def _python_path(lab_path: str) -> tuple[str, ...]:
    """Where a Lab module's mirror lives, relative to the `lab` package.

    `std` drops because the package it becomes is the standard library, and a
    following `lab` drops because the package is already called that:
    `std.lab.plasmid` is `lab.plasmid` rather than `lab.lab.plasmid`.

    The prelude has no module of its own. Lab imports it into every module
    without being asked, and the Python namespace that is always reachable is
    the package itself, so its names are generated into `_prelude` and the
    package re-exports them: `from lab import Material`.
    """

    segments = lab_path.split(".")
    if segments[0] == _ROOT:
        segments = segments[1:]
    if segments == [_PRELUDE]:
        return (f"_{_PRELUDE}",)
    if len(segments) > 1 and segments[0] == _MIRROR_ROOT:
        segments = segments[1:]
    return tuple(segments)


def _module(module: dict[str, Any], grounding: set[str]) -> GeneratedModule:
    exports = [export for export in module["exports"] if _binds(export)]
    # A name written both as a type and as a constructor is generated once, as
    # the type: the class annotates and calling it builds the record.
    constructors = {export["name"] for export in exports if export["kind"] == "constructor"}
    types = {export["name"] for export in exports if export["kind"] == "type"}
    exports = [
        export
        for export in exports
        if not (export["kind"] == "constructor" and export["name"] in types)
    ]
    # Importing one word can require importing what it extends, so a name
    # carries every module a declaration using it has to import. The prelude is
    # imported by every module without saying so, and implies no `use` line.
    # A module of nothing but roles grounds kinds in ontology terms; no
    # declaration ever names a term, so it is never imported either.
    uses = (
        ()
        if module["prelude"]
        else (
            *(path for path in module["imports"] if path not in grounding),
            module["path"],
        )
    )

    blocks = [_documentation(module["documentation"]), _HEADER]
    imported = _runtime_imports(module["path"], exports, frozenset(constructors))
    if imported:
        blocks.append(imported)
    variables = _type_variables(exports)
    if variables:
        blocks.append(variables)
    blocks.append(f'LAB_MODULE = "{module["path"]}"\n"""The Lab module these names come from."""')
    if module["prelude"]:
        blocks.append(_exported_names(exports))
    blocks.extend(_export(export, uses, frozenset(constructors)) for export in exports)
    return GeneratedModule(Path(*_python_path(module["path"])).with_suffix(".py"), _joined(blocks))


def _exported_names(exports: list[dict[str, Any]]) -> str:
    """`__all__` for the prelude, which the package re-exports wholesale."""

    names = sorted(
        (
            export["produces"] if export["kind"] == "artifact_kind" else export["name"]
            for export in exports
        ),
        key=_export_order,
    )
    rendered = "\n".join(f'    "{name}",' for name in names)
    return f"__all__ = [\n{rendered}\n]"


def _export_order(name: str) -> tuple[int, str]:
    """The order a formatter expects `__all__` in.

    Names that are all capitals come first, then the ones that begin with a
    capital, then the rest, which is how the linter reads a sorted list.
    """

    if name.isupper():
        return (0, name)
    return (1, name) if name[:1].isupper() else (2, name)


def _joined(blocks: list[str]) -> str:
    """The module's blocks, spaced the way the formatter would space them.

    A class stands two blank lines from its neighbours and a binding one, so
    the generated mirror is already formatted and regenerating it never shows
    up as a diff. A facet is one block holding both, so what it ends with
    decides the space after it.
    """

    def holds_class(block: str) -> bool:
        return block.startswith("class ") or "\nclass " in block

    pieces = [blocks[0]]
    for index, block in enumerate(blocks[1:]):
        apart = holds_class(block) or holds_class(blocks[index])
        pieces.append("\n\n\n" if apart else "\n\n")
        pieces.append(block)
    return "".join(pieces).rstrip() + "\n"


def _runtime_imports(path: str, exports: list[dict[str, Any]], constructors: frozenset[str]) -> str:
    kinds = {export["kind"] for export in exports}
    names: set[str] = set()
    types: set[str] = set()
    if "artifact_kind" in kinds:
        names.add("ArtifactKind")
        types.add("LabType")
    if "function" in kinds:
        names.add("Function")
    # A facet generates its own name and one per state it admits, and every
    # one of them is a bare word Lab reads back.
    if kinds & {"value", "constructor", "facet"}:
        names.add("Symbol")
    if "facet" in kinds:
        types.add("LabState")
    if "type" in kinds:
        types.add("LabType")
    if any(
        export["kind"] == "type" and (export["name"] in constructors or export.get("fields"))
        for export in exports
    ):
        types.add("LabConstructor")
    if "role" in kinds:
        types.add("LabRole")
    package = "." * len(_python_path(path))
    standard = ["from typing import Generic, TypeVar"] if _parameters(exports) else []
    local = []
    if "action" in kinds:
        local.append(f"from {package}_effects import Action")
    if types:
        local.append(f"from {package}_types import {', '.join(sorted(types))}")
    if names:
        local.append(f"from {package}_vocabulary import {', '.join(sorted(names))}")
    blocks = [block for block in ("\n".join(standard), "\n".join(local)) if block]
    return "\n\n".join(blocks)


def _indented(block: str) -> list[str]:
    """A block of documentation, indented into a class body.

    A blank line keeps no indentation, because trailing spaces are what a
    formatter would strip and the mirror is written already formatted.
    """

    return [f"    {line}" if line else "" for line in block.splitlines()]


def _parameters(exports: list[dict[str, Any]]) -> int:
    """The most type parameters any one type in a module takes."""

    # A facet's states are generic in the one type each narrows.
    return max(
        (
            1 if export["kind"] == "facet" else (export.get("parameters") or 0)
            for export in exports
            if export["kind"] in ("type", "facet")
        ),
        default=0,
    )


def _type_variables(exports: list[dict[str, Any]]) -> str:
    """One variable per type parameter, shared by every generic in the module."""

    count = _parameters(exports)
    return "\n".join(f'_T{index + 1} = TypeVar("_T{index + 1}")' for index in range(count))


#: The line limit the generated mirror is written to, matching the project's.
_LIMIT = 100


def _export(
    export: dict[str, Any], uses: tuple[str, ...], constructors: frozenset[str] = frozenset()
) -> str:
    if export["kind"] == "artifact_kind":
        return _artifact_kind(export, uses)
    if export["kind"] == "action":
        return _action(export, uses)
    if export["kind"] in ("type", "role"):
        return _lab_type(export, uses, constructors)
    if export["kind"] == "facet":
        return _facet(export, uses)
    factory = "Function" if export["kind"] == "function" else "Symbol"
    name = export["name"]
    assignment = f'{name} = {factory}(name="{name}", uses={_tuple(uses)})'
    if len(assignment) > _LIMIT:
        assignment = "\n".join(
            [f"{name} = {factory}(", f'    name="{name}",', f"    uses={_tuple(uses)},", ")"]
        )
    return _documented(assignment, export)


def _facet(export: dict[str, Any], uses: tuple[str, ...]) -> str:
    """A facet, generated as its name and one name per state it admits.

    The name is what a declaration states and each state is what it states as
    the value, so `competence = competent` needs both to be importable. A state
    is a bare word in Lab, which is what a `Symbol` renders as.
    """

    blocks = [_documented(_symbol(export["name"], uses), export)]
    blocks.extend(_state(state, uses) for state in export.get("states") or ())
    return "\n\n\n".join(blocks)


def _state(name: str, uses: tuple[str, ...]) -> str:
    """One state, generated as a generic class so an annotation may name it.

    `inoculated[Medium]` reads to a type checker the way `Medium is inoculated`
    reads to the compiler, and a bare `inoculated` still states the state where
    a declaration puts itself in one.
    """

    return "\n".join(
        [
            f"class {name}(LabState, Generic[_T1]):",
            f'    __lab_state__ = "{name}"',
            f"    __lab_uses__ = {_tuple(uses)}",
        ]
    )


def _symbol(name: str, uses: tuple[str, ...]) -> str:
    assignment = f'{name} = Symbol(name="{name}", uses={_tuple(uses)})'
    if len(assignment) > _LIMIT:
        assignment = "\n".join(
            [f"{name} = Symbol(", f'    name="{name}",', f"    uses={_tuple(uses)},", ")"]
        )
    return assignment


def _lab_type(
    export: dict[str, Any], uses: tuple[str, ...], constructors: frozenset[str] = frozenset()
) -> str:
    """A type or role, generated as a class so annotations typecheck.

    A parameterized type is generic in as many parameters as Lab gives it, so
    `Material[Plate]` reads to a typechecker exactly as it reads to the
    compiler.
    """

    name = export["name"]
    parameters = export.get("parameters") or 0
    if export["kind"] == "role":
        base = "LabRole"
    # A record with fields is a thing you build, and Lab writes building one the
    # same way it writes naming one. The mirror is a single class for the same
    # reason: it annotates like a type and calling it builds the record.
    elif export["name"] in constructors or export.get("fields"):
        base = "LabConstructor"
    else:
        base = "LabType"
    generic = f", Generic[{', '.join(f'_T{index + 1}' for index in range(parameters))}]"
    header = f"class {name}({base}{generic if parameters else ''}):"
    body = [f"    __lab_uses__ = {_tuple(uses)}"]
    if export["kind"] == "role":
        body.insert(0, f'    __lab_role__ = "{name}"')
    documentation = _member_documentation(export)
    lines = [header]
    if documentation:
        lines.extend(_indented(documentation))
        lines.append("")
    lines.extend(body)
    return "\n".join(lines)


def _action(export: dict[str, Any], uses: tuple[str, ...]) -> str:
    """A durable effect, carrying the phrase the standard library writes it as."""

    name = export["name"]
    lines = [
        f"{name} = Action(",
        f'    name="{name}",',
        f"    phrase={_tuple(tuple(export['phrase']))},",
        f"    results={_tuple(tuple(result['name'] for result in export['results']))},",
    ]
    if export.get("optional"):
        rendered = ", ".join(_tuple(tuple(clause)) for clause in export["optional"])
        lines.append(f"    optional=({rendered},),")
    lines.append(f"    uses={_tuple(uses)},")
    lines.append(")")
    return _documented("\n".join(lines), export)


def _artifact_kind(export: dict[str, Any], uses: tuple[str, ...]) -> str:
    # The Python name is the type instances have, because that is the name a
    # reader knows the thing by. The word declarations are written with travels
    # with it rather than becoming the Python name.
    name = export["produces"]
    lines = [f"class {name}(ArtifactKind, LabType):"]
    documentation = _member_documentation(export)
    if documentation:
        lines.extend(_indented(documentation))
        lines.append("")
    lines.append(f'    word = "{export["name"]}"')
    lines.append(f"    uses = {_tuple(uses)}")
    lines.append(f"    __lab_uses__ = {_tuple(uses)}")
    lines.extend(_properties(tuple(field["name"] for field in export["fields"])))
    return "\n".join(lines)


def _properties(properties: tuple[str, ...]) -> list[str]:
    """The property names a kind contributes, wrapped inside the line limit."""

    line = f"    properties = {_tuple(properties)}"
    if len(line) <= _LIMIT:
        return [line]
    return ["    properties = (", *(f'        "{name}",' for name in properties), "    )"]


def _tuple(values: tuple[str, ...]) -> str:
    if not values:
        return "()"
    rendered = ", ".join(f'"{value}"' for value in values)
    return f"({rendered},)" if len(values) == 1 else f"({rendered})"


def _documented(assignment: str, export: dict[str, Any]) -> str:
    documentation = _member_documentation(export)
    return f"{assignment}\n{documentation}" if documentation else assignment


def _member_documentation(export: dict[str, Any]) -> str:
    paragraphs = [export["documentation"].strip(), *_detail(export)]
    body = "\n\n".join(paragraph for paragraph in paragraphs if paragraph)
    if not body:
        return ""
    if "\n" not in body and len(body) + 6 <= _WIDTH:
        return f'"""{body}"""'
    return f'"""{body}\n"""'


def _detail(export: dict[str, Any]) -> list[str]:
    """What a reader needs that the module's own prose does not already say."""

    kind = export["kind"]
    if kind == "artifact_kind":
        stated = ", ".join(
            f"{field['name']}{'?' if field.get('optional') else ''}: {field['type']}"
            for field in export["fields"]
        )
        detail = [_wrap(f"Properties: {stated}.")] if stated else []
        if export["declares"]:
            detail.append(_wrap(f"Complete when it states {export['declares']}."))
        return detail
    if kind == "action":
        phrase = " ".join(export["phrase"])
        results = ", ".join(result["name"] for result in export["results"])
        detail = [_wrap(f"Performed as `{phrase}`.")]
        if results:
            detail.append(_wrap(f"Binds {results}."))
        return detail
    if kind == "function":
        return [_wrap(f"Called as ({', '.join(export['parameters'])}) -> {export['result']}.")]
    if kind == "value" and export["type"]:
        return [_wrap(f"A value of type {export['type']}.")]
    return []


def _wrap(text: str) -> str:
    return textwrap.fill(text, width=_WIDTH)


def _packages(generated: list[GeneratedModule]) -> list[GeneratedModule]:
    """The `__init__.py` each generated subpackage needs."""

    packages = sorted(
        {module.path.parent for module in generated if module.path.parent != Path(".")}
    )
    return [
        GeneratedModule(
            package / "__init__.py",
            f'"""The Python mirror of Lab\'s `{_ROOT}.{".".join(package.parts)}` package."""\n\n'
            f"{_HEADER}\n",
        )
        for package in packages
    ]


def _documentation(text: str) -> str:
    cleaned = text.strip()
    if not cleaned:
        return '"""Part of the Python mirror of the Lab standard library."""'
    if "\n" not in cleaned:
        return f'"""{cleaned}"""'
    return f'"""{cleaned}\n"""'


def main() -> int:
    """Regenerate the mirror in place, reporting what changed."""

    root = Path(__file__).resolve().parent
    changed = write(root, generate())
    for path in changed:
        print(f"wrote {path.relative_to(root.parent)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
