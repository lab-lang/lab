# Golden Gate in Python

This is the Python counterpart to [`golden-gate`](../golden-gate). It describes the same three plasmids, one three-replicate DH5alpha cotransformation, and material-dependent build order through Lab's typed Python frontend.

The biological designs use `lab.sbol.Document`, whose factories retain whether a component is a promoter, coding sequence, terminator, backbone, or plasmid. DNA sequences are independent typed document values referenced by those designs. The declarations around the designs state provenance separately: ordered parts use `.buy(...)`, while plasmids and strains made by this laboratory use `.build(...)`.

From this directory, run the example against the repository's Python SDK environment:

```bash
uv run --project ../../crates/lab-python python -m golden_gate
```

With `lab-compiler[bio]` already installed, the ordinary command is:

```bash
python -m golden_gate
```

The command imports every module in dependency order and passes them to `lab.check`. A compiler diagnostic points back to the Python declaration or workflow statement that produced it.

The current project CLI discovers written `.lab` and SBOL files, while Python modules enter through the SDK. This example therefore checks the same portable modules as the Lab version but does not invoke package-oriented `lab build` or facility planning.
