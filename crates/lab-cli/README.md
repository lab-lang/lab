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
lab build --target bench-ot2
```

The target's artifacts are written under `.lab/build/<name>/`, and the build prints the path of every runnable robot protocol it emitted, ready to hand to a robot application. A target profile describes the laboratory — modules, labware, deck slots, pipettes, mounts, and capacity — and never the science; reaction chemistry belongs to the designs in `src/`. Every profile field defaults to the backend's reference bench, so a profile states only what differs, and unknown keys are rejected rather than ignored.

An optional `[build] inventory` path names the JSON inventory a target build resolves artifact dependencies against.

All read-oriented commands support `--json` for editor and automation clients:

```sh
lab metadata --json
lab check --json
```

Path dependencies may optionally carry a semver requirement, which is checked against the dependency manifest. Registry dependencies remain explicitly unsupported and fail closed; adding them requires a registry protocol and integrity model rather than silent fallback. Workflow execution commands will be added only when the durable runtime has real run semantics.
