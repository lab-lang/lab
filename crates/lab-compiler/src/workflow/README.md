# Workflow dialect

The `workflow` dialect is LAIR's method-neutral procedure-intent layer. Every checked Lab action is represented by `workflow.perform`, whose serialized intent retains its exact action and operation definitions, typed arguments, ownership, named results, lineage, and source coordinates. Typed material and data use-def edges preserve action order without teaching the dialect individual scientific verbs.

Adding an action never adds a Rust operation or a dispatch arm. A package declares the action and its stable Intent operation; a registered Method with the same operation identity defines each available implementation. Artifact declarations still contribute design facts that Methods may require, while values stated directly by an action remain authoritative.

Workflow operations do not select a laboratory Method, inventory lot, offering, Asset, adapter, container, schedule, deck position, or robot command. Method refinement replaces every `workflow.perform` with candidate Procedure and Capability regions. Global facility planning selects one complete solution, and only verifier-valid Allocated Procedure LAIR may be projected into adapter invocations. Control constructs that Workflow LAIR cannot yet represent are rejected at their exact statement path; lowering never flattens their nested effects.
