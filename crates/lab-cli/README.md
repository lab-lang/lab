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

`lab.toml` anchors package identity and the build entry. Source modules are discovered recursively under `src/` and receive stable names from their package and relative path. Same-package imports and recursive path dependencies are compiled through checked module interfaces. `lab build` writes verified portable module IR, `capability_requirements.json`, and a package index under `.lab/build/`, plus a deterministic `lab.lock` at the project root. The requirement file describes checked workflow templates and contains no facility allocation.

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

The target's artifacts are written under `.lab/build/<name>/`, and the build prints the path of every runnable automation protocol it emitted, ready to hand to an instrument application. A target profile describes the laboratory — modules, labware, deck slots, pipettes, mounts, and capacity — and never the science; reaction chemistry belongs to the designs in `src/`. Every profile field defaults to the backend's reference bench, so a profile states only what differs, and unknown keys are rejected rather than ignored. A profile's filename is its name; the file itself declares only which backend consumes it.

Editors and control planes use the compiler-owned target contract rather than copying backend structs. It reports each backend's JSON Schema, complete default, catalog choices, capabilities, and workcell station kinds; validation runs the same cross-field semantics as a build and returns canonical TOML, canonical JSON, the compiler and schema versions, and a SHA-256:

```sh
lab targets describe
lab --json targets describe
lab targets default opentrons.flex --name flex-bay-1
lab targets validate targets/flex-bay-1.toml
lab --json targets render targets/flex-bay-1.toml
```

The shipped backends are `opentrons.ot2`, `opentrons.flex`, `hamilton.star`, and `workcell`. `describe` is the discovery authority for the exact compiler binary in use; consumers should not assume that list remains fixed.

`lab adapters describe` is the facility-facing discovery authority. Its `lab.adapter-catalog.v1` output keeps semantic SBOLInventory capability IRIs separate from implementation features and declares accepted control modes, run-document formats, configuration schemas, and actual planning, simulation, and runtime support. `lab adapters validate <driver> <profile>` selects the parser from the explicit driver binding; manufacturer and model never select code.

A package that usually compiles for one bench names it in the manifest instead of on every invocation:

```toml
[build]
entry = "src/programs/main.lab"
target = "opentrons-ot2"
```

`lab build` then produces that bench's protocols, `--target <name>` compiles for a different one, and `--no-target` stops at portable module IR.

An `[inventory]` table selects a validated SBOLInventory facility graph, and a target build resolves every artifact dependency against exact active MaterialLots:

```toml
[inventory]
document = "inventory/facility.ttl"
# Required when the document has several facilities:
facility = "https://example.org/facilities/example-lab"

[[execution.adapters]]
asset = "https://example.org/facilities/example-lab/star-1"
driver = "hamilton.star"
profile = "adapters/star-1.toml"
```

Each adapter declaration binds an implementation to one exact catalog Asset. Facility facts remain in RDF, driver selection is never inferred from product metadata, and endpoints and credentials remain local runtime configuration. The old symbolic `materials` and `artifacts` arrays are accepted only as a mutually exclusive migration form.

All read-oriented commands support `--json` for editor and automation clients:

```sh
lab metadata --json
lab check --json
```

Path dependencies may optionally carry a semver requirement, which is checked against the dependency manifest. Registry dependencies remain explicitly unsupported and fail closed; adding them requires a registry protocol and integrity model rather than silent fallback.

`lab run <run-directory> --dry-run` validates and narrates reviewed Hamilton STAR or workcell run documents without touching hardware. A live workcell run connects the supported stations, confirms every handoff with the operator, and appends a durable node ledger so `--resume` can continue without repeating completed motion.
