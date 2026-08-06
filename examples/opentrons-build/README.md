# Opentrons OT-2 compiler fixtures

These two modules exercise the OT-2 specialization directly through `labc`, one
file at a time. For the end-to-end workspace — designs, target profile, and
`lab build --target` — see [`../golden-gate/`](../golden-gate/).

[`reporter-library.lab`](reporter-library.lab) is the minimal complete build:
two plasmids assembled by Golden Gate, then two strains transformed, recovered,
diluted, and plated. It makes the choices this backend needs explicit — typed
inventory identities, artifact properties, and the `realize`, provision,
`transform`, recovery, dilution, and plating effects. The final sequence remains
an acceptance obligation, not evidence that a physical build succeeded.

## Dependency-driven build

[`full-build.lab`](full-build.lab) nests four plasmids and one strain so the
compiler must derive a build order. Each `realize` consumes a list of typed
`Material<Plasmid>` dependencies, and that checked dataflow creates the graph;
[`full-build-inventory.json`](full-build-inventory.json) determines which
external materials are available. No declaration names an assembly level:

```text
reporter_host
└── final_device
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

Package every generated artifact:

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
| `manual_protocol.md` | consolidated human instructions in dependency-safe order |
| `dependency_manifest.json` | machine-readable roots, edges, attempts, products, and blockers |
| `dependency_report.md` | human-readable dependency and blocker summary |
| `wave-NNN/` | standalone manual, Opentrons protocols, and Lab manifest for one robot run |

Artifacts in one wave have no ordering constraint between them, so a wave is a
single robot run over one deck. A wave emits a protocol only for the stages its
artifacts reach: an assembly-only wave produces no plating protocol.

Artifact products become available to the next planning iteration. Removing a
leaf from the inventory produces `partial` with a structured blocker; adding an
artifact to `available_artifacts` skips its build while allowing dependents to
proceed. The consolidated manual tells the operator not to advance until each
dependency has been physically produced or retrieved and accepted as a suitable
input.

## Compile for a specific bench

Both modules compile against the backend's reference bench by default. Pass a
target profile to compile for another:

```sh
cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/reporter-library.lab \
  --emit automation-bundle \
  --target-profile examples/golden-gate/targets/bench-ot2.toml \
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

## Inspect each compiler output

```sh
cargo run -p lab-compiler --bin labc -- \
  examples/opentrons-build/reporter-library.lab \
  --emit automation-json
```

`manual-protocol`, `opentrons-assembly`, `opentrons-transformation`, and
`opentrons-plating` expose the other projections of the same plan.

## Simulate generated robot code

Create the local test runtime once, then run every generated protocol through
the official Opentrons simulator:

```sh
uv venv .lab/opentrons-venv --python 3.12
uv pip install --python .lab/opentrons-venv/bin/python 'opentrons>=8.4.1,<9'
scripts/simulate-opentrons.sh /tmp/lab-full-build
```

The Rust end-to-end test uses the same simulator when `LAB_OPENTRONS_SIMULATOR`
points to the executable. It remains optional so ordinary CI does not download
the large robotics runtime:

```sh
LAB_OPENTRONS_SIMULATOR=.lab/opentrons-venv/bin/opentrons_simulate \
  cargo test -p lab-compiler --test opentrons_build
```

## Safety and scope

The emitted protocols are a compiler concept spike, not a qualified wet-lab
protocol. Before execution, a laboratory must verify source concentrations,
overhang compatibility, internal restriction sites, labware definitions, deck
fit, liquid classes, tip policy, transformation conditions, selection media, and
organism-specific incubation requirements.

The source explicitly requests `realize`, `provision`, `transform`, `recover`,
`dilute`, and `plate`; dependency ordering remains abstract, while this target
specialization selects Golden Gate, heat shock, and the concrete deck layout. It
does not yet ingest SBOL, query a live inventory service, design overhangs,
rewrite internal sites, or attach runtime evidence to the final acceptance
decision.
