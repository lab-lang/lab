"""Inspect the bundled Python bindings rendered by the Lab Compiler.

The renderer is shared with ``lab bindings python``. Filesystem ownership and
stale-file cleanup belong to that command, so this module exposes only the
generated result used by the SDK's currentness test.

Regenerate after changing the standard library::

    lab bindings python std --out-dir crates/lab-python/python/lab
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from ._native import lab_python_standard_bindings


@dataclass(frozen=True)
class GeneratedModule:
    """One generated Python source or stub, relative to the ``lab`` package."""

    path: Path
    source: str


def generate() -> list[GeneratedModule]:
    """Render standard-library runtime modules and typing stubs."""

    rendered = cast(list[dict[str, str]], json.loads(lab_python_standard_bindings()))
    return [GeneratedModule(path=Path(file["path"]), source=file["source"]) for file in rendered]
