# 0011: Artifact dependencies derive from typed material dataflow

Status: accepted, initial facility lowering implemented; amended by [0045](0045-lair-method-refinement-and-facility-allocation.md)

## Decision

Lab source expresses an artifact dependency as a typed material input to a workflow and as an operand of a resolved realization action. It does not assign artifacts to compiler-defined biological levels.

```lab
workflow assemble_reporter(
  carrier: Material<Plasmid>,
) -> Material<Plasmid>:
  dependencies = [carrier]
  product <- realize reporter from dependencies
```

The workflow signature states what must already exist. The `Material<Plasmid>` value is affine, and the `realize` contract takes the dependency list. Checked IR therefore preserves both dependency identity and ownership transfer without facility-specific graph annotations in the core language.

A facility-independent planner may derive graph edges, roots, build waves, cycles, retries, and blockers from that checked dataflow. Inventory may satisfy a node without executing its recipe, which cuts the corresponding execution path while leaving the source dependency relation intact.

## Boundary

The generic frontend resolves operations, checks types and ownership, and preserves dependency dataflow. Facility-independent method refinement may enumerate Golden Gate, another assembly method, or an offered assembly service, and may enumerate heat shock or another transformation method. Facility planning selects an unpinned method together with the exact offerings, Assets, adapters, and MaterialLots that make it feasible, after which the bound adapter chooses implementation-specific deck layouts and device operations. A different facility or explicit method policy may select a different supported refinement of the same source operation, while unsupported properties and operation sequences remain explicit diagnostics.
