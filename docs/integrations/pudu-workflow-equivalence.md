# PUDU workflow equivalence for the Golden Gate OT-2 slice

This document defines and verifies the behavioral equivalence boundary between Lab's Golden Gate example and the complete workflow in PUDU's [documented workflow guide](https://pudu.readthedocs.io/en/latest/guide/workflow.html). The reference is PUDU's `workflow_example`; BuildCompiler inputs and notebooks are not part of this comparison.

## Pinned reference

`examples/golden-gate/reference/pudu-workflow.json` pins PUDU revision `1214d2f9efd557aa84bc96502379554174355eae`, the upstream paths and SHA-256 digests of `workflow_example/assembly_input.json` and `workflow_example/transformation_spec.json`, and the one permitted identity normalization. The corresponding input snapshots live under `examples/golden-gate/reference/pudu-workflow/` so source alignment is reviewable without running PUDU.

PUDU records SBOL 2 version-1 identities such as `https://SBOL2Build.org/composite_plasmid_1/1`. Lab records the corresponding SBOL 3 persistent identity `https://SBOL2Build.org/composite_plasmid_1`. The comparison removes only a terminal `/1` from absolute PUDU IRIs; it does not compare display names or invent identity aliases.

The shared experiment has two Golden Gate products and four transformed strains. Each plasmid is assembled once, each plasmid is transformed into both DH5alpha and BL21 with two transformation replicates, and every resulting culture is serially diluted twice and plated. PUDU's own generated handoff files determine the product wells, culture wells, dilution wells, and agar wells used on the reference side.

## Compiler boundary

Functional equivalence is defined at observable values and operations, not at Python source bytes or PUDU class structure.

| Concern | PUDU output | Lab owner |
| --- | --- | --- |
| Design identity | Product, part, backbone, enzyme, strain, and chassis URIs | Checked Design and Intent LAIR identities |
| Build ordering | Generated assembly-to-transformation and transformation-to-plating handoffs | Typed material dataflow and Method dependencies |
| Physical lineage | Product, culture, dilution, and agar well maps | Allocated Procedure schedule and plate-map evidence |
| Biological parameters | Reaction composition, replicates, thermal programs, dilution, and plating values | Method parameters normalized into Procedure programs |
| Liquid operation order | Generated aspirate, dispense, mix, finish, and tip operations | Ordered `PipettingProgramV1` steps plus adapter scheduling |
| Contamination control | Deliberate fresh/shared paths and no source re-entry | Canonical fluid-path constraints checked by the adapter |
| Liquid access | Source mixing, air gaps, blowout, touch-tip, relative access, and tracked aspiration | Portable technique requirements plus calibrated OT-2 profile values |
| Thermal control | Assembly cycling, heat shock, recovery incubation, and module setpoints | `ThermalProgramV1` plus the allocated OT-2 run |
| Robot commands | Opentrons API calls executed by the simulator | Standalone reviewed OT-2 protocol documents |

The portable Procedure contract states the behavior any implementation must preserve. Device calibration remains in the implementation profile. For example, a program can require liquid-level-aware aspiration and material-surface dispensing while the OT-2 profile supplies its measured conical-tube model and agar offset. A STAR implementation can satisfy those requirements with its own geometry, liquid classes, and firmware coordinates without copying Opentrons values.

## Reference behavior retained by Lab

Golden Gate setup balances each reaction to 20 µL and adds water, ligase buffer, ligase, BsaI, backbone, and ordered parts. PUDU mixes every non-water source before aspiration, transfers at the tuned rates with blowout and touch-tip, and reuses the final-part path for two bottom-relative 20 µL bubble-clearing strokes. Lab represents these as canonical pipetting operations and techniques before the OT-2 adapter sees them. The allocated assembly batch has one shared 4 °C source setpoint, and the adapter programs that shared resource once rather than once per reaction.

Assembly cycling remains a separate semantic task even when the OT-2 scheduler fuses compatible setup and cycle tasks into one run. The source-authored program is 75 repeats of 42 °C for 120 seconds and 16 °C for 300 seconds, followed by 60 °C for 600 seconds and 80 °C for 600 seconds, with a 42 °C lid and 4 °C final hold.

Transformation stages its competent-cell aliquot at 4 °C on a temperature-controlled position, which the Method states and the facility must offer. It preserves 20 µL competent cells, 2 µL DNA, two replicates, multi-plasmid co-transformation, exact DNA-product source wells, source mixing, zero-disposal cell distribution, destination touch-tip, and two bubble-clearing strokes. Heat shock is 4 °C for 1,800 seconds, 42 °C for 60 seconds, and 4 °C for 120 seconds. Recovery adds 60 µL medium with a 10 µL air gap and an above-liquid dispense, then incubates the resulting 82 µL culture at 37 °C for 3,600 seconds.

Every transformed-culture replicate receives two 10-fold dilution steps using 18 µL medium and 2 µL culture with five 19 µL mixes. The first clean path seeds dilution two before it touches agar and then plates dilution one; a fresh path plates dilution two. Each agar spot is 4 µL at the calibrated material-surface offset with blowout. Lab's plate-map JSON and PDF come from the same checked allocation used to render the robot protocol.

PUDU's volume-aware medium aspiration is retained as an attributed OT-2 realization. The profile models the 10 mL source in a 15 mL conical tube, recalculates height in eight-destination chunks, applies the calibrated sub-surface offset and floor, and switches to the low-volume fallback below the stated threshold. The reusable compiler requirement is liquid-level-aware aspiration from declared geometry, not PUDU's specific arithmetic.

## Executable differential comparison

Run the comparison from the Lab repository root with a PUDU checkout at the pinned revision whose `.venv` contains PUDU and the Opentrons simulator:

```bash
cargo build -p lab-cli
scripts/compare_pudu_golden_gate.py --pudu-repository ~/git/RudgeLab/PUDU
```

The script creates a retained temporary directory unless `--out-dir` names a new directory. It verifies the PUDU revision and input checksums, generates and simulates PUDU's assembly, transformation, and plating protocols exactly as its workflow guide does, builds and simulates Lab's Golden Gate package with the same simulator, preserves both raw output trees, and writes `comparison.json` plus every normalized facet used in the decision.

The comparison has ten required facets:

1. checked assembly input identity and composition;
2. checked transformation input identity and composition;
3. generated assembly-product handoff;
4. generated transformed-culture handoff;
5. end-to-end culture, dilution, and agar-well lineage;
6. assembly liquid actions and contamination boundaries;
7. transformation liquid actions and contamination boundaries;
8. plating liquid actions and contamination boundaries;
9. assembly staging and thermocycler program; and
10. transformation volume and thermal intent from the two generated outputs.

Liquid traces are reduced to leaf aspirate, dispense, blowout, and touch-tip operations. Tip pickup and drop operations define contamination boundaries between those actions: Lab must preserve every boundary PUDU uses, but may introduce an additional fresh tip without failing equivalence. Physical source wells are compared by the material the generated protocol placed in them, so a deliberate temperature-controlled cell position can be compared with PUDU's passive source-rack position without confusing competent cells with recovery medium. Tip-rack slots are facility configuration; deck locations that define product lineage, dilution layers, or agar destinations remain exact. The command exits nonzero and retains both normalized documents whenever any required facet differs.

At the pinned revisions, all ten facets pass. The common trace contains 595 equal leaf liquid operations: 184 in assembly, 153 in transformation, and 258 in dilution/plating.

## Explicit output differences

The report records observed differences separately from the equivalence facets instead of discarding them:

- PUDU switches its generated transformation implementation to `water_testing=True` under the Opentrons simulator, so its simulation omits heat shock and recovery incubation. The comparison reads the resolved configuration from PUDU's generated protocol to verify thermal intent, while Lab simulates its real reviewed thermal path.
- PUDU transforms in a NEST 100 µL thermocycler plate but its generated plating protocol loads a Bio-Rad 200 µL source plate. Lab preserves the NEST plate across the explicit handoff.
- PUDU's generic assembly temperature-module request resolves to GEN1 in the pinned simulator. Lab's facility explicitly declares a GEN2 temperature module. Both outputs use the required Thermocycler Module GEN1.
- PUDU's simulated transformation stages competent cells in a passive tube rack. Lab enforces the Method's 4 °C staging requirement on the facility's GEN2 temperature module.
- Lab takes a fresh large-volume tip for the second dilution-medium distribution chunk. PUDU reuses its first tip; the report records Lab's additional boundary and verifies that it removes no PUDU contamination boundary.
- Lab opens the thermocycler after final assembly and transformation thermal work so the next reviewed plate handoff is physically possible. PUDU's simulated protocols leave it closed.

These differences are not silently normalized into equality. Each is emitted with a classification and both observed values in `comparison.json`. They do not change the shared biological products, handoffs, liquid operations, or thermal intent.

## Regression and acceptance gates

`examples/golden-gate/reference/ot2-regression.json` is an internal Lab adapter regression fixture, not the external equivalence oracle. Rust integration tests use it to keep high-risk manifest values and generated-template behavior stable without requiring PUDU in every test run. No production compiler path reads it. External equivalence is established only by running both toolchains through `scripts/compare_pudu_golden_gate.py`.

The hosted Rust and Python suites cover canonical normalization, allocation, generated manifests, template structure, and the differential normalizers. The live PUDU comparison additionally requires PUDU's pinned environment and Opentrons simulator, so it remains an explicit acceptance command rather than an ordinary hermetic unit test.

Passing software equivalence is not hardware qualification. The next gate is an operator-reviewed water or inexpensive-surrogate run on the actual OT-2 with its Thermocycler Module GEN1, followed by a biological acceptance run whose evidence is recorded. Flex and STAR must independently implement and qualify the same portable Procedure semantics with their own calibrated profiles; copying OT-2 values would violate the device boundary.
