"""Lab source emission, and the map back to the Python that produced it.

Every range this writer emits remembers the Python statement responsible for
it, so a compiler diagnostic about a span of generated Lab is reported against
the line of Python a reader actually wrote.
"""

from __future__ import annotations

import sys
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from inspect import cleandoc
from types import FrameType

INDENT = "  "


@dataclass(frozen=True)
class Origin:
    """A place in Python source."""

    file: str
    line: int
    function: str

    def __str__(self) -> str:
        return f"{self.file}:{self.line}"


def caller_origin(depth: int = 1) -> Origin:
    """Where the frame `depth` levels above this call is executing.

    `depth` counts from this function's caller, so a helper reporting on the
    code that called it passes 2. A declaration captures its origin as it is
    built, which is the only moment the Python statement responsible for it is
    on the stack.
    """

    frame: FrameType | None = sys._getframe(depth)
    if frame is None:  # pragma: no cover - CPython always supplies a frame
        return Origin("<unknown>", 0, "<unknown>")
    return Origin(frame.f_code.co_filename, frame.f_lineno, frame.f_code.co_name)


@dataclass(frozen=True)
class Region:
    """A byte range of emitted Lab, and the Python that emitted it."""

    start: int
    end: int
    origin: Origin


class SourceMap:
    """Emitted ranges, searched innermost first."""

    def __init__(self, regions: list[Region]) -> None:
        self._regions = regions

    def locate(self, offset: int) -> Origin | None:
        """The Python responsible for the byte at `offset`.

        Regions may nest, so the narrowest containing range wins: the smallest
        piece of Python that could have produced the byte is the one worth
        reporting.
        """

        containing = [
            region
            for region in self._regions
            if region.start <= offset < max(region.end, region.start + 1)
        ]
        if not containing:
            return None
        return min(containing, key=lambda region: region.end - region.start).origin


class SourceWriter:
    """An indentation-aware writer that records what produced each range."""

    def __init__(self) -> None:
        self._parts: list[str] = []
        self._length = 0
        self._depth = 0
        self._regions: list[Region] = []

    @property
    def offset(self) -> int:
        return self._length

    def line(self, text: str = "") -> None:
        rendered = f"{INDENT * self._depth}{text}\n" if text else "\n"
        self._parts.append(rendered)
        self._length += len(rendered.encode())

    def documentation(self, doc: str | None, opener: str) -> None:
        """Emit a documentation block, `/**` for a declaration or `/*!` for a module.

        Documentation arrives as a Python docstring, so it is cleaned the way
        Python cleans one: the first line keeps its position and the rest lose
        the indentation they took from the source they were written in.
        """

        if not doc:
            return
        lines = [line.rstrip() for line in cleandoc(doc).splitlines()]
        self.line(opener)
        for line in lines:
            self.line(f" * {line}".rstrip())
        self.line(" */")

    @contextmanager
    def indented(self) -> Iterator[None]:
        self._depth += 1
        try:
            yield
        finally:
            self._depth -= 1

    @contextmanager
    def region(self, origin: Origin | None) -> Iterator[None]:
        """Attribute everything written inside to one place in Python."""

        start = self._length
        try:
            yield
        finally:
            if origin is not None:
                self._regions.append(Region(start, self._length, origin))

    def finish(self) -> tuple[str, SourceMap]:
        return "".join(self._parts), SourceMap(self._regions)
