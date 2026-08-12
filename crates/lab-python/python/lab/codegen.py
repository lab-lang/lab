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
    modules = [module for module in described["modules"] if _is_mirrored(module)]
    generated = [_module(module) for module in modules]
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
    """Whether a standard module has anything the object model can use.

    Modules of durable actions are left out until workflows are written in
    Python; a mirror of them would be names with nothing to attach to.
    """

    return any(_binds(export) for export in module["exports"])


def _binds(export: dict[str, Any]) -> bool:
    """Whether an export becomes a Python name.

    `None` is a Lab value and a Python keyword. Lab's own spelling of it is
    what an author writes, so there is nothing useful to bind here.
    """

    return export["kind"] != "action" and not keyword.iskeyword(export["name"])


def _python_path(lab_path: str) -> tuple[str, ...]:
    segments = lab_path.split(".")
    if segments[0] == _ROOT:
        segments = segments[1:]
    return tuple(segments)


def _module(module: dict[str, Any]) -> GeneratedModule:
    exports = [export for export in module["exports"] if _binds(export)]
    # Importing one word can require importing what it extends, so a name
    # carries every module a declaration using it has to import. The prelude is
    # imported by every module without saying so, and implies no `use` line.
    uses = () if module["prelude"] else (*module["imports"], module["path"])

    blocks = [_documentation(module["documentation"]), _HEADER]
    imported = _runtime_imports(module["path"], exports)
    if imported:
        blocks.append(imported)
    blocks.append(f'LAB_MODULE = "{module["path"]}"\n"""The Lab module these names come from."""')
    blocks.extend(_export(export, uses) for export in exports)
    source = "\n\n".join(blocks).rstrip() + "\n"
    return GeneratedModule(Path(*_python_path(module["path"])).with_suffix(".py"), source)


def _runtime_imports(path: str, exports: list[dict[str, Any]]) -> str:
    kinds = {export["kind"] for export in exports}
    names = set()
    if "artifact_kind" in kinds:
        names.add("ArtifactKind")
    if "function" in kinds:
        names.add("Function")
    if kinds & {"value", "constructor", "type", "role"}:
        names.add("Symbol")
    if not names:
        return ""
    package = "." * len(_python_path(path))
    return f"from {package}_vocabulary import {', '.join(sorted(names))}"


#: The line limit the generated mirror is written to, matching the project's.
_LIMIT = 100


def _export(export: dict[str, Any], uses: tuple[str, ...]) -> str:
    if export["kind"] == "artifact_kind":
        return _artifact_kind(export, uses)
    factory = "Function" if export["kind"] == "function" else "Symbol"
    name = export["name"]
    assignment = f'{name} = {factory}(name="{name}", uses={_tuple(uses)})'
    if len(assignment) > _LIMIT:
        assignment = "\n".join(
            [f"{name} = {factory}(", f'    name="{name}",', f"    uses={_tuple(uses)},", ")"]
        )
    return _documented(assignment, export)


def _artifact_kind(export: dict[str, Any], uses: tuple[str, ...]) -> str:
    # The Python name is the type instances have, because that is the name a
    # reader knows the thing by. The word declarations are written with travels
    # with it rather than becoming the Python name.
    assignment = "\n".join(
        [
            f"{export['produces']} = ArtifactKind(",
            f'    word="{export["name"]}",',
            f'    produces="{export["produces"]}",',
            f"    uses={_tuple(uses)},",
            *_properties(tuple(field["name"] for field in export["fields"])),
            ")",
        ]
    )
    return _documented(assignment, export)


def _properties(properties: tuple[str, ...]) -> list[str]:
    if not properties:
        return ["    properties=(),"]
    return ["    properties=(", *(f'        "{name}",' for name in properties), "    ),"]


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
