# Lab

[Lab](https://www.lab-compiler.org) is a compiler for biology. It takes a description of what should exist, what must be true of it, and what evidence would accept it, and lowers that to something a person or a robot can run.

This distribution is `lab-compiler`, and it imports as `lab`:

```sh
pip install lab-compiler
```

Designs are written in [pySBOL3](https://pysbol3.readthedocs.io) and circuits in [LOICA](https://github.com/RudgeLab/LOICA), which is what the field already uses. Neither is required at runtime, because both are read structurally; install them alongside with `pip install "lab-compiler[bio]"`.

```python
import lab
from lab.bio.designs import Backbone
from lab.bio.golden_gate import Plasmid
from lab import circular, dna
from lab.units import ng, uL

module = lab.Module("reporter.designs")

pSB1C3 = Backbone.buy(identity="https://synbiohub.org/public/igem/pSB1C3/1")

reporter = Plasmid.build(
    sequence=dna("GCTAGCGGATCC"),
    backbone=pSB1C3,
    require=[lambda plasmid: plasmid.topology == circular],
    accept=[lambda built: built.concentration >= 100 * ng / uL],
)

lab.check(module)
```

A rejected program raises `lab.LabError`, and each diagnostic points at the line of Python that produced the Lab it is about.

Wheels are published for macOS, Linux, and Windows on CPython 3.11 and newer. Installing from source needs a Rust toolchain.

The rest of this file is the package's own documentation.

## How it works

The Python package is a PyO3 binding over the `lab-compiler` crate and an object model for writing Lab programs in Python. It does not reimplement parsing or semantic checking: the object model emits Lab source and the language's own frontend decides whether it is well formed.

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
from lab import circular, dna
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

## Designs in pySBOL3

A design is an ordinary `sbol3.Component`, passed positionally to `build`. Where SBOL can already state something, the compiler reads it rather than asking twice: the referenced parts become `components` in the order the document's `meets` constraints put them, a readable sequence becomes `sequence`, circular topology becomes the requirement it already states, and the component's description becomes the declaration's documentation. What the declaration adds is what SBOL has no vocabulary for: provenance, acceptance claims, and a place in a build order.

```python
import sbol3

from lab.bio.golden_gate import Plasmid

design = sbol3.Component(
    "reporter",
    [sbol3.SBO_DNA, sbol3.SO_CIRCULAR],
    features=[J23101, B0034, GFP, B0015],
)

reporter = Plasmid.build(
    design,
    backbone=pSB1C3,
    restriction_enzyme=BsaI,
    accept=[lambda built: built.sequence == built.design.sequence],
)
```

Each referenced part becomes a catalogued declaration whose identity is the registry IRI, because an imported component is something a supplier lists, not something this laboratory built.

## Networks in LOICA

A LOICA genetic network is how the field already designs circuits against SBOL, and it is not one circuit. It is a set of transcription units wired together by the gene products they express. Lab's `Circuit<Trigger, Product>` is one transcription unit, so `lab.circuit` lowers a network to one Lab circuit per operator, bound into a list.

The wiring is carried by the types rather than by a separate graph. A regulator is expressed by one unit and induces another, so it becomes a record playing both `Protein` and `Signal`, and the compiler checks the cascade: the second unit's promoter must respond to the first unit's product. Wiring a cascade to the wrong regulator is a type error.

The trigger and the product of each unit are read off the network, so `Circuit<Trigger, Product>` is never restated:

```python
import loica
import sbol3

import lab
from lab.bio.parts import B0015, B0034

aTc = loica.Supplement(name="aTc", sbol_comp=sbol3.Component("aTc", sbol3.SBO_SIMPLE_CHEMICAL))
sfGFP = loica.Reporter(
    name="sfGFP", sbol_comp=sbol3.Component("sfGFP", sbol3.SBO_DNA, roles=[sbol3.SO_CDS])
)
pTet = loica.Receiver(
    input=aTc,
    output=sfGFP,
    alpha=[0, 100],
    K=1,
    n=2,
    sbol_comp=sbol3.Component("pTet", sbol3.SBO_DNA, roles=[sbol3.SO_PROMOTER]),
)


@lab.circuit
def regulated_expression() -> lab.Network:
    """A promoter driving a coding sequence through a shared RBS and terminator."""
    network = loica.GeneticNetwork()
    network.add_operator(pTet)
    network.add_reporter(sfGFP)
    return lab.layout(network, rbs=B0034, terminator=B0015)


tet_reporter = regulated_expression()
```

`tet_reporter` is the list of units; its one unit checks as `Circuit<ATc, SfGFP>`. Each unit is reachable through `tet_reporter.units`, and a plasmid carries the whole network as `cargo=tet_reporter`.

Everything LOICA can build lowers:

| LOICA | Lab |
| --- | --- |
| `Source` (constitutive) | `Circuit<Constitutive, P>` |
| `Receiver`, `Hill1` | `Circuit<S, P>`, with `regulation` read from the Hill parameters |
| `Hill2`, `Sum` | `Promoter<Both<A, B>>`, nested for more than two inputs |
| polycistronic output | `CDS<Operon<A, B>>`, nested for more than two products |
| a regulator shared by two operators | one record playing `Protein` and `Signal` |
| a ring of repressors | units whose triggers and products close the cycle |

Polarity is not guesswork. LOICA states it in the Hill parameters, where a basal rate above the regulated rate means the promoter expresses less in the presence of its input, so the emitted promoter carries `regulation = repressed` or `regulation = induced`. The remaining Hill parameters (`alpha`, `K`, `n`) are characterization a catalogued promoter has no schema field for, so they stay on each unit as `unit.characterization` rather than lowering.

Both readers are structural: neither `sbol3` nor `loica` is imported by the `lab` package, so anything with the right shape lowers and the package works without either installed. `pip install "lab-compiler[bio]"` brings in the pair the examples are written with.

## Workflows

A workflow says how physical material moves once a design exists. It is the one place the object model cannot carry the program, because a workflow is control flow rather than a value: `if`, `match`, and `return` are statements Python runs rather than records. So a workflow is read from the function's own syntax.

```python
@lab.workflow
def build_reporter(wf: lab.Context) -> tuple[Material[Strain], Material[Plate]]:
    """Assemble the reporter, transform it, and plate what recovers."""
    product = wf.perform(lab.realize(reporter))
    cells = wf.perform(lab.provision(DH5alpha))
    strain, culture = wf.perform(lab.transform(reporter_host, plasmids=[product], cells=cells))
    culture = wf.perform(lab.recover(culture, duration=1 * h))
    plate = wf.perform(lab.plate(culture, antibiotic=chloramphenicol))
    return strain, plate
```

The distinction Lab writes with punctuation is a call here. `=` binds a computation, which a replay may run again because nothing in the world changed; `wf.perform` is a durable step, journaled once and never repeated, which Lab writes `<-`. Everything else follows from that:

| Python | Lab |
| --- | --- |
| `x = wf.perform(step)` | `x <- step` |
| `a, b = wf.perform(step)` | `a, b <- step` |
| `wf.perform(step)` alone | `<- step` |
| `x = f(y)` | `x = f(y)` |
| `xs = wf.state(list[T], [])` | `state xs: List<T> = []` |
| `@wf.every(30 * minutes)` | `when every 30 min:` |
| `@wf.after(18 * h)` | `when after 18 h:` |
| `wf.emit(Event(...))` | `emit Event{...}` |
| `wf.elapsed` | `workflow.elapsed` |
| `match x: case Growth.Ready():` | `match x:` / `case Ready:` |
| `wf.perform(other_workflow(arg))` | `<- other_workflow arg` |

Statements are translated from the syntax and expressions are evaluated, which is what keeps the rest of the SDK working inside a workflow: `30 * minutes` is a quantity because Python multiplied it, and the `use` lines still fall out of the names the body happens to mention.

Durable effects are the standard library's own actions, generated from the phrase Lab writes them with. An action's operands are keywords named after its slots, so `transform <design> from <plasmids> into <cells>` is `transform(design, plasmids=..., cells=...)`. An operand is one word in Lab, so anything built in place is bound above the step, exactly as the hand-written form does:

```python
strain, culture = wf.perform(lab.transform(host, plasmids=[product], cells=cells))
```

```lab
plasmids = [product]
strain, culture <- transform host from plasmids into cells
```

Records and their cases are classes. The roles a record plays are its base classes, and a case is a nested class:

```python
@lab.record
class ColonyGrowth:
    """What watching a plate produced."""

    plate: Material[Plate]
    observations: list[PlateObservation]

    @lab.case
    class Ready:
        colonies: ColonyMap

    @lab.case
    class TimedOut:
        pass
```

A form with no Lab meaning is refused where it is written, with the line of Python in the message: a `while` loop, an untyped parameter, a bare expression that performs nothing.

One limitation worth knowing: a record's fields are Lab types written as annotations, so the constructor mypy would need to see is one the decorator builds at runtime. Reading that statically is what a mypy plugin is for, the way dataclasses have one. Until there is one, a program written in the object model is checked by the Lab compiler rather than by mypy, and this repository's own gate disables those checks for its example programs while keeping the package itself strict.

## The standard-library mirror

`lab` itself and `lab.bio.*` hold the same words `std.prelude` and `std.bio.*` do. Lab imports the prelude into every module without being asked, and the Python namespace that is always reachable is the package, so `from lab import Material, dna` is what `use std.prelude` would have been. They are generated from the compiler's own catalog, so the mirror cannot drift from what a Lab program sees, and they are checked in so an editor and a typechecker can use them without running anything. Regenerate after changing the standard library:

```sh
cd crates/lab-python && uv run python -m lab.codegen
```

`tests/test_codegen.py` fails if the checked-in mirror is stale. Types and roles are generated as classes so annotations written with them typecheck; values, functions, and durable actions are generated as the objects that render them.

## What is covered

Artifact declarations, pySBOL3 designs, LOICA networks, records with cases, and workflows including reactive ones. Roles are written as Lab source and checked with `compile_lab_module`.

The Golden Gate example's designs are written both ways: as Lab in [`examples/golden-gate/src/designs/`](../../examples/golden-gate/src/designs/) and with this SDK in [`tests/programs/golden_gate/`](tests/programs/golden_gate/). `tests/test_golden_gate.py` compiles both and requires the same checked module from each; `tests/test_sbol_designs.py`, `tests/test_loica_circuits.py`, and `tests/test_workflows.py` hold the SBOL, LOICA, and workflow frontends to the same standard against hand-written Lab.

## Development

The Python module, native extension, tests, linter, and strict typechecker are one maintained unit. Run every gate from the repository root with:

```sh
scripts/check-python-sdk.sh
```

The gate runs against the installed extension, so rebuild it after changing Rust:

```sh
cd crates/lab-python && uv run maturin develop --uv
```

## Releasing

The distribution is `lab-compiler` on PyPI and the package it installs is `lab`. Its version is the Cargo workspace version, so the wheels and the CLI binaries ship from one tag and cannot disagree about what release they are.

Pushing a `v*.*.*` tag builds abi3 wheels for macOS, Linux, and Windows, builds an sdist carrying the workspace crates the bindings compile from, and uploads both to PyPI. One abi3 wheel per platform serves CPython 3.11 and newer, so the matrix is platforms rather than platforms times interpreter versions.

Build the artifacts locally to see exactly what a release would upload:

```sh
cd crates/lab-python
uv run maturin build --release --out dist
uv run maturin sdist --out dist
```

Uploads use [trusted publishing](https://docs.pypi.org/trusted-publishers/), so there is no API token in the repository. It needs two things set up once, and until they exist the `pypi` job is the only part of a release that fails:

- a PyPI trusted publisher for the project, naming this repository and `release.yml` as the workflow;
- a GitHub environment named `pypi`, which is where a required-reviewer rule goes if a release should pause for a human.
