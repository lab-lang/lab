# Focused compiler extensions

Lab's extension points follow the abstraction being contributed. A scientific vocabulary package declares types, facets, actions, and workflows. A Method pack explains how an exact action operation becomes device-neutral Procedure tasks. An adapter explains how one versioned Procedure contract runs on one family of instruments. None of these requires adding a biological operation match to the compiler.

## Scientific package and Python bindings

Write and check the package first, then generate the Python surface from its checked interfaces:

```sh
lab check path/to/package
lab bindings python path/to/package
```

The default output is `bindings/python` under the package root. Every checked module produces a runtime `.py` module and a `.pyi` typing stub. The runtime objects retain their exact `(module, local)` definition identity; stubs preserve named generic parameters and bounds, roles, function, action, and workflow inputs, package-qualified nominal types, facet states, constructor results, and exact result arity and types. Python keywords are escaped without changing the underlying Lab name. Before returning any files, the generator checks the complete module for collisions among exports, produced artifact types, facet states, typing helpers, import helpers, and normalized module paths. An ambiguous package is rejected before the CLI writes anything. There is no handwritten Python mirror to keep synchronized.

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

```sh
lab new method-pack thermal-methods
cd thermal-methods
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

The closed slot kinds are `intent`, `artifact`, `parameter`, `scalar`, `integer`, `text`, `boolean`, `iri`, `input`, `output`, and `material`. `intent` inserts the complete checked action, including exact declaration identity, structured typed expressions, ownership, lineage, artifact context, and source coordinates. `artifact` inserts the complete owning design and fails when the action is not part of an artifact realization. Unknown references, wrong scalar projections, reserved `$lab` keys, and malformed slots fail during Method refinement, before facility planning. The rendered JSON must then satisfy the named Procedure contract.

If the Procedure shape itself is new, register one `ProcedureContractRegistration` with `ProcedureCompiler::with_contract`. That analyzer owns decoding, structural validation, capability derivation, feature derivation, and the material interface for the contract. Add a Rust `ProcedureProgramBuilderRegistration` only when the program cannot be expressed as declarative template data; compose it separately with `ProcedureCompiler::with_builder`.

## Adapter

```sh
lab new adapter acme-cycler --driver acme.cycler
cd acme-cycler
cargo test
```

The generated crate's only Lab dependency is `lab-adapter-api`, returns one complete `lab_adapter_api::AdapterRegistration`, and includes a standalone registry conformance test. `lab-adapter-api` re-exports every Lab-owned type in the named feasibility and lowering callback signatures, together with the canonical pipetting and thermal Procedure models and allocated task records they inspect. An adapter crate does not depend on or import `lab-compiler` or `lab-capability`. Add profile parsing, Procedure feasibility, and artifact lowering inside that crate. Application composition is one line: `registry.with_registration(my_adapter::registration())?`. Scientific operation names do not belong in an adapter registration.

After parsing and semantic checks, a profile validator calls `lab_adapter_api::canonical_adapter_profile(driver, name, &typed_profile)`. This API-owned helper stamps the Lab API version, renders the canonical TOML and JSON, and computes the digest, so an adapter crate cannot accidentally report its own package version as the adapter API version. `AdapterRegistry` independently rechecks every result: the requested driver and profile name must be exact, `canonical_toml` must be the canonical encoding of `canonical_json`, and `sha256` must be the lowercase 64-hex digest of those exact TOML bytes. A lowerer receives only the owned, validated profile and immutable invocation plan, and returns a collision-checked artifact bundle plus exact reviewed-document records. The generated feasibility and lowering placeholders reject work until they are implemented, so an untouched scaffold cannot claim device support.

Reviewed-document loading, simulation, and live hardware execution are an optional second layer in `lab-adapters`. This keeps an adapter that only contributes planning and file lowering free of robot SDKs and runtime dependencies. Runtime-capable applications may attach `RuntimeDocumentRegistration` records with `AdapterRuntimeRegistrationExt`; the runtime validates those records against the same descriptor before use.

## Internal machinery

The facility solver and Pliron are deliberately not extension points. Package interfaces, Method documents, Procedure templates, contracts and builders, adapter registrations, and generated Python modules exchange owned data across those internals. [Decision 0055](../language/decisions/0055-solving-and-pliron-are-compiler-internals.md) records what each internal tool owns, what it must not own, and the evidence required to replace it.
