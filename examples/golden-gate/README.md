# Golden Gate cloning on an Opentrons OT-2

This is Lab's end-to-end example: a package that describes a small reporter
panel biologically, and compiles it into the automation protocols that build it.

It reproduces the three-stage workflow from
[PUDU](https://pudu.readthedocs.io/en/latest/guide/workflow.html) — Golden Gate
assembly, heat-shock transformation, and serial dilution with selective plating
— from two composite plasmids into four engineered strains.

## What it builds

Two transcription units, each a promoter driving a fluorescent reporter, are
assembled into the same backbone. Each is then introduced into two different
host organisms:

```text
composite_plasmid_1 (J23101 → GFP)      composite_plasmid_2 (J23106 → RFP)
   ├── composite_strain_1  (DH5alpha)      ├── composite_strain_2  (DH5alpha)
   └── composite_strain_3  (BL21)          └── composite_strain_4  (BL21)
```

One plasmid feeding two strains is the point of the example. A strain is its
own artifact, so DH5alpha carrying `composite_plasmid_1` and BL21 carrying the
same plasmid are two separate things to build and accept. Nothing in the source
says which order to build them in; the compiler derives that from the material
each workflow consumes.

## Build it

From the `examples/golden-gate` directory, run:

```bash
lab build
```

The manifest declares `[build] target = "opentrons-ot2"`, so a plain
`lab build` compiles for the OT-2 bench. `lab build --target <name>` compiles
for another bench, and `lab build --no-target` stops at portable module IR.

The build writes protocols under `.lab/build/opentrons-ot2/`, one directory
per planning wave, and prints the path of every runnable automation protocol.

The build output holds, per target directory:

| Path | Contents |
| --- | --- |
| `dependency_manifest.json` | machine-readable graph, waves, and blockers |
| `dependency_report.pdf` | typeset dependency and blocker summary (`.typ` source beside it) |
| `manual_protocol.pdf` | typeset bench instructions in execution order (`.typ` source beside it) |
| `lab-style.typ` | the shared document style; every directory holding a document carries a copy |
| `wave-001/` | assembly of both plasmids: one deck, one run |
| `wave-002/` | transformation and plating of all four strains |

Each output directory is a self-contained [Typst](https://typst.app) project:
`lab build` typesets the PDFs in-process (fonts embedded, no network), and
anyone with the `typst` CLI can restyle `lab-style.typ` and re-typeset a
document without the Lab toolchain.

Artifacts in the same wave have no ordering constraint between them, so a wave
is a single robot run over a single deck. Wave 2 cannot start until wave 1's
plasmids physically exist and have been accepted as suitable inputs.

## Build it for a different instrument

`targets/opentrons-flex.toml` describes an Opentrons Flex. It declares
`[target] backend = "opentrons.flex"`, and that key is what selects the
backend:

```bash
lab build --target opentrons-flex
```

The same programs, designs, and inventory produce the same waves under
`.lab/build/opentrons-flex/`, with each stage emitted as an Opentrons JSON
protocol (schema 8) rather than Python. Verify them with:

```bash
scripts/analyze-opentrons-flex.sh examples/golden-gate/.lab/build/opentrons-flex
```

`targets/hamilton-star.toml` describes a Hamilton STARlet. Its
`backend = "hamilton.star"` selects the firmware-protocol backend:

```bash
lab build --target hamilton-star
```

Each wave then contains `*.star.json` run documents — ordered, reviewable
Hamilton firmware frames with an operator description per step — plus the
manual protocol that interleaves the off-deck thermal work. Review a wave
without hardware, or execute it on the connected machine:

```bash
lab run examples/golden-gate/.lab/build/hamilton-star/wave-001 --dry-run
```

`targets/workcell-star.toml` composes the same STARlet with an Inheco ODTC
thermocycler and a human carrying the plate between them. Its
`backend = "workcell"` selects the multi-station backend:

```bash
lab build --target workcell-star
```

Each wave then holds per-station packages under `stations/` and a
`plan.workcell.json` coordination plan: the STAR's runs, the thermal
programs that would otherwise be operator prose (now `*.odtc.json`
documents the cycler executes), and an explicit handoff node for every
plate movement. `lab run` walks the plan, gates every handoff on the
operator, and records each node in `run-ledger.jsonl` so an interrupted
wave continues with `--resume`:

```bash
lab run examples/golden-gate/.lab/build/workcell-star/wave-001 --dry-run
```

## See the deck

`targets/opentrons-ot2.toml` describes an OT-2 (`lab build --target
opentrons-ot2` emits `*_protocol.py` under `.lab/build/opentrons-ot2/`).
Open the Opentrons app, go to **Protocols**, and either drag one of those
protocol files onto the window or use **Import a Protocol → Choose file** and
paste the path the build printed. The app analyzes it and draws the deck.

The app must have OT-2 support. Opentrons split that into a separate
application at version 9, so use the 8.4.x app or the `Opentrons-OT2` build; a
9.x app rejects these protocols with a message pointing at the OT-2 download.

To check a protocol without the GUI, run the app's own analyzer over it:

```bash
/Applications/Opentrons.app/Contents/Resources/python/bin/python3.10 \
  -m opentrons.cli analyze --json-output /tmp/analysis.json \
  examples/golden-gate/.lab/build/opentrons-ot2/wave-002/transformation_protocol.py
```

The JSON reports `errors` plus the full deck — modules, labware with slot
assignments, and pipettes with mounts — which is what the deck map renders.

## Verify the generated code

```bash
scripts/check-opentrons-target.sh examples/golden-gate/.lab/build/opentrons-ot2
```

```bash
scripts/simulate-opentrons.sh examples/golden-gate/.lab/build/opentrons-ot2
```

The first lints and typechecks every emitted protocol; the second runs them
through the official Opentrons simulator.
