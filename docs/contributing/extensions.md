# Contributing a focused compiler feature

Lab's extension points follow the abstraction being contributed. A scientific vocabulary package declares types, facets, actions, and workflows. A Method pack explains how an exact action operation becomes device-neutral Procedure tasks. An adapter explains how one versioned Procedure contract runs on one family of instruments. None of these requires adding a biological operation match to the compiler.

Choose the integration path from the work you want to contribute:

| User story | Owned changes | Executable example |
| --- | --- | --- |
| I want to describe new science and call it from Python | Lab package, Method document, generated bindings | [`scientific-package`](../../examples/contributing/scientific-package) |
| I want to describe liquid operations in Python | Typed pipetting template, Method task, generated JSON catalog | [`homogenize.py`](../../examples/contributing/scientific-package/methods/homogenize.py) |
| I want to encode a pipetting algorithm | One Rust builder registration, a Method selecting it, focused program tests | [`pipetting-extension`](../../examples/contributing/pipetting-extension/src/lib.rs) |
| I want to add a Hamilton liquid class | A versioned class library and a STAR operational profile | [`hamilton-profile.toml`](../../examples/contributing/hamilton-profile.toml) |
| I want to lower programs for another instrument | One adapter crate owning its descriptor and callbacks | [`preview-adapter`](../../examples/contributing/preview-adapter/src/lib.rs) |

The examples are built and tested by the workspace and Python SDK checks. [The example guide](../../examples/contributing/README.md) gives commands and expected results. Templates and Rust algorithms produce the same versioned Procedure contract; existing adapters consume that contract without recognizing the scientific operation's name.

## Scientific package and Python bindings

Write and check the package first, then generate the Python surface from its checked interfaces:

```sh
lab check path/to/package
lab bindings python path/to/package
```

The default output is `bindings/python` under the package root. Every checked module produces a runtime `.py` module and a `.pyi` typing stub. The runtime objects retain their exact `(module, local)` definition identity; stubs preserve named generic parameters and bounds, roles, function, action, and workflow inputs, package-qualified nominal types, facet states, constructor results, and exact result arity and types. Python keywords are escaped without changing the underlying Lab name. Before returning any files, the generator checks the complete module for collisions among exports, produced artifact types, facet states, typing helpers, import helpers, and normalized module paths. An ambiguous package is rejected before the CLI writes anything. There is no handwritten Python mirror to keep synchronized.

Dynamic Python design factories such as `lab.buy(..., state=...)` do not currently infer every facet in mypy. Where a factory creates a facet-qualified reference, annotate or cast that reference to its actual `DesignReference[...]` type; the Lab checker still validates the authored facet. Generated action calls and `wf.perform` retain their result types.

The output root also contains `.lab-python-bindings.json`. On the next generation Lab removes only obsolete paths named by that manifest, so a renamed Lab module cannot survive as importable stale Python and handwritten neighbors are never treated as generated files.

The bundled standard library is not privileged. It uses the same renderer:

```sh
lab bindings python std --out-dir crates/lab-python/python/lab
```

When Python code consumes bindings generated from an ordinary package, check the emitted entry against that package's compiled interfaces and name the entry explicitly when refining or planning:

```python
program = lab.check(my_protocol.module, project="path/to/package")
refined = lab.refine(
    program,
    entry_module="my_protocol",
    project="path/to/package",
)
planned = lab.plan(
    program,
    entry_module="my_protocol",
    project="path/to/package",
)
```

The application service contributes the package modules, Methods, Procedures, inventory, and adapters exactly once. An imported generated action or workflow therefore resolves to its checked package definition; Python does not copy or reinterpret that definition.

## Method pack

For pipetting, start with [Author a pipetting Method in Python](python-procedures.md). `from lab.procedures import pipetting as p` provides logical vessel handles, exact quantities, typed parameter references, and transfer, distribution, and mix operations. It emits the existing template contract, so a Method author does not need to handwrite nested JSON or change Rust.

```sh
lab new method-pack liquid-methods
cd liquid-methods
lab check .
```

The scaffold separates Lab vocabulary under `src/` from portable `lab.method-catalog.v2` documents under `methods/`. `lab check` is the conformance entry point: it validates the source interfaces, each Method graph, and the complete registry composed with dependencies and the standard library.

Each Procedure task chooses exactly one execution form. A `template` embeds a contract and JSON body; a `builder` names a registered algorithm and its contract; a `primitive` states capability requirements directly. Use a template when the complete program is data plus checked task values. Its only substitution syntax is an object whose sole key is `$lab`:

