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

## Designs in pySBOL3

A design is an ordinary `sbol3.Component`, passed positionally to `build`. Where SBOL can already state something, the compiler reads it rather than asking twice: the referenced parts become `components` in the order the document's `meets` constraints put them, a readable sequence becomes `sequence`, circular topology becomes the requirement it already states, and the component's description becomes the declaration's documentation. What the declaration adds is what SBOL has no vocabulary for: provenance, acceptance claims, and a place in a build order.

```python
import sbol3

from lab.bio import golden_gate

design = sbol3.Component(
    "reporter",
    [sbol3.SBO_DNA, sbol3.SO_CIRCULAR],
    features=[J23101, B0034, GFP, B0015],
)

reporter = golden_gate.Plasmid.build(
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
def regulated_expression() -> lab.Layout:
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

Both readers are structural: neither `sbol3` nor `loica` is imported by the `lab` package, so anything with the right shape lowers and the package works without either installed. `pip install lab-sdk[bio]` brings in the pair the examples are written with.

## The standard-library mirror

`lab.prelude` and `lab.bio.*` hold the same words `std.prelude` and `std.bio.*` do. They are generated from the compiler's own catalog, so the mirror cannot drift from what a Lab program sees, and they are checked in so an editor and a typechecker can use them without running anything. Regenerate after changing the standard library:

```sh
cd crates/lab-python && uv run python -m lab.codegen
```

`tests/test_codegen.py` fails if the checked-in mirror is stale. Modules of durable actions are not mirrored yet, because workflows are not written in Python.

## What is covered

Artifact declarations, pySBOL3 designs, and LOICA circuits, including the records, catalogued parts, and bindings a circuit lowering mints. Roles and workflows are written as Lab source and checked with `compile_lab_module`.

The Golden Gate example's designs are written both ways: as Lab in [`examples/golden-gate/src/designs/`](../../examples/golden-gate/src/designs/) and with this SDK in [`tests/programs/golden_gate/`](tests/programs/golden_gate/). `tests/test_golden_gate.py` compiles both and requires the same checked module from each; `tests/test_sbol_designs.py` and `tests/test_loica_circuits.py` hold the SBOL and LOICA frontends to the same standard against hand-written Lab.

## Development

The Python module, native extension, tests, linter, and strict typechecker are one maintained unit. Run every gate from the repository root with:

```sh
scripts/check-python-sdk.sh
```

The gate runs against the installed extension, so rebuild it after changing Rust:

```sh
cd crates/lab-python && uv run maturin develop --uv
```
