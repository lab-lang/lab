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

from . import adapters as adapters
from . import methods as methods
from . import planning as planning
from . import procedures as procedures
from . import sbol as sbol
from ._artifacts import artifact
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
    BuildDeclaration,
    BuyDeclaration,
    Case,
    CircuitDeclaration,
    Claim,
    Declaration,
    Module,
    Predicate,
    Property,
    RecordDeclaration,
    WorkflowDeclaration,
)
from ._effects import Action, Effect
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
from ._prelude import *
from ._program import (
    Diagnostic,
    LabError,
    Program,
    analyze,
    analyze_sources,
    check,
    check_sources,
)
from ._records import CaseType, RecordType, case, record
from ._sbol import DesignError
from ._source import Origin
from ._types import TypeApplication
from ._vocabulary import ArtifactKind, Function, Symbol
from ._workflows import Context, Workflow, WorkflowCall, WorkflowError, workflow
from .adapters import catalog as adapter_catalog
from .adapters import validate_profile as validate_adapter_profile

# The durable effects of the standard library, which a workflow performs
# through `wf.perform`. They are the generated mirror's own objects, so
# reaching them here and reaching them through their module are the same
# thing, and either spelling emits the same `use` line.
from .bio.build import realize
from .methods import RefinedProgram, refine
from .planning import FacilityPlan, plan, plan_project
from .plasmid import (
    assemble,
    capture,
    culture,
    dilute,
    dispose,
    pick,
    plate,
    provision,
    purify,
    quantify,
    recover,
    screen,
    sequence,
    split,
    store,
    synthesize,
    transform,
)


def compile_lab_module(source: str) -> dict[str, Any]:
    """Parse, resolve, and type-check a Lab source module."""

    return cast(dict[str, Any], json.loads(_compile_lab_module(source)))


__all__ = [
    "CDS",
    "DNA",
    "Accepted",
    "Action",
    "Antibiotic",
    "ArtifactKind",
    "Backbone",
    "Binding",
    "BuildDeclaration",
    "BuyDeclaration",
    "Case",
    "CaseType",
    "Chassis",
    "Circuit",
    "CircuitDeclaration",
    "CircuitError",
    "Claim",
    "Clone",
    "CloneSet",
    "Colonies",
    "ColonyMap",
    "Context",
    "Culture",
    "Declaration",
    "DesignError",
    "Diagnostic",
    "Duration",
    "Effect",
    "Event",
    "Evidence",
    "Evidential",
    "Expression",
    "FacilityPlan",
    "Fields",
    "Fragment",
    "Function",
    "Image",
    "LabError",
    "List",
    "Material",
    "Module",
    "Network",
    "NetworkBinding",
    "Origin",
    "Part",
    "Plasmid",
    "Plate",
    "Predicate",
    "Program",
    "Promoter",
    "Property",
    "Protein",
    "Quantity",
    "Reason",
    "Record",
    "RecordDeclaration",
    "RecordType",
    "RefinedProgram",
    "Regulation",
    "Rejected",
    "RestrictionEnzyme",
    "Screening",
    "Signal",
    "Strain",
    "Symbol",
    "Topology",
    "TypeApplication",
    "Unit",
    "UnitBinding",
    "Workflow",
    "WorkflowCall",
    "WorkflowContext",
    "WorkflowDeclaration",
    "WorkflowError",
    "acceptance_failed",
    "accepts",
    "adapter_catalog",
    "adapters",
    "analyze",
    "analyze_sources",
    "and_",
    "artifact",
    "assemble",
    "capture",
    "case",
    "check",
    "check_sources",
    "circuit",
    "circular",
    "compile_lab_module",
    "culture",
    "detect_colonies",
    "dilute",
    "dispose",
    "dna",
    "expression",
    "inconclusive_sequence",
    "induced",
    "layout",
    "methods",
    "no_colonies",
    "not_",
    "or_",
    "pick",
    "plan",
    "plan_project",
    "planning",
    "plate",
    "procedures",
    "provision",
    "purify",
    "quantify",
    "realize",
    "record",
    "recover",
    "refine",
    "repressed",
    "sbol",
    "screen",
    "sequence",
    "sequence_mismatch",
    "sites",
    "split",
    "store",
    "synthesize",
    "transform",
    "validate_adapter_profile",
    "workflow",
]
