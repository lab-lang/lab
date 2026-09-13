# Contribute through one integration path

These examples are part of the workspace's tests. Run commands from the repository root. The broader [extension guide](../../docs/contributing/extensions.md) explains ownership, contracts, installation, and current limits.

## Describe science and use it from Python

`scientific-package/src/science.lab` declares a `homogenize` action that consumes and returns the same physical material lineage. [`methods/homogenize.py`](scientific-package/methods/homogenize.py) uses `from lab.procedures import pipetting as p` to author a 30 µL transfer followed by three 10 µL mixes on one fluid path, then writes `methods/homogenize.json`. The volumes describe this example's preparation. The [Python Procedure guide](../../docs/contributing/python-procedures.md) explains the complete API and its validation boundary.

```sh
crates/lab-python/.venv/bin/python examples/contributing/scientific-package/methods/homogenize.py
cargo run --locked -p lab-cli -- check examples/contributing/scientific-package
cargo run --locked -p lab-cli -- bindings python examples/contributing/scientific-package
PYTHONPATH=examples/contributing/scientific-package/bindings/python \
  crates/lab-python/.venv/bin/python examples/contributing/scientific-package/protocol.py
```

The last command prints `Package action compiled and refined through its pipetting Method.` It imports the generated action, writes a Python workflow, and refines it against the package's checked interfaces and Method catalog. It stops before facility allocation because this portable package contains no inventory or instrument binding. The SDK must first be installed with `cd crates/lab-python && uv sync --locked --all-groups`.

The generated `.py` and `.pyi` files are checked in for inspection. Regeneration is checked by the CLI test, and SDK tests exercise runtime compilation, generic inference, record constructor result types, and rejection of incompatible material subjects.

## Encode a pipetting algorithm in Rust

`pipetting-extension/src/lib.rs` expresses that same Method implementation using the public `ProcedureProgramBuildContext` and `PipettingBuilder`. It is a standalone crate with no access to compiler internals. The project integration test changes only the Method execution selector from `template` to `builder`, composes its registration, and compares the decoded programs and derived requirements. It also checks missing registration and an invalid volume unit.

```sh
cargo test --locked -p lab-project --test contributing
cargo test --locked -p lab-compiler --test pipetting_authoring
```

For a reusable algorithm, see `crates/lab-compiler/src/procedure/pipetting/dilution.rs`. Its input volume, material identity, and dilution settings are explicit; its caller owns recipe-specific preparation assumptions.

## Contribute a Hamilton liquid-class library

`hamilton-profile.toml` is a complete profile contribution selecting a versioned library and an explicit liquid category. Its curves are transcribed reference data for testing the integration. The profile's 17 µL/s aspiration speed is intentionally different so tests can prove the selected class reaches the output.

```sh
cargo test --locked -p lab-cli --test project_workflow \
  a_facility_can_lower_exact_requirements_through_several_assets
```

This compiles the existing Golden Gate package against STAR and ODTC assets, verifies the contributed library/classes in invocation manifests, and checks class digests in emitted STAR runs. Adapter unit tests additionally exercise strict per-material classification, transfer/mix propagation, and rejection of unclassified mixtures. Applying a real liquid-class contribution requires its own measured calibration and compatible labware geometry.

## Implement another adapter

`preview-adapter/src/lib.rs` is a complete minimal example with one Lab dependency, `lab-adapter-api`. It owns its profile validator, descriptor, feasibility check, and file lowerer. The conformance test supplies complete allocated assembly, transformation, and dilution tasks and checks that emitted programs and material bindings are preserved exactly.

The preview format is an inspection artifact with no hardware executor. A device adapter replaces the file emission with device-specific lowering, narrows feature declarations to what it implements, and tests those device semantics. Runtime support is optional and is registered separately alongside the same adapter registration.

To start a production crate, use `lab new adapter my-handler --driver my.handler`. To install native callbacks into both CLI and SDK, follow the shared application composition and rebuild instructions in the extension guide.
