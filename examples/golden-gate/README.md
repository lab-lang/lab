# Golden Gate cloning on an Opentrons OT-2

This is Lab's end-to-end facility example: a package describes separate GFP and RFP transformations, compiles them into portable capability requirements, allocates those requirements against an SBOLInventory facility, and lowers the resulting OT-2 bindings into automation protocols.

The workflow covers Golden Gate assembly, chemical transformation and heat shock, recovery-medium addition and incubation, replicate-aware serial dilution, and selective plating.

## What it builds

Two reporter plasmids are assembled and transformed separately into DH5alpha:

```text
GVD0011 (J23101 → GFP) ─> GFP_strain (DH5alpha), 3 replicates
GVD0013 (J23106 → RFP) ─> RFP_strain (DH5alpha), 6 replicates
```

Each strain receives only its own reporter plasmid. The assembly products are explicit material dependencies, so the compiler derives the assembly-to-transformation order and retains each product's provenance. The GFP assembly produces 25 µL; the RFP recipe is doubled proportionally to produce 50 µL for its six 5 µL DNA transfers.

Each transformation group provisions its own competent cells with the existing Lab syntax:

```lab
cells <- provision DH5alpha
strain, culture <- transform design from dependencies into cells
```

The planner follows that material into its consuming procedure and derives demand from the exact liquid transfers. The designs state three GFP replicates, six RFP replicates, and 20 µL of cells per reaction. The inventory lot states its packaging: two available 1 mL aliquots. Neither the total cell volume nor a source-vial count is an experiment parameter.

`lab plan` and `lab build` report 60 µL consumed for GFP and 120 µL for RFP, reserving one separate stock aliquot for each provisioning call. The resulting nine outputs consume 180 µL. Reservations are retained in the facility solution and adapter invocations, and the manual run sheet identifies the stock aliquots to stage. Python clients can read the same compiler result through `plan.material_requirements`.

Stock facts live on `lots:DH5alpha_lot` in `inventory/facility.ttl`, using Lab inventory extension properties `aliquotVolumeUl` and `aliquotCount`; optional `deadVolumeUl` describes inaccessible volume per aliquot. If smaller aliquots require more sources, the planner splits the cell distribution across those sources while preserving each output's dose. It rejects insufficient stock or an allocation the selected instrument cannot stage. Distinct provisioning calls reserve distinct aliquots; this is an immutable planning reservation, not a live inventory mutation.

The DNA sequences are first-class values declared independently of the designs that reference them. Provenance is separate again: `buy` marks catalogued parts and reagents, while `build` marks the plasmids and strains this laboratory makes.

## Check and build the experiment

From `examples/golden-gate`, run:

```bash
lab check
lab build
```

`lab build` emits checked module IR, refined Method alternatives, the global planning problem, the exact facility solution, Allocated Procedure LAIR, and immutable adapter invocations. It then derives OT-2 protocols and PDFs through the adapter bound to the selected Asset. Its output names each biological build product, compiler artifact, Asset bundle, automation protocol, operator document, and reviewed plan path.

## Facility-derived outputs

The package selects `inventory/facility.ttl`, a conformant SBOLInventory document containing the laboratory's zones, exact stock MaterialLots, a manual workstation, and an Opentrons OT-2 Asset with the original Temperature Module GEN1 and Thermocycler Module GEN1 installed. The Asset offers plannable metered transfer, in-well mixing, 4 °C temperature-controlled staging (which both the staged Golden Gate reagents and the competent-cell aliquot require), liquid-level-aware aspiration, vessel-relative liquid access, air-gap handling, post-dispense blowout, touch-tip, programmed block-temperature control, and heated-lid control. The thermal offering parameters state the installed thermocycler's 96-sample capacity, 10–100 µL working-volume range, 4–99 °C block range, and 37–110 °C lid range. The adapter profile selects a P20 Single-Channel GEN2 on the left mount, P300 Single-Channel GEN2 on the right mount, and generic device resources: sources on Temperature Module GEN1, a work plate on Thermocycler Module GEN1, a 15 mL bulk-liquid rack, a material-surface plate, and small and large tip racks. It uses Opentrons' GEN1 API load names, `temperature module` and `thermocycler module`; GEN2 modules remain separate supported profile choices rather than being inferred from the OT-2. The local adapter binding states that Lab's `opentrons.ot2` implementation can operate that exact Asset, while `adapters/opentrons-ot2.toml` supplies the reviewed resources and technique calibration.

```bash
lab run .lab/build --dry-run
```

The facility phase selects sixteen Method instances and binds 56 atomic requirements. Assembly setup and thermal cycling for each plasmid are followed by transformation setup, heat shock, recovery-medium addition, recovery incubation, serial dilution, and selective plating for each strain. The adapter emits sixteen separately staged Python protocols and sixteen operator PDFs, plus the plan's manual run sheet. The package requires an adapter for every non-manual requirement, so a missing implementation is a planning error. `lab plan` writes this facility phase separately under `.lab/plan/`.

| Path | Contents |
| --- | --- |
| `.lab/build/compiler/refined.lair` | all applicable portable Method, Procedure, and Capability alternatives |
| `.lab/build/compiler/planning-problem.json` | graph-wide Method and facility constraint problem |
| `.lab/build/compiler/facility-solution.json` | exact selected Methods, MaterialLots, offerings, Assets, and adapters |
| `.lab/build/compiler/allocated.lair` | verifier-valid selected Procedure graph with exact allocation bindings |
| `.lab/build/compiler/adapter-invocations.json` | immutable exact tasks and selected material bindings grouped by Asset and adapter |
| `.lab/build/facility_lowering.json` | emitted artifacts, formats, Requirements, profiles, and digests by Asset route |
| `.lab/build/plan.execution.json` | reviewed facility-wide dependency DAG and child documents |
| `.lab/build/assets/opentrons_ot2/tasks/NNN-pipetting-program/automation_protocol.py` | one canonical pipetting Procedure |
| `.lab/build/assets/opentrons_ot2/tasks/NNN-thermal-program/automation_protocol.py` | one canonical thermal Procedure |
| `tasks/NNN-*/invocation_manifest.json` within the Asset bundle | exact program, selected materials, requirements, device resources and logical-to-physical locations |
| `tasks/NNN-*/manual_protocol.pdf` within the Asset bundle | the corresponding operator instructions |

The Procedure graph retains typed edges from each built plasmid into its dependent strain workflow. Each file has its own reviewed physical allocation. Before running it, stage upstream values at the input wells in that file's manifest; preceding files may use different coordinates. Automatic preservation of physical locations between files is not implemented. The execution DAG orders the files and preserves material provenance, but does not perform those staging transfers.

The OT-2 offerings are `Plannable` with `ReviewedFileControl`. `lab run .lab/build --dry-run` verifies the inventory, compiler evidence, adapter profile, every scheduled protocol and support-artifact digest, and the complete DAG before narrating the plan. Each generated protocol is tied to one exact Procedure task and all of its allocated atomic requirements, but the example does not claim that this Asset is hardware-qualified for live execution.

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
  .lab/build/assets/opentrons_ot2/tasks/001-pipetting-program/automation_protocol.py
```

To lint, typecheck, and simulate the complete emitted OT-2 package:

```bash
ot2_output="$(pwd)/.lab/build/assets/opentrons_ot2"
../../scripts/check-opentrons-bundle.sh "$ot2_output"
../../scripts/simulate-opentrons.sh "$ot2_output"
```
