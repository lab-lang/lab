# 0046: Allocated Procedure is the only device-lowering boundary

## Status

Accepted. Extends [0044: Facility graphs and capability binding replace workcell targets](0044-facility-graphs-replace-workcell-targets.md) and [0045: LAIR represents method alternatives before facility allocation](0045-lair-method-refinement-and-facility-allocation.md). Retires the fixed Protocol dialect and whole-program adapter compatibility path.

## Context

The first automated Golden Gate slice selected one compiler-owned Protocol before facility planning. OT-2, Flex, and STAR then consumed that entire Protocol graph and reconstructed device plans for the whole experiment. Later facility allocation could associate the resulting bundle with triggering requirements, but the backend still saw work beyond any one exact allocation.

That compatibility architecture violated the intended ownership boundary. A whole-program backend could implicitly depend on unallocated source structure, repeat biological planning, ignore a requirement it did not understand, or make one device bundle appear to implement several requirements without proving which document realized which work. It also preserved two executable compiler paths after Method, Procedure, Capability, global planning, and Allocation LAIR were implemented.

## Decision

Verifier-valid `allocated-procedure` LAIR is the only production input from which device lowering may be projected. The fixed Protocol dialect, `MethodSelectedProtocol` stage, pre-facility `select_protocol` conversion, and whole-program backend entry points are removed.

The compiler projects one immutable `AdapterInvocationPlan` from Allocated Procedure LAIR. That record freezes:

- the planning-problem, allocated-LAIR, and inventory digests;
- the selected Facility;
- every selected Method and its exact source operation;
- every selected Procedure task, value edge, parameter, material input, and output;
- every Requirement-to-CapabilityOffering-to-Asset binding and observed qualification and control mode;
- every exact MaterialLot or selected Method output used as a material source; and
- every explicit adapter ID, profile path, profile digest, feature set, and run-document contract.

Invocations group tasks and requirements only by one exact Asset and one exact adapter binding. A backend receives its invocation plus the immutable plan so it can resolve stable references, but it may read and lower only the tasks and requirements named by that invocation. It may not inspect checked source, unresolved Method candidates, the facility RDF graph, or another Asset's tasks.

The current independently executable automation contract requires each lowered Procedure task to have exactly one allocated Requirement owned by its invocation. Each emitted reviewed child document names that Requirement. If a future device must coordinate several capabilities atomically, it must introduce a versioned multi-requirement invocation contract that states the shared semantics explicitly; it cannot treat whole-program visibility as implicit coordination.

Shared Procedure views validate semantic operation IRIs, capability kinds, parameter identities and types, canonical units, material roles, selected material sources, and exact allocation ownership before a concrete adapter performs device-specific resource planning. An adapter rejects any semantic value it cannot preserve.

Manual tasks and requirements assigned to offerings without a lowering service remain in the selected Method graph and reviewed execution plan. The absence of a device artifact does not erase the work, and the presence of a lowerer does not promote the offering's SBOLInventory qualification.

`labc` remains a compiler-inspection tool and has no device-selection or adapter-lowering mode. Package compilation, inventory loading, allocation, adapter invocation, artifact persistence, and reviewed-plan construction go through the shared `lab-project` application service used by the CLI and Python bindings.

## Consequences

- There is one semantic path from source and Python frontends to device work.
- Method choice and facility allocation are complete and verifier-checked before a backend runs.
- Every generated device document is attributable to an exact Procedure task, Requirement, offering, Asset, adapter profile, material binding, and source inventory.
- OT-2, Flex, and STAR share one public invocation architecture while retaining their real device-specific constraints and formats.
- Backends cannot silently recover a biological recipe from another representation or select resources outside the reviewed solution.
- Multi-device composition is the composition of facility-allocated Procedure tasks and explicit material dependencies, not a workcell target or a whole-program backend.
- The immutable invocation schema, not Pliron or an adapter-specific recipe AST, is the extension boundary for non-compiler consumers.
