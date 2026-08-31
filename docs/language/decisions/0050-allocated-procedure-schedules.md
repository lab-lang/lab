# 0050: Allocated Procedure schedules make device batching explicit

## Status

Accepted. Extends [0046: Allocated Procedure is the only device-lowering boundary](0046-allocated-procedure-is-the-device-boundary.md) without weakening its allocation boundary.

## Context

An `AdapterInvocationPlan` assigns exact Procedure tasks and requirements to one physical Asset and checked adapter, but assignment alone does not determine a safe device run. The first OT-2 vertical slice emitted one protocol per task. Every protocol planned its wells independently, so successive tasks reused addresses such as `A1` without a persistent location record. It also prevented PUDU's important batch behavior: several compatible reactions share one thermal program, transformation stages remain on one thermocycler plate, and serial dilution can flow directly into plating with a deliberate two-tip contamination path.

This behavior cannot live only in a Python template. A template-local fusion would be invisible to compiler validation, would erase the identities of the tasks it combined, and could cause the generic execution plan to execute the same shared document once for every requirement.

## Decision

The compiler defines a versioned `AllocatedProcedureSchedule` between immutable facility allocation and device document emission. A schedule belongs to one exact `AdapterInvocation`, is bound to the SHA-256 of its complete `AdapterInvocationPlan`, and may not select another Method, capability offering, Asset, adapter, MaterialLot, or Procedure implementation.

`lab.adapter-invocations.v1` retains each selected Method's explicit completion dependencies, input and output ports, and Procedure yields in addition to its tasks. A scheduler therefore follows exact selected value edges instead of correlating samples by display names or operation order.

Each `AllocatedExecutionGroup` names one or more complete Procedure tasks, every requirement owned by those tasks, and dependencies on other groups. Validation requires that every task and requirement in the invocation appears exactly once, that no task's atomic requirement formula is split between groups, and that the group graph is acyclic.

The schedule also records persistent physical locations using stable references to Method inputs, task outputs, and allocated material inputs. Resource IDs and addresses are adapter-defined because a well, carrier site, and fluidic port are different physical namespaces. The generic validator proves that every logical reference exists; the concrete adapter must prove that each resource and address exists in the checked implementation profile, has sufficient capacity, and preserves all canonical fluid-path and technique constraints.

One reviewed device document realizes one execution group. When one document names requirements from several tasks, the generic execution-plan builder emits one `Execute` node containing their union and maps every constituent task to that node for dependency analysis. Internal task edges collapse inside the node; dependencies outside the group remain explicit.

## Consequences

- Device batching is a checked compiler artifact rather than an incidental rendering optimization.
- Task, Requirement, material, and provenance identities survive fusion.
- Physical positions persist across setup, processing, and evidence generation.
- Adapters may decline fusion and emit one valid group per task.
- A fused run remains confined to one exact Asset and adapter invocation.
- Golden Gate can reproduce PUDU's resource schedule without making PUDU classes or OT-2 deck coordinates part of canonical Procedure semantics.
