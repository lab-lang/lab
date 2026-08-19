"""Durable effects: the actions a workflow performs on the world.

An action is written in Lab as a phrase rather than a call, because the
operands read better with the prepositions between them:

    strain, culture <- transform reporter_host from dependencies into cells

The standard library states that phrase, so the mirror does not invent one. A
generated action carries the words and the operand slots it was declared
with, and calling it in Python fills the slots:

    transform(reporter_host, plasmids=dependencies, cells=cells)

The first operand is positional because a phrase always begins with one; the
rest are keywords named after the slots, which is what the phrase calls them.
An action's results are named too, and how many there are is what decides
whether a workflow binds one name, several, or none.

Some clauses are optional. Leaving one out drops its words as well as its
operand, which is what makes `realize reporter` and `realize reporter from
dependencies` two spellings of the same action.
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence

from ._expressions import Expression, expression


class Effect:
    """One action applied to its operands, ready to be written as a phrase."""

    __slots__ = ("action", "operands")

    def __init__(self, action: Action, operands: dict[str, Expression]) -> None:
        self.action = action
        self.operands = operands

    def render(self) -> str:
        """The Lab phrase, with each slot filled by its operand."""

        skipped = self.action.skipped(set(self.operands))
        words = []
        for index, word in enumerate(self.action.phrase):
            if index in skipped:
                continue
            slot = _slot(word)
            words.append(self.operands[slot].operand() if slot else word)
        return " ".join(words)

    def lab_modules(self) -> Iterator[str]:
        yield from self.action.uses
        for operand in self.operands.values():
            yield from operand.lab_modules()

    @property
    def results(self) -> tuple[str, ...]:
        return self.action.results

    def __repr__(self) -> str:
        return f"<lab effect: {self.render()}>"


class Action:
    """A durable effect a workflow may perform.

    Not an expression. An action happens, which is why Lab writes it behind
    `<-` and why nothing here can be used where a value belongs.
    """

    __slots__ = ("clauses", "name", "phrase", "required", "results", "slots", "uses")

    def __init__(
        self,
        *,
        name: str,
        phrase: Sequence[str],
        results: Sequence[str] = (),
        optional: Sequence[Sequence[str]] = (),
        uses: Sequence[str] = (),
    ) -> None:
        self.name = name
        self.phrase = tuple(phrase)
        self.results = tuple(results)
        self.uses = tuple(uses)
        self.slots = tuple(slot for slot in map(_slot, self.phrase) if slot)
        #: Each optional clause, as the phrase positions it occupies and the
        #: operands it carries.
        self.clauses = tuple(self._locate(tuple(clause)) for clause in optional)
        optional_slots = {slot for _, slots in self.clauses for slot in slots}
        self.required = tuple(slot for slot in self.slots if slot not in optional_slots)

    def _locate(self, clause: tuple[str, ...]) -> tuple[tuple[int, ...], tuple[str, ...]]:
        """Where a clause sits in the phrase, and which operands it carries."""

        for start in range(len(self.phrase) - len(clause) + 1):
            if self.phrase[start : start + len(clause)] == clause:
                positions = tuple(range(start, start + len(clause)))
                slots = tuple(slot for slot in map(_slot, clause) if slot)
                return positions, slots
        raise ValueError(f"{self.name} has no clause {' '.join(clause)}")

    def skipped(self, supplied: set[str]) -> set[int]:
        """The phrase positions an unsupplied optional clause takes with it."""

        return {
            position
            for positions, slots in self.clauses
            if not any(slot in supplied for slot in slots)
            for position in positions
        }

    def __call__(self, *positional: object, **named: object) -> Effect:
        if len(positional) > len(self.slots):
            raise TypeError(
                f"{self.name} takes {len(self.slots)} operand(s), "
                f"{', '.join(self.slots)}; {len(positional)} were given positionally"
            )
        bound: dict[str, object] = dict(zip(self.slots, positional, strict=False))
        for slot, value in named.items():
            if slot not in self.slots:
                raise TypeError(
                    f"{self.name} has no operand '{slot}'; it is written '{' '.join(self.phrase)}'"
                )
            if slot in bound:
                raise TypeError(f"{self.name} got operand '{slot}' twice")
            bound[slot] = value
        missing = [slot for slot in self.required if slot not in bound]
        if missing:
            raise TypeError(
                f"{self.name} is missing operand(s) {', '.join(missing)}; it is "
                f"written '{' '.join(self.phrase)}'"
            )
        return Effect(self, {slot: expression(value) for slot, value in bound.items()})

    def __repr__(self) -> str:
        return f"<lab action {' '.join(self.phrase)}>"


def _slot(word: str) -> str | None:
    """The operand name a phrase word stands for, if it is a slot at all."""

    if word.startswith("<") and word.endswith(">"):
        return word[1:-1]
    return None
