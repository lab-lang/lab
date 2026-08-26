# `lab` CLI

`lab` is the command line for Lab. It manages projects and packages today and will grow into the operational interface for live laboratory runs.

Install the current CLI from the repository root:

```sh
cargo install --path crates/lab-cli --locked
```

This installs `lab` into Cargo's binary directory, normally `~/.cargo/bin`. Reinstall after local CLI changes with `cargo install --path crates/lab-cli --locked --force`.

The initial project loop is:

```sh
lab new tet-reporter
cd tet-reporter
lab check
lab build
```

`lab.toml` anchors package identity and the build entry. Source modules are discovered recursively under `src/` and receive stable names from their package and relative path. Same-package imports and recursive path dependencies are compiled through checked module interfaces. `lab build` writes verified portable module IR plus a package index under `.lab/build/` and a deterministic `lab.lock` at the project root.

A `lab.toml` may instead declare a workspace, grouping member packages under one root:

```toml
[workspace]
members = ["packages/catalog", "packages/device"]
default-member = "packages/device"
```

A workspace root owns membership and nothing else; each member stays an ordinary package. `default-member` names the package a single-package command acts on, and is required once a workspace has more than one member.

## Building for a bench

`lab build --target <name>` reads `targets/<name>.toml`, lowers the default member and everything it depends on as one program, and hands the verified result to the backend that profile names:

```sh
lab build --target opentrons-ot2
```

The target's artifacts are written under `.lab/build/<name>/`, and the build prints the path of every runnable robot protocol it emitted, ready to hand to a robot application. A target profile describes the laboratory — modules, labware, deck slots, pipettes, mounts, and capacity — and never the science; reaction chemistry belongs to the designs in `src/`. Every profile field defaults to the backend's reference bench, so a profile states only what differs, and unknown keys are rejected rather than ignored. A profile's filename is its name; the file itself declares only which backend consumes it.

Editors and control planes use the compiler-owned target contract rather than copying backend structs. It reports each backend's JSON Schema, complete default, catalog choices, capabilities, and workcell station kinds; validation runs the same cross-field semantics as a build and returns canonical TOML, canonical JSON, the compiler and schema versions, and a SHA-256:

```sh
lab targets describe
lab --json targets describe
lab targets default opentrons.flex --name flex-bay-1
lab targets validate targets/flex-bay-1.toml
lab --json targets render targets/flex-bay-1.toml
```

The shipped backends are `opentrons.ot2`, `opentrons.flex`, `hamilton.star`, and `workcell`. `describe` is the discovery authority for the exact compiler binary in use; consumers should not assume that list remains fixed.

A package that usually compiles for one bench names it in the manifest instead of on every invocation:

```toml
[build]
entry = "src/programs/main.lab"
target = "opentrons-ot2"
```

`lab build` then produces that bench's protocols, `--target <name>` compiles for a different one, and `--no-target` stops at portable module IR.

An `[inventory]` table states what the laboratory has on hand, and a target build resolves every artifact dependency against it:

```toml
[inventory]
materials = ["BsaI", "T4_DNA_ligase", "pSB1C3"]
artifacts = ["composite_plasmid_1"]
```

`materials` are consumables a reaction may draw on; `artifacts` are already realized and are not built again. Both default to empty, so a package that declares no inventory builds everything from nothing and reports what it is missing.

All read-oriented commands support `--json` for editor and automation clients:

```sh
lab metadata --json
lab check --json
```

Remote robot-learning compute is C3-first. A local `.env` may hold
`C3_API_KEY`; it is ignored by Git and read as data rather than sourced as a
shell script. The doctor is read-only: it validates authentication and the
current L40-class catalog without submitting a job.

```sh
lab compute doctor
lab compute list
```

Isaac Sim requires RTX hardware, so the doctor recognizes C3's L40/L40S class
and does not present A100 or H100 capacity as Isaac-compatible. Actual training
commands remain absent until the tracked C3 capability gate and a real PPO
runner have passed.

Path dependencies may optionally carry a semver requirement, which is checked against the dependency manifest. Registry dependencies remain explicitly unsupported and fail closed; adding them requires a registry protocol and integrity model rather than silent fallback. Workflow execution commands will be added only when the durable runtime has real run semantics.
