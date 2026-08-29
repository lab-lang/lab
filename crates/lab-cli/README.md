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

`lab.toml` anchors package identity and the build entry. Source modules are discovered recursively under `src/` and receive stable names from their package and relative path. Same-package imports and recursive path dependencies are compiled through checked module interfaces. Every `lab build` writes verified portable module IR, `compiler/refined.lair`, `compiler/planning-problem.json`, a package index, and a deterministic `lab.lock`. When the runnable package selects an SBOLInventory document, the same build solves Methods and exact facility resources together, applies the result to `compiler/allocated.lair`, projects `compiler/adapter-invocations.json`, invokes only adapters selected through exact Asset bindings, emits their protocols and PDFs, and freezes everything in `.lab/build/plan.execution.json`. The package index's optional `facility` section records relative paths to the solution, invocations, lowering manifest, reviewed plan, Asset bundles, protocols, and PDFs.

`lab plan` exposes that facility phase as a separate command, writing it under `.lab/plan/` without the portable module bundle. It requires an SBOLInventory document, applies the package's exact facility selector, selects each unpinned Method together with exact MaterialLots, CapabilityOfferings, Assets, and adapters, and writes the refined LAIR, planning problem, facility solution, Allocated Procedure LAIR, adapter invocations, and validated execution plan. Candidate ordering never chooses a Method or resource: zero solutions is an explained failure, and several equal solutions require explicit policy. A planning-only or manual facility needs no adapter declaration. When an allocated Asset has a compatible lowering adapter, both `lab build` and `lab plan` emit its exact-task documents and freeze the driver, profile, Requirement, offering, Asset, child path, format, and digest in the reviewed plan.

A `lab.toml` may instead declare a workspace, grouping member packages under one root:

```toml
[workspace]
members = ["packages/catalog", "packages/device"]
default-member = "packages/device"
```

A workspace root owns membership and nothing else; each member stays an ordinary package. `default-member` names the package a single-package command acts on, and is required once a workspace has more than one member.

## Facility-derived lowering

The portable frontend of `lab build` never contains an instrument selector: `[build]` names only the experiment entry, and the CLI has no `--target` mode. When the package names a facility document, the build derives device choice by matching the experiment's capability requirements against that validated facility. With no facility document, the same command stops after portable compilation.

An `[inventory]` table selects the SBOLInventory graph. Local adapter declarations connect exact Asset IRIs in that graph to installed Lab implementations:

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

The facility is the lowering surface. The facility phase shared by `lab build` and `lab plan` resolves exact MaterialLots, selects Methods, allocates requirements to CapabilityOfferings and their owning Assets, and invokes only the adapters attached to those selected Assets. The reviewed plan freezes the inventory, compiler evidence, staged adapter profiles, and every emitted device and support artifact by SHA-256. Each independently executable child document is projected from one exact allocated Procedure task and names the Requirement it realizes.

```sh
lab build
lab run .lab/build --dry-run
```

`lab adapters describe` is the discovery authority for the exact compiler binary. Its `lab.adapter-catalog.v3` output keeps semantic SBOLInventory capability IRIs separate from implementation features and declares accepted control modes, run-document formats, configuration schemas, and actual planning, lowering, simulation, and runtime services. The explicit driver argument selects validation code; neither an adapter profile nor an Asset's manufacturer or model can select another implementation. `lab.adapter-profile.v2` contains no backend or Asset selector, rejects the removed `[target]` table, and places OT-2 API-version configuration under `[protocol]`.

```sh
lab adapters describe
lab --json adapters describe --driver opentrons.flex
lab adapters default opentrons.flex --name flex-bay-1
lab adapters validate opentrons.flex adapters/flex-bay-1.toml
lab --json adapters render opentrons.flex adapters/flex-bay-1.toml
```

All read-oriented commands support `--json` for editor and automation clients:

```sh
lab metadata --json
lab check --json
```

Path dependencies may optionally carry a semver requirement, which is checked against the dependency manifest. Registry dependencies remain explicitly unsupported and fail closed; adding them requires a registry protocol and integrity model rather than silent fallback.

`lab run <reviewed-plan-directory> --dry-run` validates the frozen inventory, exact bindings, adapter profiles, every reviewed child artifact, and the dependency DAG, then narrates every node without touching hardware. `lab run <reviewed-plan-directory> --simulate` walks the same exact plan through simulation adapters and writes `inventory-simulation.ttl`; `lab run <reviewed-plan-directory>` uses live executors and writes `inventory-after.ttl`. Both modes append a durable node ledger so `--resume` can continue without repeating completed work, but simulation and live ledgers are intentionally incompatible. Material movements and manual nodes remain explicit operator confirmations in either mode.
