"""Python SDK for the Lab frontend."""

import json
from typing import Any, cast

from ._native import compile_lab_module as _compile_lab_module


def compile_lab_module(source: str) -> dict[str, Any]:
    """Parse, resolve, and type-check a Lab source module."""

    return cast(dict[str, Any], json.loads(_compile_lab_module(source)))


__all__ = ["compile_lab_module"]
