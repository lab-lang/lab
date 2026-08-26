# Golden Gate extended in Python

This is the Python counterpart to [`golden-gate-extended`](../golden-gate-extended). It keeps the larger example's built and bought plasmids, four strains, acceptance claims, genetic circuit panel, reactive colony observation, and workflow composition while using the Python frontend throughout.

The example deliberately uses each biological integration at its natural boundary:

- `lab.sbol.Document` constructs typed DNA parts and plasmid designs without direct RDF graph manipulation;
- `.buy(...)` and `.build(...)` keep procurement separate from design identity;
- LOICA describes the three regulated transcription units;
- Python classes declare evidence records and tagged outcomes; and
- `@lab.workflow` translates durable effects, timers, state, branching, and material flow into the same checked modules as Lab source.

From this directory, run it against the repository's SDK environment:

```bash
uv run --project ../../crates/lab-python python -m golden_gate_extended
```

With `lab-compiler[bio]` already installed, run:

```bash
python -m golden_gate_extended
```

The current project CLI discovers written `.lab` and SBOL files, while Python modules enter through the SDK. This example checks the portable modules but does not invoke the target-specific `lab build` step.
