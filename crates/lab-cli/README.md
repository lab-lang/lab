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

`lab.toml` anchors package identity and the build entry. Source modules are discovered recursively under `src/` and receive stable names from their package and relative path. Same-package imports and recursive path dependencies are compiled through checked module interfaces. `lab build` writes verified portable module IR, `capability_requirements.json`, `capability_instances.json` for runnable packages, an optional `adapter_bindings.json`, and a package index under `.lab/build/`, plus a deterministic `lab.lock` at the project root. The requirement file describes every checked workflow template and contains no facility allocation. The instance file expands only templates reachable from the exact entry module's `main` workflow, preserves every resolved workflow call site, and rejects recursive expansion rather than inventing a finite run. The adapter-binding file freezes exact Asset, compatible CapabilityOffering, profile-hash, qualification, control-mode, and service-eligibility facts but does not allocate a workflow requirement.

`lab plan` requires an SBOLInventory document, applies the package's exact facility selector, allocates every reachable capability instance to one exact offering and Asset, and writes `facility_allocation.json` plus a validated `plan.execution.json` under `.lab/plan/`. Candidate ordering never chooses an asset: zero eligible offerings is an explained failure, and several eligible offerings require an explicit allocation policy. A planning-only or manual facility needs no adapter declaration. When an allocated Asset has a compatible lowering adapter, `lab plan` emits its protocols and freezes its exact driver, profile, triggering requirements, child paths, formats, and digests in the reviewed plan.

A `lab.toml` may instead declare a workspace, grouping member packages under one root:

```toml
[workspace]
members = ["packages/catalog", "packages/device"]
default-member = "packages/device"
```

A workspace root owns membership and nothing else; each member stays an ordinary package. `default-member` names the package a single-package command acts on, and is required once a workspace has more than one member.

## Facility-derived lowering

`lab build` is always portable. It has no instrument selector: `[build]` names only the experiment entry, and the CLI has no `--target` mode. Device choice begins only after `lab plan` has matched the experiment's capability requirements against one validated facility.

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

The facility is the lowering surface. `lab plan` resolves exact MaterialLots, allocates requirements to CapabilityOfferings and their owning Assets, and invokes only the adapters attached to those selected Assets. The reviewed plan freezes the inventory, allocation, staged adapter profiles, and every emitted device and support artifact by SHA-256. Whole-program adapters produce one reviewed lowering bundle covering all of their triggering requirements; Lab does not pretend that one generated protocol corresponds to one arbitrary requirement.

```sh
lab plan
lab run .lab/plan --dry-run
```

`lab adapters describe` is the discovery authority for the exact compiler binary. Its `lab.adapter-catalog.v1` output keeps semantic SBOLInventory capability IRIs separate from implementation features and declares accepted control modes, emitted document formats, configuration schemas, and actual planning, lowering, simulation, and runtime services. The explicit driver argument selects validation code; neither an adapter profile nor an Asset's manufacturer or model can select another implementation.

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
