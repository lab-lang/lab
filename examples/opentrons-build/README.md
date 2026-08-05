# Golden Gate to Opentrons tutorial

This concept example compiles two plasmid declarations and explicit standard-library realization workflows into a deterministic batch spanning:

1. Golden Gate assembly;
2. heat-shock transformation into competent cells;
3. two serial dilutions and selective plating;
4. a human-readable protocol;
5. three standalone Opentrons OT-2 Python protocols.

The example deliberately makes the choices required by this backend explicit in [`reporter-library.lab`](reporter-library.lab): typed inventory identities and plasmid metadata plus `realize`, provision, transformation, recovery, dilution, and plating effects. The final sequence remains an acceptance obligation, not evidence that a physical build succeeded.

## Dependency-driven full build

[`full-build.lab`](full-build.lab) expresses a nested build with ordinary plasmid declarations and workflows using bundled `std.bio.build` and `std.bio.inventory` modules. Inventory constructors associate external inventory identities with typed source symbols; plasmid metadata and component lists refer to those symbols. Each `realize` operation consumes a list of typed `Material<Plasmid>` dependencies. That checked workflow dataflow creates the graph, while [`full-build-inventory.json`](full-build-inventory.json) determines which external materials are available. The compiler derives three build waves without named biological levels:

```text
final_device
├── reporter_region
│   └── promoter_carrier
└── regulator_region
```

Inspect the fixed-point plan:

```sh
cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/full-build.lab \
  --emit dependency-plan \
  --inventory examples/opentrons-build/full-build-inventory.json
```

Package every generated artifact as an independently reviewable Lab/Opentrons batch:

```sh
cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/full-build.lab \
  --emit full-build-bundle \
  --inventory examples/opentrons-build/full-build-inventory.json \
  --output-dir /tmp/lab-full-build
```

The package root contains:

| File | Purpose |
| --- | --- |
| `manual_protocol.md` | consolidated human instructions in dependency-safe batch order |
| `dependency_manifest.json` | machine-readable roots, edges, attempts, products, and blockers |
| `dependency_report.md` | human-readable dependency and blocker summary |
| `batch-NNN-artifact/` | standalone manual, Opentrons protocols, and Lab manifest for one generated artifact |

Artifact products become available to the next planning iteration. Removing a leaf from the inventory produces `partial` with a structured blocker; adding an artifact to `available_artifacts` skips its build while allowing dependents to proceed. The consolidated manual tells the operator not to advance until each dependency has been physically produced or retrieved and accepted as a suitable input.

## Simulate generated robot code

Create the local test runtime once, then run every generated protocol through the official Opentrons simulator:

```sh
uv venv .lab/opentrons-venv --python 3.12
uv pip install --python .lab/opentrons-venv/bin/python 'opentrons>=8.4.1,<9'
scripts/simulate-opentrons.sh /tmp/lab-full-build
```

The Rust end-to-end test uses the same simulator when `LAB_OPENTRONS_SIMULATOR` points to the executable. It remains optional so ordinary CI does not download the large robotics runtime:

```sh
LAB_OPENTRONS_SIMULATOR=.lab/opentrons-venv/bin/opentrons_simulate \
  cargo test -p lab-compiler --test opentrons_build
```

## Inspect each compiler output

From the repository root:

```sh
cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/reporter-library.lab \
  --emit automation-json

cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/reporter-library.lab \
  --emit manual-protocol

cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/reporter-library.lab \
  --emit opentrons-assembly
```

`opentrons-transformation` and `opentrons-plating` expose the other two robot stages.

## Write the complete bundle

```sh
cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/reporter-library.lab \
  --emit automation-bundle \
  --output-dir /tmp/lab-reporter-build
```

The output directory contains:

| File | Purpose |
| --- | --- |
| `automation_manifest.json` | Lab's deterministic construct, well, and handoff plan |
| `manual_protocol.md` | English instructions with reaction and plate maps |
| `assembly_protocol.py` | Golden Gate setup and thermocycling for OT-2 |
| `transformation_protocol.py` | competent-cell transfer, heat shock, and recovery for OT-2 |
| `plating_protocol.py` | dilution and selective plating for OT-2 |

## Safety and scope

The emitted protocols are a compiler concept spike, not a qualified wet-lab protocol. Before execution, a laboratory must verify source concentrations, overhang compatibility, internal restriction sites, labware definitions, deck fit, liquid classes, tip policy, transformation conditions, selection media, and organism-specific incubation requirements.

This is a Lab-native narrow lowering for one OT-2 use case. The source explicitly requests `assemble`, `transform`, `recover`, `dilute`, and `plate`; dependency ordering remains abstract, while this target specialization selects Golden Gate, heat shock, and the concrete deck layout. It does not yet ingest SBOL, query a live inventory service, design overhangs, rewrite internal sites, or attach runtime evidence to the final acceptance decision.
