# Lab Python SDK

The Python package is a PyO3 binding over `lab-compiler` and an object model for writing Lab programs in Python. It does not reimplement parsing or semantic checking: the object model emits Lab source and the language's own frontend decides whether it is well formed.

Checking source text directly returns the backend-neutral checked module as Python-native data:

```python
from lab import compile_lab_module

module = compile_lab_module(source)
print(module["declarations"])
```

## Writing a program

A Lab module is a Python module. The standard library is mirrored as Python packages, so a kind is a class you import, its properties are keyword arguments, and a claim is a function of the artifact it is about:

```python
"""Two composite transcription units."""

import lab
from lab.bio.golden_gate import Plasmid
from lab.prelude import circular, dna
from lab.units import C, minutes, uL

from .inventory import B0034, BsaI, J23101, pSB1C3

module = lab.Module("golden_gate.designs.plasmids", doc=__doc__)

composite_plasmid_1 = Plasmid.build(
    sequence=dna("GCTAGCGGATCC"),
    backbone=pSB1C3,
    components=[J23101, B0034],
    restriction_enzyme=BsaI,
    reaction_volume=20 * uL,
    ligate_duration=5 * minutes,
    require=[lambda plasmid: plasmid.topology == circular],
    accept=[lambda plasmid: plasmid.sequence == plasmid.design.sequence],
)
```

That emits the `use` lines too. `Plasmid` imported from `lab.bio.golden_gate` is the Golden Gate view of a plasmid, so it carries both `std.bio.designs` and `std.bio.golden_gate`; `pSB1C3` carries the module that declared it. A module states `uses=[...]` only for a module nothing refers to by name.

A declaration takes its Lab name from the Python name it is bound to, so nothing is spelled twice; one generated in a loop states its own `name`. A claim is a function so that the artifact's properties arrive through a parameter rather than appearing from nowhere, which is also what lets a typechecker see them.

`lab.check` emits the modules in dependency order and hands the result to the compiler:

```python
program = lab.check(inventory.module, plasmids.module, strains.module)
print(program.sources["golden_gate.designs.plasmids"])
```

A rejected program raises `lab.LabError`, and every diagnostic carries the line of Python responsible for the Lab it is about alongside the compiler's own excerpt:

```
Lab reported 1 error(s)

  File "designs/plasmids.py", line 21

error: Plasmid has no property 'reaction_volme'
 --> golden_gate.designs.plasmids:6:3
  |
6 |   reaction_volme = 20 uL
  |   ^^^^^^^^^^^^^^
  |
  = help: did you mean 'reaction_volume'?
```

## The standard-library mirror

`lab.prelude` and `lab.bio.*` hold the same words `std.prelude` and `std.bio.*` do. They are generated from the compiler's own catalog, so the mirror cannot drift from what a Lab program sees, and they are checked in so an editor and a typechecker can use them without running anything. Regenerate after changing the standard library:

```sh
cd crates/lab-python && uv run python -m lab.codegen
```

`tests/test_codegen.py` fails if the checked-in mirror is stale. Modules of durable actions are not mirrored yet, because workflows are not written in Python.

## What is covered

Artifact declarations. Records, roles, circuits, and workflows are written as Lab source and checked with `compile_lab_module`.

The Golden Gate example's designs are written both ways: as Lab in [`examples/golden-gate/src/designs/`](../../examples/golden-gate/src/designs/) and with this SDK in [`tests/programs/golden_gate/`](tests/programs/golden_gate/). `tests/test_golden_gate.py` compiles both and requires the same checked module from each.

## Development

The Python module, native extension, tests, linter, and strict typechecker are one maintained unit. Run every gate from the repository root with:

```sh
scripts/check-python-sdk.sh
```

The gate runs against the installed extension, so rebuild it after changing Rust:

```sh
cd crates/lab-python && uv run maturin develop --uv
```
