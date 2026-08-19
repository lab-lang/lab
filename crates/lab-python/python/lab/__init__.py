"""Python SDK for the Lab frontend.

Two things live here. `compile_lab_module` checks Lab source text. The rest is
an object model for writing Lab programs in Python: a Python module holds a Lab
module, declarations are objects bound to Python names, and `check` emits the
Lab those objects describe and hands it to the compiler.

The standard library is mirrored as ordinary Python packages, so `plasmid` is
`lab.bio.designs.Plasmid` and importing it is what puts `use std.bio.designs`
in the emitted module. Those mirrors are generated from the compiler's own
catalog by `lab.codegen`.

Checking is never reimplemented here. The language's own frontend is the only
authority on whether a program is well formed, and a diagnostic it reports is
mapped back to the line of Python that produced the Lab it is about.
"""

import json
from typing import Any, cast

from ._circuits import (
    CircuitError,
    Network,
    NetworkBinding,
    UnitBinding,
    circuit,
    layout,
)
from ._declarations import (
    Binding,
    CircuitDeclaration,
    Claim,
    Declaration,
    Module,
    Predicate,
    Property,
    RecordDeclaration,
)
from ._expressions import (
    Expression,
    Fields,
    Quantity,
    Record,
    Unit,
    and_,
    expression,
    not_,
    or_,
)
from ._native import compile_lab_module as _compile_lab_module
from ._program import (
    Diagnostic,
    LabError,
    Program,
    analyze,
    analyze_sources,
    check,
    check_sources,
)
from ._sbol import DesignError
from ._source import Origin
from ._vocabulary import ArtifactKind, Function, Symbol


def compile_lab_module(source: str) -> dict[str, Any]:
    """Parse, resolve, and type-check a Lab source module."""

    return cast(dict[str, Any], json.loads(_compile_lab_module(source)))


__all__ = [
    "ArtifactKind",
    "Binding",
    "CircuitDeclaration",
    "CircuitError",
    "Claim",
    "Declaration",
    "DesignError",
    "Diagnostic",
    "Expression",
    "Fields",
    "Function",
    "LabError",
    "Module",
    "Network",
    "NetworkBinding",
    "Origin",
    "Predicate",
    "Program",
    "Property",
    "Quantity",
    "Record",
    "RecordDeclaration",
    "Symbol",
    "Unit",
    "UnitBinding",
    "analyze",
    "analyze_sources",
    "and_",
    "check",
    "check_sources",
    "circuit",
    "compile_lab_module",
    "expression",
    "layout",
    "not_",
    "or_",
]
