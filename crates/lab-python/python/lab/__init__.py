"""Python SDK for the Lab Compiler."""

from dataclasses import dataclass
import json
from typing import Any

from ._native import compile_lab_lang as _compile_lab_lang


@dataclass(frozen=True)
class Compilation:
    """Verified compiler output exposed in Python-native forms."""

    ir: str
    plan: dict[str, Any]


def compile_lab_lang(source: str) -> Compilation:
    """Compile Lab Lang using the reference laboratory profile."""

    ir, plan_json = _compile_lab_lang(source)
    return Compilation(ir=ir, plan=json.loads(plan_json))


__all__ = ["Compilation", "compile_lab_lang"]