```json
{
  "repeats": {"$lab": {"kind": "integer", "id": "cycles"}},
  "input": {"$lab": {"kind": "input", "index": 0}},
  "outputs": [{"$lab": {"kind": "output", "id": "product"}}]
}
```

The closed slot kinds are `intent`, `artifact`, `parameter`, `scalar`, `integer`, `text`, `boolean`, `iri`, `input`, `output`, and `material`. `intent` inserts the complete checked action, including exact declaration identity, structured typed expressions, ownership, lineage, artifact context, and source coordinates. `artifact` inserts the complete owning design and fails when the action is not part of an artifact realization. Catalog loading rejects malformed slots and references to undeclared task values. Refinement resolves the values and checks scalar projections and the rendered Procedure contract before facility planning.

If the Procedure shape itself is new, register one `ProcedureContractRegistration` with `ProcedureCompiler::with_contract`. That analyzer owns decoding, structural validation, capability derivation, feature derivation, and the material interface for the contract. Add a Rust `ProcedureProgramBuilderRegistration` only when the program cannot be expressed as declarative template data; compose it separately with `ProcedureCompiler::with_builder`.

## Contribute a pipetting algorithm

A `ProcedureProgramBuildContext` exposes checked task accessors: `require_io`, `output`, `require_material_roles`, `one_material`, `integer_parameter`, `volume_parameter`, and text/list accessors. A volume must have the canonical microlitre unit, an integer must have the expected unit or be unitless, and role/cardinality mismatches produce errors naming the task. Contributors do not parse compiler-generated identifiers or work with Pliron values.

Use `PipettingBuilder` to declare input, material-source, and product vessels, then author ordered transfers, distributions, and mixes. Vessel handles check position bounds. A `continuous_path` scopes operations sharing a fluid path. `finish()` runs the canonical validator, including exact liquid accounting, retained-volume and capacity constraints, reference integrity, and operation bounds. Adapter feasibility separately checks device limits and supported fluid-path combinations. The full `Vessel` and `PipettingStep` records remain available for features beyond the convenience methods.

The built-in serial dilution now uses this interface. Its reusable `serial_dilution` function accepts `SerialDilutionSettings`, including the actual `input_volume_each`, diluent, transfer and mix volumes, replicates, and stages. The transformation Method's wrapper calculates its recipe's starting volume separately. Tests assert stage/well order, independent replicate paths, final volumes, and rejection of insufficient liquid. A new algorithm uses a new builder ID with the existing pipetting contract; a new contract is needed only for semantics that pipetting cannot represent.

## Contribute Hamilton liquid-handling knowledge

A STAR profile selects a versioned liquid-class library. Each class owns applicability, volume curves, speeds, geometry/technique constraints, and calibration provenance. Profiles bind exact allocated material symbols and logical Procedure-input vessel IDs to that library's liquid categories:

```toml
[liquid_handling.materials]
glycerol = "glycerol_50_percent"

[liquid_handling.inputs]
prepared-mixture = "glycerol_50_percent"
```

An authored `liquid_handling` table without `default_liquid` requires explicit classification. Omitting the whole table retains the bundled aqueous reference behavior. Set a fallback only when it describes the actual loaded liquids. The adapter tracks categories through transfers and mixes within each Procedure, including grouped operations and wells that are emptied and refilled. Combining different categories becomes unclassified; a later aspiration or mix fails instead of guessing a calibrated class. Prepared inputs must be classified explicitly. Automatic mixture classification and propagation of liquid state across Procedures are future work.

The selected library, class identities, versions, content digests, speeds, and calibration provenance appear in the invocation manifest; emitted STAR runs retain the selected class evidence. The contributed-profile regression compiles the Golden Gate workflow through STAR and verifies that its authored classes reach the emitted artifacts. The example uses transcribed reference curves to test integration; it supplies no physical calibration claim.

Geometry is also adapter-owned. STAR currently plans 8-channel STAR/STARlet machines using the vendored carrier and labware catalog (`pcr_plate_96`, `sample_tubes_24`, `trough_60ml`, and the 50/300/1000 µL filter-tip racks). Adding an arbitrary vendor labware file is not a supported profile extension. A new geometry requires a catalog definition, compatible access/volume geometry, and adapter tests. The Opentrons reference profiles declare module-backed source/work areas and a bulk-liquid rack; OT-2 additionally declares a material-surface plate. These are device resources, selected from canonical vessel roles, loads and dispense techniques. Per-position working volumes are checked before lowering. A custom resource profile must provide compatible labware geometry and working volumes. A canonical program's logical positions do not establish that a device can reach or hold them; adapter feasibility must accept the complete program and profile before lowering.

