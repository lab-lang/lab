# 0045: LAIR represents method alternatives before facility allocation

## Status

Accepted. Amends [0004: Portable module compilation boundary](0004-portable-module-ir.md), [0011: Artifact dependencies derive from typed material dataflow](0011-dependencies-from-material-dataflow.md), and [0044: Facility graphs and capability binding replace workcell targets](0044-facility-graphs-replace-workcell-targets.md).

## Context

Lab initially used LAIR, a family of Pliron dialects, to lower checked biological designs and method-neutral workflow operations into one preselected Protocol graph. That vertical slice demonstrated typed SSA material flow, operation verification, dialect conversion, whole-module material-linearity analysis, textual IR, and compiler-owned pass pipelines.

The vertical slice fixed one plasmid-build conversion in compiler code. A durable action carried one capability kind, `select_protocol` selected the Golden Gate path without consulting a facility, and facility allocation independently traversed checked workflows to reconstruct requirement instances. That arrangement could not represent several scientifically valid methods for one action, could not allow available facility resources to determine which unpinned method was feasible, and permitted the workflow, LAIR, and planner to disagree about what work was required.

The facility model established by 0044 makes this limitation concrete. An experiment describes scientific intent, while an SBOLInventory graph describes the zones, Assets, capability offerings, and MaterialLots available to realize it. Selecting one method before consulting that facility can reject a feasible experiment or silently privilege the compiler's first implementation.

## Decision

Lab remains a multi-layer IR compiler. Checked portable IR is the typed frontend boundary. LAIR is the compiler's internal, mutable transformation IR. Versioned procedure, adapter-invocation, execution-plan, and run-document records are immutable public boundaries derived from verified LAIR; they do not expose Pliron contexts, pointers, operations, or lifetimes.

LAIR progressively represents four semantic concerns:

```text
Design and Intent
    -> Method alternatives with Procedure and Capability regions
    -> graph-wide method and facility constraint solving
    -> Allocated Procedure
    -> adapter invocation and device lowering
```

The Design dialect represents reusable biological information. A generalized Intent or Workflow dialect represents reachable source actions, calls, control structure, and typed material dataflow without selecting a method or facility resource. The Method dialect represents one or more candidate refinements for an action. Candidate regions contain Procedure tasks and their first-class Capability requirements and must yield compatible result types. The Procedure dialect uses a small closed structural vocabulary with open semantic operation identities, so adding a method or capability does not require defining a new Pliron operation class.

Method refinement is facility-independent, but unpinned method selection is not. Refinement enumerates every applicable method candidate and its requirement graph. Planning constructs one constraint problem over method choices, capability offerings, Assets, adapters, and MaterialLots. Physical locations and device batching are decided after allocation, by the schedule an adapter proposes and the compiler validates, rather than inside this problem. An explicit source or manifest method pin restricts a method variable to one candidate; otherwise the solver selects a method only when the facility and stated policy make one solution unique. Equally valid solutions remain an explained ambiguity, except where the alternatives are interchangeable physical resources, which [0051](0051-interchangeable-resources-resolve-without-a-pin.md) resolves deterministically and records.

The planning problem is a purpose-built constraint representation rather than a Pliron dialect. A read-only LAIR analysis extracts it from verifier-valid method alternatives. The solver returns decisions keyed by stable MethodChoice, Requirement, ProcedureNode, and MaterialInput identities. An allocation pass validates and applies the complete solution to the same LAIR module, erases unselected candidate regions, inserts explicit movements, and records exact `Requirement -> CapabilityOffering -> Asset`, adapter, and MaterialLot bindings. No adapter may accept unresolved method alternatives.

Capability requirements are produced only by method refinement. A source action identifies scientific intent and its typed operands and results; it does not permanently own one capability kind. A primitive method may produce one requirement, a composite method may produce several procedure tasks and requirements, and an offered high-level service may remain a valid alternative primitive method. Requirement extraction never performs an independent checked-AST traversal.

Pliron remains an implementation detail of `lab-compiler`. Production APIs own `Context`, `ModuleOp`, and `AnalysisManager` together behind stage-typed wrappers, and transformations consume one verified stage to produce the next. `lab-opt` retains a dynamic textual session for inspecting, verifying, and transforming LAIR. Textual LAIR is a compiler-development interface until a separate decision gives it a stable external compatibility policy.

SBOLInventory is a target-environment description, not another LAIR dialect. The compiler queries a validated immutable facility snapshot and carries only exact selected resource identities and the source inventory digest into allocated LAIR and reviewed plans. It does not import the facility RDF graph into Pliron or re-model Facility, Zone, Asset, CapabilityOffering, or MaterialLot classes in compiler IR.

Adapters consume versioned immutable invocation records projected from allocated LAIR rather than raw Pliron objects. A built-in adapter may use an adapter-local Pliron dialect internally when device planning benefits from verification and optimization, but the public adapter contract does not require Pliron and external adapters cannot mutate compiler IR.

Reviewed execution plans and device run documents are runtime ABIs, not LAIR stages. They freeze the exact method, offering, Asset, adapter, material, child-document, and source digests a reviewer approved. Runtime interprets those records and writes a ledger and SBOLInventory provenance; it does not run compiler passes or repeat allocation.

## Stage contracts

The intended executable LAIR stages are:

```text
design-intent
refined-alternatives
allocated-procedure
```

`design-intent` contains Design and method-neutral Intent operations. `refined-alternatives` eliminates refinable Intent actions in favor of Method candidate regions containing Procedure and Capability operations. `allocated-procedure` contains no unresolved Method choice and records every exact facility binding required for adapter invocation.

Stage identity is explicit module metadata and a structural verifier contract. Dialect counting may support diagnostics, but it is not sufficient to establish a stage. Operation-local verifiers establish local type and attribute invariants; Pliron analyses establish non-local invariants such as affine material use, method-contract conformance, requirement completeness, and allocation consistency.

## Consequences

- The compiler remains recognizably multi-layer: frontend IR, several LAIR levels, a constraint problem, allocated LAIR, adapter IR where useful, and reviewed runtime formats.
- Facility availability can select among valid methods without putting device identities in Lab source.
- High-level biological actions no longer pretend to map directly to one instrument capability.
- Capability requirements, material flow, source trace, and method ancestry share one compiler representation and cannot drift across independent traversals.
- Pliron supplies SSA, regions, verification, analyses, rewriting, and textual tooling without becoming a public data model or plugin ABI.
- The solver uses a representation suited to global constraint propagation and returns decisions that must be applied back to the exact LAIR identities from which they were derived.
- Public method and adapter extensions use stable declarative records; they do not require downstream crates or Python packages to depend on Pliron.
- The Design and Workflow parts of the original vertical slice remain useful implementation evidence. [0046](0046-allocated-procedure-is-the-device-boundary.md) records the completed removal of the fixed Protocol dialect and makes Allocated Procedure the only production device-lowering boundary.
