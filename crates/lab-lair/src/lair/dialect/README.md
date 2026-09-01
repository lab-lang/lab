# LAIR

LAIR, the Lab Automation Intermediate Representation, is Lab's multi-layer compiler IR. It is implemented as a family of Pliron dialects that preserve biological and physical meaning while a program is progressively lowered from scientific intent toward allocated laboratory work. [Decision 0045](../../../../../docs/language/decisions/0045-lair-method-refinement-and-facility-allocation.md) establishes the stage architecture, and [Decision 0046](../../../../../docs/language/decisions/0046-allocated-procedure-is-the-device-boundary.md) makes Allocated Procedure the only production device-lowering boundary.

Pliron is the structural substrate of `lab-lair`: its operations, SSA values, regions, and stage markers carry the graph-scale semantics of a laboratory program. Method and Procedure bodies are complementary parts of that aggregate rather than a separate Pliron-free model. Stage wrappers own their `Context`, `ModuleOp`, and analyses together, while `lab-opt` remains the dynamic textual tool for compiler development.

## Implemented dialects

- `design` represents reusable DNA sequences, plasmids, strains, and declarative design identity;
- `workflow` represents method-neutral realization, provision, transformation, recovery, dilution, and plating intent with typed SSA material edges;
- `method` represents alternative refinements for one exact Intent operation;
- `procedure` represents generic tasks, typed value ports, exact operation parameters, material inputs, and material-state transitions;
- `capability` represents requirements and typed offering constraints owned by Procedure tasks;
- `allocation` represents the selected Method and exact offering, Asset, adapter, and MaterialLot bindings; and
- `meta` records the explicit stage contract on the module.

Procedure operations and material states use open absolute IRIs while the structural Pliron vocabulary remains small and closed. Adding a portable Method or Capability therefore does not require adding another operation class to the compiler. Local task, requirement, port, parameter, and material identities remain stable across refinement, planning, allocation, adapter projection, and reviewed plans.

## Executable stages

The canonical verified pipeline has three production stages:

```text
design-intent
    -> refined-alternatives
    -> allocated-procedure
```

`design-intent` contains Design values and method-neutral Workflow/Intent operations. `PortableLairProgram` is the owned wrapper for this stage.

`refined-alternatives` eliminates every refinable Intent action in favor of `method.choice` regions. Each candidate contains verifier-valid Procedure dataflow and first-class Capability requirements, and every candidate for one choice yields a compatible typed signature. Registered domain operations also carry a validated canonical Procedure program directly on their task operation. `RefinedLairProgram` owns this stage. A read-only analysis projects it into `lab.planning-problem.v1`; the solver never mutates LAIR.

`allocated-procedure` contains one selected Method for every choice, every selected Procedure task and parameter, all Capability requirements, one exact binding for every requirement, one exact source for every material input, and one allocation context identifying the facility and source inventory digest. It contains no `method.choice`, Workflow action, or unresolved candidate. `AllocatedLairProgram` owns this stage and re-runs whole-module material-linearity analysis before exposing immutable adapter invocations.

Stage identity is explicit `lair.stage` metadata plus a structural verifier contract. Merely counting dialect operations cannot establish a stage. Operation verifiers enforce local types and attributes; stage verification enforces graph completeness and identity relationships; analyses enforce non-local invariants such as affine material use.

## Planning is not a dialect

The global planning problem is a purpose-built serializable constraint representation extracted from verified `refined-alternatives` LAIR. It carries Method choices, Procedure tasks, requirements, typed parameters, material alternatives, and stable ancestry without Pliron objects. The solver combines that problem with one validated immutable SBOLInventory snapshot, exact MaterialLot evidence, configured adapter bindings, and explicit policy.

The solver returns a complete `FacilityPlanningSolution` keyed to the exact identities in the planning problem. The allocation pass validates the solution against the problem before applying it. Candidate order is deterministic for review but never chooses a Method or facility resource.

SBOLInventory is not imported into LAIR. Facility, Zone, Asset, CapabilityOffering, and MaterialLot remain RDF model objects owned by `sbol-inventory`; allocated LAIR carries only selected IRIs and provenance digests.

## Adapter and runtime boundary

`lab.adapter-invocations.v1` is projected only from verifier-valid Allocated Procedure LAIR. It freezes selected Method graphs including exact input/output/yield edges, typed tasks and normalized programs, exact Procedure implementation identities, parameters, exact requirement-to-offering-to-Asset bindings, exact material sources, adapter/profile bindings, and the inventory, planning-problem, and allocated-LAIR digests. External code consumes these owned serializable records, never the Pliron module.

The built-in OT-2, Flex, and STAR adapters lower exact assigned Procedure tasks. Device-specific planning may introduce private typed plans or dialects, but it cannot revisit Method selection or facility allocation. Versioned execution plans and child run documents are runtime ABIs derived from those invocations, not later LAIR stages.

## Physical-resource rule

Values representing physical material are affine: they may have at most one consuming use. Information values such as designs and evidence may be reused. An explicit Procedure operation must represent splitting, sampling, transfer, or another physical transition that creates separately owned outputs.

Operation verifiers check only local material-state compatibility. `MaterialLinearityAnalysis` follows SSA uses across the complete Allocated Procedure module, and the `check-material-linearity` pass makes that analysis available to the textual pipeline. Adapter invocation projection cannot proceed if the allocated graph violates this invariant.