The file adapters emit separately staged Procedure runs. Their manifests retain upstream material references, initial loads, and logical-to-physical locations. Operators must stage those inputs for each run; automatic preservation of physical locations across Procedure files is not implemented. This is a current execution boundary, including for the Golden Gate example.

## Adapter

```sh
lab new adapter acme-handler --driver acme.handler
cd acme-handler
cargo test
```

The generated crate's only Lab dependency is `lab-adapter-api`, returns one complete `lab_adapter_api::AdapterRegistration`, and includes a standalone registry conformance test. `lab-adapter-api` re-exports every Lab-owned type in the named feasibility and lowering callback signatures, together with the canonical pipetting and thermal Procedure models and allocated task records they inspect. An adapter crate does not depend on or import `lab-compiler` or `lab-capability`. Add profile parsing, Procedure feasibility, and artifact lowering inside that crate. Application composition is one line: `registry.with_registration(my_adapter::registration())?`. Scientific operation names do not belong in an adapter registration.

After parsing and semantic checks, a profile validator calls `lab_adapter_api::canonical_adapter_profile(driver, name, &typed_profile)`. This API-owned helper stamps the Lab API version, renders the canonical TOML and JSON, and computes the digest, so an adapter crate cannot accidentally report its own package version as the adapter API version. `AdapterRegistry` independently rechecks every result: the requested driver and profile name must be exact, `canonical_toml` must be the canonical encoding of `canonical_json`, and `sha256` must be the lowercase 64-hex digest of those exact TOML bytes. A lowerer receives only the owned, validated profile and immutable invocation plan, and returns a collision-checked artifact bundle plus exact reviewed-document records. The generated feasibility and lowering placeholders reject work until they are implemented, so an untouched scaffold cannot claim device support.

Reviewed-document loading, simulation, and live hardware execution are an optional second layer in `lab-adapters`. This keeps an adapter that only contributes planning and file lowering free of robot SDKs and runtime dependencies. Runtime-capable applications may attach `RuntimeDocumentRegistration` records with `AdapterRuntimeRegistrationExt`; the runtime validates those records against the same descriptor before use.

## Install a Rust contribution in the CLI and Python SDK

Rust extensions are statically linked. The shared application composition is [`application_extensions`](../../crates/lab-project/src/extensions.rs). Add a path or versioned crate dependency under `[dependencies]` in `crates/lab-project/Cargo.toml`, then add its registration there:

```rust
Ok(ApplicationExtensions {
    adapters: lab_adapters::builtin_adapter_registry()?
        .with_registration(my_adapter::registration()?)?,
    procedures: lab_compiler::procedure::builtin_procedure_compiler()
        .with_builder(my_pipetting::registration())?,
})
```

Both frontends use this composition for adapter discovery/profile validation and Method checking/refinement/planning/lowering. CLI reviewed execution also uses the composed adapter registry. Built-in Procedures own their registrations alongside their domain implementation; built-in adapters own their descriptors, feasibility/lowering callbacks, and optional runtime documents in their device modules. The lists assembling these registrations contain no device-specific callback bodies.

Build and install the changed toolchain from the repository root:

```sh
cargo install --locked --path crates/lab-cli --force
(cd crates/lab-python && uv sync --locked --all-groups --reinstall-package lab-compiler)
```

The latter rebuilds the local editable SDK. For distribution, build a wheel with `maturin build --locked --release --manifest-path crates/lab-python/Cargo.toml` and install that wheel in the consuming environment. Installing a Python package or putting a Rust crate on disk does not register its native callbacks in an already-built compiler. Pure Lab packages and Method templates need no native rebuild; use package dependencies and regenerate their Python bindings.

Embedders can instead call `ProjectCompilation::load_with_extensions(path, adapters, procedures)` with an explicit composition. Duplicate IDs, unknown builders, and contract mismatches are rejected at composition/package validation. This is a build-time integration point, not a dynamic plugin loader.

## Internal machinery

The facility solver and Pliron are deliberately not extension points. Package interfaces, Method documents, Procedure templates, contracts and builders, adapter registrations, and generated Python modules exchange owned data across those internals. [Decision 0055](../language/decisions/0055-solving-and-pliron-are-compiler-internals.md) records what each internal tool owns, what it must not own, and the evidence required to replace it.
