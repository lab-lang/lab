# PUDU transformation equivalence for the Golden Gate OT-2 slice

This audit compares Lab's Golden Gate transformation protocol with PUDU's exact main transformation entrypoint, [`scripts/automated_ot2/run_sbol2transformation_with_params.py`](https://github.com/RudgeLab/PUDU/blob/main/scripts/automated_ot2/run_sbol2transformation_with_params.py). It does not use PUDU's older `workflow_example` fixtures and does not make the Golden Gate package depend on or refer to PUDU.

## Pinned source

[`scripts/reference/pudu-transformation-entrypoint.json`](../../scripts/reference/pudu-transformation-entrypoint.json) pins PUDU revision `1214d2f9efd557aa84bc96502379554174355eae` and the SHA-256 digests of both the entrypoint and [`src/pudu/transformation.py`](https://github.com/RudgeLab/PUDU/blob/main/src/pudu/transformation.py). The comparison refuses a different revision, tracked source changes, or a digest mismatch.

The entrypoint requests one strain named `GVD_strain`: DH5alpha cotransformed with `GVD0011`, `GVD0013`, and `GVD0015` in three replicate wells. Each DNA source contains 25 µL, each competent-cell source contains 150 µL, and the recovery-medium source contains 1,200 µL. Each reaction receives 20 µL competent cells, 5 µL of every plasmid, and 60 µL recovery medium. Heat shock is 4 °C for 30 minutes, 42 °C for one minute, and 4 °C for two minutes. Recovery is 37 °C for 60 minutes.

## Equivalence boundary

The audit requires three independently reviewable facets to pass:

1. `configuration`: biological input, replicate count, source and transfer volumes, API level 2.20, thermal profiles, pipette models and mounts, module load names, labware, slots, and capacities;
2. `robot-actions.transformation`: every material-normalized leaf aspirate, dispense, blowout, touch-tip action, and tip-change boundary; and
3. `resolved-hardware`: P20 Single-Channel GEN2 on the left mount, P300 Single-Channel GEN2 on the right mount, Temperature Module GEN1, Thermocycler Module GEN1, and the NEST 100 µL PCR plate.

Physical source locations are normalized only by explicit material identity. This permits comparison of the standalone PUDU setup, where DNA tubes occupy the temperature module, with the end-to-end Lab workflow, where assembly products retain their output wells and competent cells occupy the facility's 4 °C controlled position. Product and reaction destinations are not normalized away.

## Run the differential

From the Lab repository root, with PUDU installed in its pinned checkout's `.venv`:

```bash
cargo build -p lab-cli
scripts/compare_pudu_golden_gate.py --pudu-repository ~/git/RudgeLab/PUDU
```

The script builds the Golden Gate package, simulates its transformation protocol and the exact PUDU entrypoint with the same Opentrons simulator, retains both raw traces, and writes every normalized facet plus `comparison.json` to a retained temporary directory. Any required facet mismatch exits nonzero.

At the pinned revision, all three facets pass. Both traces contain 143 equal normalized liquid actions with the same 11 tip boundaries.

## Reported implementation differences

The report preserves three differences that must not be mistaken for protocol intent equivalence:

- Source placement differs. PUDU chills the DNA tubes and leaves competent cells in a passive tube rack. Lab preserves the assembly-product handoff and stages competent cells at the Method's required 4 °C setpoint.
- PUDU forces `water_testing=True` whenever Opentrons reports simulation, so the simulator skips both thermal profiles. Lab simulates the configured thermal path and opens the thermocycler for the reviewed handoff. Thermal intent is therefore compared from the resolved PUDU constructor and Lab manifest, not inferred from PUDU's empty simulated thermal trace.
- PUDU passes `block_max_volume=30` to both profiles even though the reactions contain 35 µL during heat shock and 95 µL during recovery. Lab derives 35 µL and 95 µL from the actual transfers. The audit reports this upstream inconsistency rather than copying it into Lab.

## Acceptance boundary

Software equivalence is not physical qualification. The emitted protocol is simulator-compatible for the exact OT-2 hardware tuple, but an operator-reviewed water or inexpensive-surrogate run and a biological acceptance run on the real instrument remain separate gates.
