# Golden Gate cloning on an Opentrons OT-2

This is Lab's end-to-end example: a package that describes a small reporter
panel biologically, and compiles it into the robot protocols that build it.

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

The manifest declares `[build] target = "opentrons-ot2"`, so a plain `lab build`
compiles for that bench; `lab build --target <name>` compiles for another one,
and `lab build --no-target` stops at portable module IR.

The build prints the path of every runnable protocol it emitted:

```text
Robot protocols:
  .../.lab/build/opentrons-ot2/wave-001/assembly_protocol.py
  .../.lab/build/opentrons-ot2/wave-002/plating_protocol.py
  .../.lab/build/opentrons-ot2/wave-002/transformation_protocol.py
```

It writes those under `.lab/build/opentrons-ot2/`, one directory per planning wave:

| Path | Contents |
| --- | --- |
| `dependency_manifest.json` | machine-readable graph, waves, and blockers |
| `dependency_report.md` | human dependency and blocker summary |
| `manual_protocol.md` | consolidated bench instructions in execution order |
| `wave-001/` | assembly of both plasmids: one deck, one run |
| `wave-002/` | transformation and plating of all four strains |

Artifacts in the same wave have no ordering constraint between them, so a wave
is a single robot run over a single deck. Wave 2 cannot start until wave 1's
plasmids physically exist and have been accepted as suitable inputs.

## See the deck

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
