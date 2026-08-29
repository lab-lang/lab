# 0006: Affine material flow in portable workflows

Status: accepted, initial implementation

Every physical `Material<T>` has one owning place in a workflow. Ownership is verified after name and type checking, using the `copy`, `borrow`, and `take` modes supplied by resolved action contracts.

The analysis is place-sensitive. Taking `colony_result.plate` invalidates that physical projection without preventing later access to immutable `colony_result.observations`. Actions consume their `take` operands, preserve their `borrow` operands, and introduce ownership for material results. Pure bindings move physical values and may not bind one physical value to multiple names.

Every terminating control-flow path must transfer, return, store, or dispose all materials it owns. Continuing branches must agree on their owned material places. Reactive handlers begin from the same captured ownership state; a non-terminating invocation must preserve it for later events.

This frontend analysis complements the existing SSA material-linearity analysis in the current method-selected Protocol IR. Under [0045](0045-lair-method-refinement-and-facility-allocation.md), the frontend pass reasons about source workflow control flow before refinement, refined-alternatives LAIR verifies material use within every candidate region, and allocated-procedure LAIR verifies the selected concrete SSA consumers before adapter invocation.

Loops over collections containing materials are rejected for now. They require an explicit consuming iterator contract that defines ownership for zero, partial, completed, and early-return iteration.
