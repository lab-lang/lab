"""Compiling emitted Lab, and reporting its diagnostics against Python.

The compiler checks Lab source, so a diagnostic arrives with a span into the
source this SDK emitted. The source map turns that span back into the line of
Python that wrote it, and the reported error carries both: the Python a reader
can edit, and the compiler's own excerpt of the Lab it produced.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any, cast

from ._declarations import Module
from ._native import analyze_lab_modules as _analyze_lab_modules
from ._source import Origin, SourceMap


@dataclass(frozen=True)
class Diagnostic:
    """One compiler diagnostic, placed in Lab and in Python."""

    module: str
    severity: str
    code: str
    message: str
    help: tuple[str, ...]
    rendered: str
    origin: Origin | None

    def report(self) -> str:
        located = (
            ""
            if self.origin is None
            else f'  File "{self.origin.file}", line {self.origin.line}\n\n'
        )
        return f"{located}{self.rendered}"


class LabError(Exception):
    """Raised when checking a Lab program reports errors."""

    def __init__(self, diagnostics: list[Diagnostic]) -> None:
        self.diagnostics = diagnostics
        report = "\n\n".join(diagnostic.report() for diagnostic in diagnostics)
        super().__init__(f"Lab reported {len(diagnostics)} error(s)\n\n{report}")


@dataclass(frozen=True)
class Program:
    """A checked Lab program, and the source each module was checked from."""

    sources: dict[str, str]
    checked: dict[str, dict[str, Any]]
    diagnostics: tuple[Diagnostic, ...]

    def declarations(self, module: str) -> list[dict[str, Any]]:
        return cast(list[dict[str, Any]], self.checked[module]["declarations"])


def analyze(*modules: Module) -> Program:
    """Emit and check modules, returning diagnostics rather than raising.

    Modules are checked in the order given, each against the interfaces of the
    ones before it, so a module is written after whatever it imports.
    """

    sources: dict[str, str] = {}
    maps: dict[str, SourceMap] = {}
    for module in modules:
        source, source_map = module.emit()
        sources[module.name] = source
        maps[module.name] = source_map
    return _analyze(sources, maps)


def analyze_sources(sources: dict[str, str]) -> Program:
    """Check Lab source text that was written rather than emitted."""

    return _analyze(sources, {})


def _analyze(sources: dict[str, str], maps: dict[str, SourceMap]) -> Program:
    analyzed = cast(
        list[dict[str, Any]],
        json.loads(_analyze_lab_modules([(name, source) for name, source in sources.items()])),
    )

    checked: dict[str, dict[str, Any]] = {}
    diagnostics: list[Diagnostic] = []
    for result in analyzed:
        name = cast(str, result["module"])
        if result["checked"] is not None:
            checked[name] = cast(dict[str, Any], result["checked"])
        for raw in cast(list[dict[str, Any]], result["diagnostics"]):
            start = cast(int, raw["span"]["start"])
            diagnostics.append(
                Diagnostic(
                    module=name,
                    severity=cast(str, raw["severity"]),
                    code=cast(str, raw["code"]),
                    message=cast(str, raw["message"]),
                    help=tuple(cast(list[str], raw.get("help", []))),
                    rendered=cast(str, raw["rendered"]),
                    origin=maps[name].locate(start) if name in maps else None,
                )
            )
    return Program(sources=sources, checked=checked, diagnostics=tuple(diagnostics))


def check(*modules: Module) -> Program:
    """Emit and check modules, raising `LabError` if any of them is rejected."""

    return _raising(analyze(*modules))


def check_sources(sources: dict[str, str]) -> Program:
    """Check written Lab source, raising `LabError` if any of it is rejected."""

    return _raising(analyze_sources(sources))


def _raising(program: Program) -> Program:
    errors = [diagnostic for diagnostic in program.diagnostics if diagnostic.severity == "error"]
    if errors:
        raise LabError(errors)
    return program
