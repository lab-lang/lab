# Lab OT-2 Python backend

This directory is the maintained Python implementation of Lab's Opentrons OT-2 backend. Protocol behavior belongs in ordinary Python modules under `src/lab_opentrons_ot2/protocols/`, not in Rust string literals.

The maintained modules import `Ot2ExecutionPlan` unconditionally from `plan_types.py` and are always typechecked. During compiler emission, Rust deterministically replaces that package import with the same marked `TypedDict` definitions and injects the serialized execution plan. The generated protocol therefore remains a standalone file accepted by the robot while preserving the checked source's types and behavior.

The Python and Opentrons versions are pinned in `uv.lock`. Run every Python adapter gate with:

```sh
scripts/check-opentrons-bundle.sh
```

After generating `.lab/full-build`, also lint, typecheck, and byte-compile every emitted protocol:

```sh
scripts/check-opentrons-bundle.sh .lab/full-build
scripts/simulate-opentrons.sh .lab/full-build
```

`plan_types.py` is the Python view of the Rust `Ot2ExecutionPlan` serialization contract. Changes to either side must update the other side and pass both Rust emission tests and the Python checks.
