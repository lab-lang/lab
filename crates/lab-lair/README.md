# LAIR implementation notes

`crates/lab-lair/` owns Lab's aggregate intermediate representation and builds the experimental `labc` compiler and `lab-opt` IR tool. These are compiler-development interfaces; the standard package, planning, and runtime workflow is exposed through the repository's `lab` binary.

Lab is a multi-layer IR compiler. `CheckedModule` is the portable frontend boundary, verified LAIR is the mutable transformation boundary, the facility planning problem is the global constraint boundary, and immutable adapter-invocation and reviewed-plan documents are the implementation and runtime boundaries. A backend cannot consume a checked source module or unresolved Method alternatives.

The canonical facility-aware pipeline is:

```text
checked modules
    -> Design and Intent LAIR
    -> Method alternatives containing Procedure tasks and Capability requirements
    -> one graph-wide Method, MaterialLot, offering, Asset, and adapter solution
    -> Allocated Procedure LAIR
    -> immutable adapter invocations
    -> independently reviewed device and operator documents
    -> one facility-wide execution plan
```

`PortableLairProgram` owns the Pliron context and verifier-valid `design-intent` module. `refine_methods` consumes a validated `lab_lair::method::MethodRegistry` and returns a `RefinedLairProgram` whose candidate regions preserve exact Procedure parameters, typed material dataflow, and first-class Capability requirements. `planning_problem` projects that IR into a purpose-built global constraint model. `FacilityPlanningSolution::solve` selects Methods and exact resources together. `RefinedLairProgram::allocate` applies that complete solution back to the same stable identities, erases unselected candidates, and returns verifier-valid `allocated-procedure` LAIR. `AllocatedLairProgram::adapter_invocations` is the only production backend projection.

Verifier-valid Allocated Procedure LAIR is the only input device lowering is projected from. Material linearity is checked over Allocated Procedure SSA before invocation projection. Current adapters therefore cannot recover a biological recipe from source IR, traverse the whole experiment, or select another Method, MaterialLot, offering, Asset, or adapter.

The source tree follows semantic ownership and dependency direction:

- `src/method/` owns portable Method definitions and registries together with `method.choice`, bundled methods, and the refinement pass that constructs candidate regions;
- `src/procedure/` owns `procedure.task` and its typed ports together with canonical pipetting and thermal bodies, task normalization, validation, capability derivation, exact quantities, and whole-program material-linearity analysis;
- `src/design/` owns reusable biological design identities and their LAIR operations;
- `src/workflow/` owns method-neutral Workflow/Intent operations;
- `src/lowering.rs` translates checked Lab modules into the coupled Design and Workflow portions of a LAIR program;
- `src/capability/` owns Capability requirement operations and exact scalar attribute codecs;
- `src/allocation/` owns exact binding operations and application of complete facility decisions to LAIR;
- `src/stage/` owns the explicit stage marker and whole-module structural contracts;
- `src/ir/` owns the small set of Pliron attribute helpers shared across domain operations;
- `src/lair/` temporarily owns only the program wrapper, pass pipeline, and compiler session while those core modules are flattened;
- `src/planning/` owns planning-problem extraction, the RDF-independent constraint problem, exact MaterialLot evidence, adapter-binding snapshot, graph-wide solver, immutable adapter-invocation projection, reviewed execution-plan construction, and dependency reporting;
- `src/backend/` owns adapter discovery, operational-profile validation, shared typed views over exact allocated Procedure tasks, and concrete implementations grouped by vendor family;
- `src/artifact/` owns generated files independently of filesystem persistence;
- `lab-runfmt` owns the versioned reviewed documents interpreted by the runtime; and
- `src/bin/labc/` and `src/bin/lab-opt/` own developer-facing command orchestration.

The dependency direction is language model -> LAIR and planning -> adapter invocation -> backend artifacts, with application crates owning filesystems, SBOLInventory loading, and output writes. LAIR and global planning never depend on a concrete robot.

`lab.adapter-catalog.v2` is the machine-readable implementation contract. Each stable adapter ID declares implementation features and private configuration schema. Its versioned Procedure implementations separately declare a stable implementation IRI, exact Procedure contract and operation set, required capability kinds, accepted control modes, run-document formats, and truthful planning, lowering, simulation, and runtime services. Broad adapter capability declarations are a compatibility surface for operations that have not yet been normalized and are not authority for a normalized Procedure program. A driver is selected only by an explicit manifest binding to an exact Asset IRI, never by manufacturer or model inference.

`AdapterInvocationPlan` freezes every selected Method and Procedure task, canonical program, exact parameter and material value, requirement-to-offering-to-Asset binding, Procedure implementation, adapter identity, operational-profile digest, inventory digest, and allocated-LAIR digest. Invocations group only the tasks and requirements assigned to one exact Asset and adapter. One independently lowered task owns a non-empty requirement set; a canonical program may require that complete set to bind atomically to one Asset, adapter, and Procedure implementation.

OT-2, Flex, and STAR lower through the same invocation boundary. Shared Procedure views validate stable operation and parameter identities, canonical QUDT units, material roles, exact selected material sources, capability kinds, and adapter capacity. Device-specific deck allocation, run-document construction, protocol rendering, and implementation constraints remain within the concrete adapter. Manual work and offerings without lowering services remain present in the semantic Procedure and reviewed facility plan without being misrepresented as device output.

`labc --emit` exposes source AST, checked module IR, `design-intent` LAIR, `refined-alternatives` LAIR, or the global planning problem for one self-contained source file. It deliberately has no device or adapter flag. `lab-opt` parses, verifies, transforms, and prints textual LAIR without acting as another source frontend. Multi-module package compilation, inventory selection, global allocation, adapter invocation, artifact persistence, and runtime plans belong to `lab` through the shared `lab-project` service.

For the complete package path, exact SBOLInventory facility, adapter binding, allocated Procedure evidence, and emitted OT-2 protocols, see the [Golden Gate example](../../examples/golden-gate/README.md).

Generated Opentrons protocols lint and type-check with `scripts/check-opentrons-bundle.sh <bundle>`.
To run them under the official simulator, build it once and pass it to `scripts/simulate-opentrons.sh`:

```sh
uv venv .lab/opentrons-venv --python 3.12
uv pip install --python .lab/opentrons-venv/bin/python 'opentrons>=8.4.1,<9'
scripts/simulate-opentrons.sh examples/golden-gate/.lab/build/assets/opentrons_ot2 \
  .lab/opentrons-venv/bin/opentrons_simulate
```
