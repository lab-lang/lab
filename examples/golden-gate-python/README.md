# Golden Gate in Python

This is the Python counterpart to [`golden-gate`](../golden-gate). It describes separate GFP and RFP transformations with three and six DH5alpha replicates respectively, using a separate 1 mL cell stock aliquot for each group. Each reaction receives 20 µL of cells. The shared compiler derives nine reactions, 180 µL of cells consumed, and two separately reserved stock aliquots. The canonical Lab package's inventory supplies the 1 mL aliquot size and available count; `lab plan`, `lab build`, and the Python planning API expose the same calculated requirements.

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
