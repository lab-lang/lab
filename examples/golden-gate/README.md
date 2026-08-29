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

## Check and build the experiment

From `examples/golden-gate`, run:

```bash
lab check
lab build
```

`lab build` emits checked module IR, refined Method alternatives, the global planning problem, the exact facility solution, Allocated Procedure LAIR, and immutable adapter invocations. It then derives OT-2 protocols and PDFs through the adapter bound to the selected Asset. Its output names each biological build product, compiler artifact, Asset bundle, automation protocol, operator document, and reviewed plan path.

## Facility-derived outputs

The package selects `inventory/facility.ttl`, a conformant SBOLInventory document containing the laboratory's zones, exact stock MaterialLots, a manual workstation, and an Opentrons OT-2 Asset with plannable liquid-handling and thermal-cycling offerings. The local adapter binding states that Lab's `opentrons.ot2` implementation can operate that exact Asset.

```bash
lab run .lab/build --dry-run
```

The facility phase selects 22 Method instances and binds their 24 requirements to exact CapabilityOfferings and Assets. Because the allocated OT-2 has an installed lowering adapter, `lab build` emits independently reviewable Python protocols for the eight supported Procedure tasks without reading a package target. Manual provisioning, transformation, recovery, and plating remain explicit allocated work in the facility-wide plan without being misrepresented as OT-2 code. `lab plan` remains available when only this facility phase should be written separately under `.lab/plan/`.

| Path | Contents |
| --- | --- |
| `.lab/build/compiler/refined.lair` | all applicable portable Method, Procedure, and Capability alternatives |
| `.lab/build/compiler/planning-problem.json` | graph-wide Method and facility constraint problem |
| `.lab/build/compiler/facility-solution.json` | exact selected Methods, MaterialLots, offerings, Assets, and adapters |
| `.lab/build/compiler/allocated.lair` | verifier-valid selected Procedure graph with exact allocation bindings |
| `.lab/build/compiler/adapter-invocations.json` | immutable exact tasks grouped by selected Asset and adapter |
| `.lab/build/facility_lowering.json` | emitted artifacts, formats, Requirements, profiles, and digests by Asset route |
| `.lab/build/plan.execution.json` | reviewed facility-wide dependency DAG and child documents |
| `.lab/build/assets/opentrons_ot2/tasks/001-setup-golden-gate-reaction/` | exact setup task manifest, standalone Python protocol, and operator PDF |
| `.lab/build/assets/opentrons_ot2/tasks/002-thermal-cycle-golden-gate-reaction/` | exact cycling task manifest, standalone Python protocol, and operator PDF |
| `.lab/build/assets/opentrons_ot2/tasks/005-serial-dilution/` | one of four independently allocated dilution task bundles |

The Procedure graph preserves explicit typed edges between reaction setup and thermal cycling and from each built plasmid into its dependent strain workflows. The reviewed execution DAG is derived from those same selected values; an adapter does not reconstruct a separate wave or artifact graph.

The OT-2 offerings are `Plannable` with `ReviewedFileControl`. `lab run .lab/build --dry-run` verifies the inventory, compiler evidence, adapter profile, every exact-task protocol and support-artifact digest, and the complete DAG before narrating the plan. Each generated protocol is tied to one exact allocated Requirement, but the example does not claim that this Asset is hardware-qualified for live execution.

## Use another instrument

Another facility can run the same experiment by supplying an SBOLInventory document with compatible offerings and explicit adapter bindings for its exact Assets. Instrument choice is a facility-allocation result; the workflow does not use `--target` or name a backend.

## Inspect the OT-2 deck

Find the emitted protocols with:

```bash
find .lab/build/assets -name automation_protocol.py -print
```

Open the Opentrons app, go to **Protocols**, and import one of those files. The app must have OT-2 support; use the 8.4.x app or the `Opentrons-OT2` build because a 9.x app rejects OT-2 protocols.

To check a protocol without the GUI, run the app's analyzer over the selected file:

```bash
/Applications/Opentrons.app/Contents/Resources/python/bin/python3.10 \
  -m opentrons.cli analyze --json-output /tmp/analysis.json \
  "$(find .lab/build/assets -name automation_protocol.py -print -quit)"
```

To lint, typecheck, and simulate the complete emitted OT-2 package:

```bash
ot2_output=.lab/build/assets/opentrons_ot2
../../scripts/check-opentrons-bundle.sh "$ot2_output"
../../scripts/simulate-opentrons.sh "$ot2_output"
```
