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

```bash
cargo run -p lab-cli --bin lab -- build examples/golden-gate --target bench-ot2
```

The build prints the path of every runnable protocol it emitted:

```text
Robot protocols:
  .../.lab/build/bench-ot2/wave-001/assembly_protocol.py
  .../.lab/build/bench-ot2/wave-002/plating_protocol.py
  .../.lab/build/bench-ot2/wave-002/transformation_protocol.py
```

It writes those under `.lab/build/bench-ot2/`, one directory per planning wave:

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

Opening the file with `open -a Opentrons` does not work: the app registers no
handler for `.py`, so it launches without importing anything.

The app must have OT-2 support. Opentrons split that into a separate
application at version 9, so use the 8.4.x app or the `Opentrons-OT2` build; a
9.x app rejects these protocols with a message pointing at the OT-2 download.

To check a protocol without the GUI, run the app's own analyzer over it:

```bash
/Applications/Opentrons.app/Contents/Resources/python/bin/python3.10 \
  -m opentrons.cli analyze --json-output /tmp/analysis.json \
  examples/golden-gate/.lab/build/bench-ot2/wave-002/transformation_protocol.py
```

The JSON reports `errors` plus the full deck — modules, labware with slot
assignments, and pipettes with mounts — which is what the deck map renders.

## How the package is laid out

```text
lab.toml                          package identity, entry point, inventory
inventory.json                    what is physically on hand
targets/bench-ot2.toml            the bench: deck, labware, instruments
src/designs/inventory.lab         external inventory identities
src/designs/plasmids.lab          two composite plasmids
src/designs/strains.lab           four engineered strains
src/workflows/assemble.lab        stage 1
src/workflows/build_strains.lab   stages 2 and 3
src/programs/reporter_panel.lab   the runnable entry point
```

A program's modules are lowered together, so the designs in `src/designs/` and
the workflows that realize them stay in separate files.

The split between `src/` and `targets/` is the one that matters. Reagent
volumes, cycle counts, and heat-shock temperatures are scientific choices, so
they live with the designs in `.lab` source. Labware, deck slots, pipettes, and
mounts describe a particular bench, so they live in a target profile. Another
laboratory runs these same programs by writing its own profile.

Every field in a profile has a default matching the reference bench, so a real
profile is usually short. `targets/bench-ot2.toml` is written out in full only
because it is this example's subject. Declaring a second slot for a plate raises
the batch size the bench can hold without editing any program.

## Verify the generated code

```bash
scripts/check-opentrons-target.sh examples/golden-gate/.lab/build/bench-ot2
```

```bash
scripts/simulate-opentrons.sh examples/golden-gate/.lab/build/bench-ot2
```

The first lints and typechecks every emitted protocol; the second runs them
through the official Opentrons simulator.

## Scope

The emitted protocols are a compiler concept spike, not a qualified wet-lab
procedure. Before execution, a laboratory must verify source concentrations,
overhang compatibility, internal restriction sites, labware definitions, deck
fit, liquid classes, tip policy, transformation conditions, selection media, and
organism-specific incubation requirements.

Lab does not yet ingest SBOL, query a live inventory service, resolve inventory
lots, design overhangs, normalize source concentrations, prepare DNA between
waves, or attach runtime evidence to the acceptance claims these designs
declare. The sequences here are synthetic compiler fixtures.
