# Golden Gate cloning on an Opentrons OT-2

This is Lab's end-to-end facility example: a package describes a small reporter panel biologically, compiles it into portable capability requirements, allocates those requirements against an SBOLInventory facility, and lowers the resulting OT-2 bindings into automation protocols.

It reproduces the complete cloning workflow represented by [PUDU](https://pudu.readthedocs.io/en/latest/guide/workflow.html): Golden Gate assembly, chemical transformation and heat shock, recovery-medium addition and incubation, replicate-aware serial dilution, and selective plating from two composite plasmids into four engineered strains.

## What it builds

Two transcription units, each a promoter driving a fluorescent reporter, are assembled into the same backbone. Each is then introduced into two different host organisms:

```text
composite_plasmid_1 (J23101 → GFP)      composite_plasmid_2 (J23106 → RFP)
   ├── composite_strain_1  (DH5alpha)      ├── composite_strain_2  (DH5alpha)
   └── composite_strain_3  (BL21)          └── composite_strain_4  (BL21)
```

One plasmid feeding two strains is the point of the example. A strain is its own artifact, so DH5alpha carrying `composite_plasmid_1` and BL21 carrying the same plasmid are two separate things to build and accept. Nothing in the source says which order to build them in; the compiler derives that from the material each workflow consumes.

The DNA sequences are first-class values declared independently of the designs that reference them. Provenance is separate again: `buy` marks catalogued parts and reagents, while `build` marks the plasmids and strains this laboratory makes.

## The shared PUDU workflow input

This example is the Lab form of PUDU's documented `workflow_example`, not a reconstruction of a separate BuildCompiler notebook. The plasmid products, ordered parts, backbone, restriction enzyme, four strain products, two chassis, transformation replicates, and plating parameters correspond directly to PUDU's canonical `assembly_input.json` and `transformation_spec.json`. Pinned snapshots of those two inputs live under `reference/pudu-workflow/`, and `reference/pudu-workflow.json` records their upstream paths, revision, and checksums.

PUDU's fixtures use SBOL 2 versioned identities ending in `/1`; Lab's SBOL 3 designs and inventory use the corresponding persistent identities without the terminal version segment. That is the only identity normalization needed to compare the program inputs.

To run both complete toolchains and compare their generated handoffs, lineage, thermal intent, and normalized Opentrons actions, use:

```bash
cargo build -p lab-cli
../../scripts/compare_pudu_golden_gate.py --pudu-repository ~/git/RudgeLab/PUDU
```

The comparison retains both raw output trees, writes every normalized facet and a machine-readable `comparison.json`, and exits nonzero if a required facet differs. See the [PUDU workflow equivalence audit](../../docs/integrations/pudu-workflow-equivalence.md) for the exact boundary and the explicitly reported upstream and facility differences.

## Check and build the experiment

From `examples/golden-gate`, run:

```bash
lab check
lab build
```

`lab build` emits checked module IR, refined Method alternatives, the global planning problem, the exact facility solution, Allocated Procedure LAIR, and immutable adapter invocations. It then derives OT-2 protocols and PDFs through the adapter bound to the selected Asset. Its output names each biological build product, compiler artifact, Asset bundle, automation protocol, operator document, and reviewed plan path.

## Facility-derived outputs

The package selects `inventory/facility.ttl`, a conformant SBOLInventory document containing the laboratory's zones, exact stock MaterialLots, a manual workstation, and an Opentrons OT-2 Asset with the original Thermocycler Module GEN1 installed. The Asset offers plannable metered transfer, in-well mixing, 4 °C temperature-controlled staging (which both the staged Golden Gate reagents and the competent-cell aliquot require), liquid-level-aware aspiration, vessel-relative liquid access, air-gap handling, post-dispense blowout, touch-tip, programmed block-temperature control, and heated-lid control. The thermal offering parameters state the installed module's 96-sample capacity, 10–100 µL working-volume range, 4–99 °C block range, and 37–110 °C lid range. The adapter profile uses Opentrons' GEN1 API load name, `thermocycler module`; GEN2 remains a separate supported profile choice rather than being inferred from the OT-2. The local adapter binding states that Lab's `opentrons.ot2` implementation can operate that exact Asset, while `adapters/opentrons-ot2.toml` supplies the reviewed deck and technique calibration.

```bash
lab run .lab/build --dry-run
```

The facility phase selects 22 Method instances and binds 88 atomic requirements to exact CapabilityOfferings and Assets. Four manual material-provisioning requirements remain on the operator workstation. The other 84 requirements belong to 28 normalized Procedure tasks allocated to the OT-2: two assembly tasks for each plasmid, followed by transformation setup, heat shock, recovery-medium addition, recovery incubation, serial dilution, and selective plating for each strain. The adapter preserves those scientific task identities while scheduling them into three reviewed device runs: assembly, transformation, and plating. `lab build` emits three standalone Python protocols, three run-level operator PDFs, and one static aggregate plate-map PDF. `lab plan` remains available when only this facility phase should be written separately under `.lab/plan/`.

| Path | Contents |
| --- | --- |
| `.lab/build/compiler/refined.lair` | all applicable portable Method, Procedure, and Capability alternatives |
| `.lab/build/compiler/planning-problem.json` | graph-wide Method and facility constraint problem |
| `.lab/build/compiler/facility-solution.json` | exact selected Methods, MaterialLots, offerings, Assets, and adapters |
| `.lab/build/compiler/allocated.lair` | verifier-valid selected Procedure graph with exact allocation bindings |
| `.lab/build/compiler/adapter-invocations.json` | immutable exact tasks and selected material bindings grouped by Asset and adapter |
| `.lab/build/facility_lowering.json` | emitted artifacts, formats, Requirements, profiles, and digests by Asset route |
| `.lab/build/plan.execution.json` | reviewed facility-wide dependency DAG and child documents |
| `.lab/build/assets/opentrons_ot2/execution_schedule.json` | versioned execution groups, dependencies, and persistent physical locations |
| `.lab/build/assets/opentrons_ot2/assembly_protocol.py` | both reaction setups and their shared authored thermal program |
| `.lab/build/assets/opentrons_ot2/transformation_protocol.py` | all competent-cell/DNA setup, heat shock, recovery-medium addition, and recovery incubation work |
| `.lab/build/assets/opentrons_ot2/plating_protocol.py` | all replicate-aware dilution and selective plating work with PUDU's contamination-safe two-tip order |
| `.lab/build/assets/opentrons_ot2/plate_map.pdf` | static aggregate selective-plate allocation generated from the same checked schedule as the robot protocol |

The Procedure graph preserves explicit typed edges between reaction setup and thermal cycling and from each built plasmid into its dependent strain workflows. The allocated schedule freezes the exact assembly-product wells used as transformation DNA sources and the exact recovered-culture wells used by dilution. The reviewed execution DAG is derived from those same selected values; an adapter does not reconstruct a separate wave or artifact graph.

The OT-2 offerings are `Plannable` with `ReviewedFileControl`. `lab run .lab/build --dry-run` verifies the inventory, compiler evidence, adapter profile, every scheduled protocol and support-artifact digest, and the complete DAG before narrating the plan. Each generated protocol is tied to an exact execution group containing the union of its allocated Procedure tasks and atomic requirements, but the example does not claim that this Asset is hardware-qualified for live execution.

## Use another instrument

Another facility can run the same experiment by supplying an SBOLInventory document with compatible offerings and explicit adapter bindings for its exact Assets. Instrument choice is a facility-allocation result; the workflow does not use `--target` or name a backend.

## Inspect the OT-2 deck

Find the emitted protocols with:

```bash
find .lab/build/assets/opentrons_ot2 -name '*_protocol.py' -print
```

Open the Opentrons app, go to **Protocols**, and import one of those files. The app must have OT-2 support; use the 8.4.x app or the `Opentrons-OT2` build because a 9.x app rejects OT-2 protocols.

To check a protocol without the GUI, run the app's analyzer over the selected file:

```bash
/Applications/Opentrons.app/Contents/Resources/python/bin/python3.10 \
  -m opentrons.cli analyze --json-output /tmp/analysis.json \
  .lab/build/assets/opentrons_ot2/assembly_protocol.py
```

To lint, typecheck, and simulate the complete emitted OT-2 package:

```bash
ot2_output=.lab/build/assets/opentrons_ot2
../../scripts/check-opentrons-bundle.sh "$ot2_output"
../../scripts/simulate-opentrons.sh "$ot2_output"
```
