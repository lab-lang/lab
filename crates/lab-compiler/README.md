# Compiler implementation notes

`crates/lab-compiler/` builds the experimental `labc` compiler and `lab-opt` IR tool. These are compiler-development interfaces; the standard package, planning, and runtime workflow is exposed through the repository's `lab` binary.

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

`PortableLairProgram` owns the Pliron context and verifier-valid `design-intent` module. `refine_methods` consumes a validated `lab_method::MethodRegistry` and returns a `RefinedLairProgram` whose candidate regions preserve exact Procedure parameters, typed material dataflow, and first-class Capability requirements. `planning_problem` projects that IR into a purpose-built global constraint model. `FacilityPlanningSolution::solve` selects Methods and exact resources together. `RefinedLairProgram::allocate` applies that complete solution back to the same stable identities, erases unselected candidates, and returns verifier-valid `allocated-procedure` LAIR. `AllocatedLairProgram::adapter_invocations` is the only production backend projection.

The fixed Protocol dialect and its pre-facility `select_protocol` conversion have been retired. Material linearity is checked over Allocated Procedure SSA before invocation projection. Current adapters therefore cannot recover a biological recipe from source IR, traverse the whole experiment, or select another Method, MaterialLot, offering, Asset, or adapter.

The source tree follows semantic ownership and dependency direction:

- `src/lair/` owns Design, Workflow/Intent, Method, Procedure, Capability, and Allocation dialects; source lowering; Method refinement; planning-problem extraction; solution application; stage contracts; analyses; and textual IR tooling;
- `src/planning/` owns the RDF-independent constraint problem, exact MaterialLot evidence, adapter-binding snapshot, graph-wide solver, immutable adapter-invocation projection, reviewed execution-plan construction, and dependency reporting;
- `src/backend/` owns adapter discovery, operational-profile validation, shared typed views over exact allocated Procedure tasks, and concrete implementations grouped by vendor family;
- `src/artifact/` owns generated files independently of filesystem persistence;
- `lab-runfmt` owns the versioned reviewed documents interpreted by the runtime; and
- `src/bin/labc/` and `src/bin/lab-opt/` own developer-facing command orchestration.

The dependency direction is language model -> LAIR and planning -> adapter invocation -> backend artifacts, with application crates owning filesystems, SBOLInventory loading, and output writes. LAIR and global planning never depend on a concrete robot.

`lab.adapter-catalog.v4` is the machine-readable implementation contract. Each stable adapter ID declares implementation features and private configuration schema. Its versioned Procedure implementations separately declare a stable implementation IRI, exact Procedure contract and operation set, required capability kinds, accepted control modes, run-document formats, and truthful planning, lowering, simulation, and runtime services. Broad adapter capability declarations are a compatibility surface for operations that have not yet been normalized and are not authority for a normalized Procedure program. A driver is selected only by an explicit manifest binding to an exact Asset IRI, never by manufacturer or model inference.

`AdapterInvocationPlan` freezes every selected Method and Procedure task, exact parameter and material value, requirement-to-offering-to-Asset binding, adapter identity, operational-profile digest, inventory digest, and allocated-LAIR digest. Invocations group only the tasks and requirements assigned to one exact Asset and adapter. The current built-in automation contract requires one exact allocated requirement to own each independently lowered task; a future coordinated adapter must introduce an explicit multi-requirement contract instead of weakening that invariant.

OT-2, Flex, and STAR lower through the same invocation boundary. Shared Procedure views validate stable operation and parameter identities, canonical QUDT units, material roles, exact selected material sources, capability kinds, and adapter capacity. Device-specific deck allocation, run-document construction, protocol rendering, and implementation constraints remain within the concrete adapter. Manual work and offerings without lowering services remain present in the semantic Procedure and reviewed facility plan without being misrepresented as device output.

`labc --emit` exposes source AST, checked module IR, `design-intent` LAIR, `refined-alternatives` LAIR, or the global planning problem for one self-contained source file. It deliberately has no device or adapter flag. `lab-opt` parses, verifies, transforms, and prints textual LAIR without acting as another source frontend. Multi-module package compilation, inventory selection, global allocation, adapter invocation, artifact persistence, and runtime plans belong to `lab` through the shared `lab-project` service.

For the complete package path, exact SBOLInventory facility, adapter binding, allocated Procedure evidence, and emitted OT-2 protocols, see the [Golden Gate example](../../examples/golden-gate/README.md).

Generated Opentrons protocols can be checked with the optional official simulator when `LAB_OPENTRONS_SIMULATOR` points at its executable:

```sh
uv venv .lab/opentrons-venv --python 3.12
uv pip install --python .lab/opentrons-venv/bin/python 'opentrons>=8.4.1,<9'
LAB_OPENTRONS_SIMULATOR=.lab/opentrons-venv/bin/opentrons_simulate \
  cargo test -p lab-compiler --test opentrons_build
```
