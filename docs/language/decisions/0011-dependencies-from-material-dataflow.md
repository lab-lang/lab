# 0011: Artifact dependencies derive from typed material dataflow

Status: accepted, initial target lowering implemented

## Decision

Lab source expresses an artifact dependency as a typed material input to a workflow and as an operand of a resolved realization action. It does not assign artifacts to compiler-defined biological levels.

```lab
workflow realize_reporter(
  carrier: Material<Plasmid>,
) -> BuiltArtifact:
  dependencies = [carrier]
  product, construct <- realize reporter from dependencies
```

The workflow signature states what must already exist. The `Material<Plasmid>` value is affine, and the `realize` contract takes the dependency list. Checked IR therefore preserves both dependency identity and ownership transfer without target-specific graph annotations in the core language.

A target lowerer may derive graph edges, roots, build waves, cycles, retries, and blockers from that checked dataflow. Inventory may satisfy a node without executing its recipe, which cuts the corresponding execution path while leaving the source dependency relation intact.

## Boundary

The generic frontend resolves operations, checks types and ownership, and preserves dependency dataflow. It does not select Golden Gate, heat shock, deck layouts, reaction volumes, or any other hardware procedure. Those choices and their constraints belong to a narrow target specialization. A different target may lower the same source operations differently or reject unsupported properties and operation sequences explicitly.
