# Golden Gate cloning on an Opentrons OT-2

This is Lab's end-to-end facility example: a package describes a small reporter panel biologically, compiles it into portable capability requirements, allocates those requirements against an SBOLInventory facility, and lowers the resulting OT-2 bindings into automation protocols.

It reproduces the three-stage workflow from [PUDU](https://pudu.readthedocs.io/en/latest/guide/workflow.html): Golden Gate assembly, heat-shock transformation, and serial dilution with selective plating from two composite plasmids into four engineered strains.

## What it builds

Two transcription units, each a promoter driving a fluorescent reporter, are assembled into the same backbone. Each is then introduced into two different host organisms:

```text
composite_plasmid_1 (J23101 → GFP)      composite_plasmid_2 (J23106 → RFP)
   ├── composite_strain_1  (DH5alpha)      ├── composite_strain_2  (DH5alpha)
   └── composite_strain_3  (BL21)          └── composite_strain_4  (BL21)
```

One plasmid feeding two strains is the point of the example. A strain is its own artifact, so DH5alpha carrying `composite_plasmid_1` and BL21 carrying the same plasmid are two separate things to build and accept. Nothing in the source says which order to build them in; the compiler derives that from the material each workflow consumes.

The DNA sequences are first-class values declared independently of the designs that reference them. Provenance is separate again: `buy` marks catalogued parts and reagents, while `build` marks the plasmids and strains this laboratory makes.

## Check and build the portable experiment

From `examples/golden-gate`, run:

```bash
lab check
lab build
```

`lab build` emits checked module IR, reachable capability requirements, and exact inventory and adapter-binding snapshots under `.lab/build/`. It does not select an instrument or emit device protocols.

## Plan and lower against the facility

The package selects `inventory/facility.ttl`, a conformant SBOLInventory document containing the laboratory's zones, exact stock MaterialLots, a manual workstation, and an Opentrons OT-2 Asset with plannable liquid-handling and thermal-cycling offerings. The local adapter binding states that Lab's `opentrons.ot2` implementation can operate that exact Asset.

```bash
lab plan
lab run .lab/plan --dry-run
```

`lab plan` binds every reachable requirement to one exact CapabilityOffering and Asset. Because the allocated OT-2 has an installed lowering adapter, the same command emits three OT-2 Python protocols without reading a package target.

| Path | Contents |
| --- | --- |
| `.lab/plan/facility_allocation.json` | requirement-to-offering-to-Asset allocation and rejected candidates |
| `.lab/plan/facility_lowering.json` | exact Asset, adapter, profile digest, triggering requirements, emitted artifacts, and artifact digests |
| `.lab/plan/plan.execution.json` | reviewed facility-wide DAG and hash-addressed adapter-lowering child bundle |
| `.lab/plan/lowerings/<asset>/opentrons-ot2/dependency_manifest.json` | material graph, exact MaterialLot bindings, waves, and blockers |
| `.lab/plan/lowerings/<asset>/opentrons-ot2/dependency_report.pdf` | typeset dependency and blocker summary |
| `.lab/plan/lowerings/<asset>/opentrons-ot2/manual_protocol.pdf` | typeset bench instructions in execution order |
| `.lab/plan/lowerings/<asset>/opentrons-ot2/wave-001/` | assembly of both plasmids |
| `.lab/plan/lowerings/<asset>/opentrons-ot2/wave-002/` | transformation and plating of all four strains |

Artifacts in the same wave have no ordering constraint between them, so a wave is one robot run over one deck. Wave 2 cannot start until wave 1's plasmids physically exist and have been accepted as suitable inputs.

The OT-2 offerings are `Plannable` with `ReviewedFileControl`. `lab run .lab/plan --dry-run` verifies every frozen protocol and support-artifact digest before narrating the plan. The Execute nodes remain planning-only because the current OT-2 lowerer emits one whole-program bundle rather than an independently executable document per capability requirement; the example does not claim that this Asset is hardware-qualified for live execution.

## Use another instrument

Another facility can run the same experiment by supplying an SBOLInventory document with compatible offerings and explicit adapter bindings for its exact Assets. Instrument choice is a facility-allocation result; the workflow does not use `--target` or name a backend.

## Inspect the OT-2 deck

Find the emitted protocols with:

```bash
find .lab/plan/lowerings -name '*_protocol.py' -print
```

Open the Opentrons app, go to **Protocols**, and import one of those files. The app must have OT-2 support; use the 8.4.x app or the `Opentrons-OT2` build because a 9.x app rejects OT-2 protocols.

To check a protocol without the GUI, run the app's analyzer over the selected file:

```bash
/Applications/Opentrons.app/Contents/Resources/python/bin/python3.10 \
  -m opentrons.cli analyze --json-output /tmp/analysis.json \
  "$(find .lab/plan/lowerings -name transformation_protocol.py -print -quit)"
```

To lint, typecheck, and simulate the complete emitted OT-2 package:

```bash
ot2_output="$(find .lab/plan/lowerings -type d -name opentrons-ot2 -print -quit)"
../../scripts/check-opentrons-bundle.sh "$ot2_output"
../../scripts/simulate-opentrons.sh "$ot2_output"
```
